use common::{api, service};
use std::{fmt, io, net, str::FromStr, sync, thread};

mod config;
mod control;
mod svc;
#[cfg(target_os = "windows")]
mod windows;

pub enum Error {
    Api(api::Error),
    Config(config::Error),
    Io(io::Error),
    PipelineBroken,
    Svc(svc::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::Api(e) => write!(f, "API error: {e}"),
            Self::Config(e) => write!(f, "configuration error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::PipelineBroken => write!(f, "broken pipeline"),
            Self::Svc(e) => write!(f, "virtual channel error: {e}"),
        }
    }
}

impl From<api::Error> for Error {
    fn from(e: api::Error) -> Self {
        Self::Api(e)
    }
}

impl From<config::Error> for Error {
    fn from(e: config::Error) -> Self {
        Self::Config(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<crossbeam_channel::RecvError> for Error {
    fn from(_e: crossbeam_channel::RecvError) -> Self {
        Self::PipelineBroken
    }
}

impl<T> From<crossbeam_channel::SendError<T>> for Error {
    fn from(_e: crossbeam_channel::SendError<T>) -> Self {
        Self::PipelineBroken
    }
}

impl From<svc::Error> for Error {
    fn from(e: svc::Error) -> Self {
        Self::Svc(e)
    }
}

pub(crate) static SVC_TO_CONTROL: sync::OnceLock<crossbeam_channel::Sender<svc::Response>> =
    sync::OnceLock::new();

fn svc_commander(control: &crossbeam_channel::Receiver<svc::Command>) -> Result<(), Error> {
    loop {
        match control.recv()? {
            svc::Command::Open => {
                if let Some(svc) = svc::SVC.write().unwrap().as_mut() {
                    if let Err(e) = svc.open() {
                        common::error!("SVC open failed: {e}");
                    }
                } else {
                    common::error!("SVC not initialized");
                }
            }
            svc::Command::SendChunk(chunk) => {
                if let Some(svc) = svc::SVC.read().unwrap().as_ref() {
                    if let Err(e) = svc.write(chunk.serialized()) {
                        common::error!("SVC write failed: {e}");
                    }
                } else {
                    common::error!("SVC not initialized");
                }
            }
            svc::Command::Close => {
                if let Some(svc) = svc::SVC.write().unwrap().as_mut() {
                    if let Err(e) = svc.close() {
                        common::error!("SVC close failed: {e}");
                    }
                } else {
                    common::error!("SVC not initialized");
                }
            }
        }
    }
}

#[allow(clippy::missing_panics_doc)]
pub fn init(
    frontend_channel: service::Channel,
    backend_to_frontend: crossbeam_channel::Receiver<api::ChunkControl>,
) -> Result<(), Error> {
    let config = match config::Config::read()? {
        None => {
            let config = config::Config::default();
            config.save()?;
            config
        }
        Some(config) => config,
    };

    common::init_logs(config.log_level(), config.log_file());

    common::debug!("initializing frontend");

    let servers = config.services.into_iter().filter(|s| s.enabled).try_fold(
        vec![],
        |mut servers, service| {
            let ip = net::IpAddr::from_str(&service.ip.unwrap_or(config.ip.clone()))
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
            let port = service.port;
            let service = service::lookup(service.name.as_str())
                .ok_or(Error::Config(config::Error::UnknownService(service.name)))?;
            let port = port
                .or(service
                    .tcp_frontend()
                    .map(service::TcpFrontend::default_port))
                .ok_or(Error::Config(config::Error::UnknownService(
                    service.name().to_string(),
                )))?;

            let sockaddr = net::SocketAddr::new(ip, port);
            let server = common::service::TcpFrontendServer::bind(service, sockaddr)?;

            servers.push(server);

            Ok::<_, Error>(servers)
        },
    )?;

    thread::Builder::new()
        .name("frontend".into())
        .spawn(move || {
            thread::scope(|scope| {
                for server in &servers {
                    thread::Builder::new()
                        .name(server.service().name().to_string())
                        .spawn_scoped(scope, || {
                            if let Err(e) = server.start(&frontend_channel) {
                                common::error!("{} error: {e}", server.service().name());
                            } else {
                                common::debug!("{} terminated", server.service().name());
                            }
                        })
                        .unwrap();
                }

                if let Err(e) =
                    frontend_channel.start(service::Kind::Frontend, &backend_to_frontend)
                {
                    common::error!("frontend error: {e}");
                } else {
                    common::debug!("frontend terminated");
                }
            });
        })
        .unwrap();

    Ok(())
}

pub(crate) fn start() {
    if SVC_TO_CONTROL.get().is_some() {
        return;
    }

    common::debug!("initializing frontend");

    let (
        control,
        frontend_to_svc_send,
        svc_to_frontend_receive,
        control_to_svc_send,
        control_to_svc_receive,
    ) = control::Control::new();

    SVC_TO_CONTROL.get_or_init(|| control_to_svc_send);

    thread::Builder::new()
        .name("svc_commander".into())
        .spawn(move || {
            if let Err(e) = svc_commander(&control_to_svc_receive) {
                common::error!("svc_commander error: {e}");
            }
            common::debug!("svc_commander terminated");
        })
        .unwrap();

    let services = service::Channel::new(frontend_to_svc_send);

    if let Err(e) = init(services, svc_to_frontend_receive) {
        common::error!("init error: {e}");
    }

    thread::Builder::new()
        .name("control".into())
        .spawn(|| control.start())
        .unwrap();
}
