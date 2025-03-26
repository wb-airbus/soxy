use common::{
    api, clipboard, command, ftp,
    service::{self, Frontend},
    socks5, stage0,
};
use std::{fmt, io, net, sync, thread};

mod control;
mod svc;
#[cfg(target_os = "windows")]
mod windows;

pub enum Error {
    Svc(svc::Error),
    Io(io::Error),
    PipelineBroken,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::Svc(e) => write!(f, "virtual channel error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::PipelineBroken => write!(f, "broken pipeline"),
        }
    }
}

impl From<svc::Error> for Error {
    fn from(e: svc::Error) -> Self {
        Self::Svc(e)
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
    common::init_logs(true, None);

    common::debug!("initializing frontend");

    let from_tcp_clipboard =
        net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), 3032);
    //let timeout_clipboard = None;
    let mut server_clipboard = clipboard::frontend::Server::bind(from_tcp_clipboard)?;

    let from_tcp_command =
        net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), 3031);
    //let timeout_command = None;
    let mut server_command = command::frontend::Server::bind(from_tcp_command)?;

    let from_tcp_ftp =
        net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), 2021);
    //let timeout_ftp = None;
    let mut server_ftp = ftp::frontend::Server::bind(from_tcp_ftp)?;

    let from_tcp_socks5 =
        net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), 1080);
    //let timeout_socks5 = None;
    let mut server_socks5 = socks5::frontend::Server::bind(from_tcp_socks5)?;

    let from_tcp_stage0 =
        net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), 1081);
    //let timeout_stage0 = None;
    let mut server_stage0 = stage0::frontend::Server::bind(from_tcp_stage0)?;

    thread::Builder::new()
        .name("frontend".into())
        .spawn(move || {
            thread::scope(|scope| {
                thread::Builder::new()
                    .name(format!("{}", api::Service::Clipboard))
                    .spawn_scoped(scope, || {
                        if let Err(e) = server_clipboard.start(&frontend_channel) {
                            common::error!("{} error: {e}", api::Service::Clipboard);
                        } else {
                            common::debug!("{} terminated", api::Service::Clipboard);
                        }
                    })
                    .unwrap();

                thread::Builder::new()
                    .name(format!("{}", api::Service::Command))
                    .spawn_scoped(scope, || {
                        if let Err(e) = server_command.start(&frontend_channel) {
                            common::error!("{} error: {e}", api::Service::Command);
                        } else {
                            common::debug!("{} terminated", api::Service::Command);
                        }
                    })
                    .unwrap();

                thread::Builder::new()
                    .name(format!("{}", api::Service::Ftp))
                    .spawn_scoped(scope, || {
                        if let Err(e) = server_ftp.start(&frontend_channel) {
                            common::error!("{} error: {e}", api::Service::Ftp);
                        } else {
                            common::debug!("{} terminated", api::Service::Ftp);
                        }
                    })
                    .unwrap();

                thread::Builder::new()
                    .name(format!("{}", api::Service::Socks5))
                    .spawn_scoped(scope, || {
                        if let Err(e) = server_socks5.start(&frontend_channel) {
                            common::error!("{} error: {e}", api::Service::Socks5);
                        } else {
                            common::debug!("{} terminated", api::Service::Socks5);
                        }
                    })
                    .unwrap();

                thread::Builder::new()
                    .name(format!("{}", api::Service::Stage0))
                    .spawn_scoped(scope, || {
                        if let Err(e) = server_stage0.start(&frontend_channel) {
                            common::error!("{} error: {e}", api::Service::Stage0);
                        } else {
                            common::debug!("{} terminated", api::Service::Stage0);
                        }
                    })
                    .unwrap();

                if let Err(e) =
                    frontend_channel.start(api::ServiceKind::Frontend, &backend_to_frontend)
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
