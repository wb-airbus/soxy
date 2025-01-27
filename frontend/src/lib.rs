use common::{
    api, clipboard, command, ftp,
    service::{self, Frontend},
    socks5,
};
use std::{fmt, io, net, sync, thread};

mod control;
mod svc;
#[cfg(target_os = "windows")]
mod windows;

enum Error {
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

pub(crate) fn init() -> Result<(), Error> {
    if SVC_TO_CONTROL.get().is_some() {
        return Ok(());
    }

    if let Err(e) = common::init_logs() {
        eprintln!("failed to initialize log: {e}");
    }

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

    thread::Builder::new()
        .name("services".into())
        .spawn(move || {
            thread::scope(|scope| {
                thread::Builder::new()
                    .name(format!("{}", api::Service::Clipboard))
                    .spawn_scoped(scope, || {
                        if let Err(e) = server_clipboard.start(&services) {
                            common::error!("{} error: {e}", api::Service::Clipboard);
                        } else {
                            common::debug!("{} terminated", api::Service::Clipboard);
                        }
                    })
                    .unwrap();

                thread::Builder::new()
                    .name(format!("{}", api::Service::Command))
                    .spawn_scoped(scope, || {
                        if let Err(e) = server_command.start(&services) {
                            common::error!("{} error: {e}", api::Service::Command);
                        } else {
                            common::debug!("{} terminated", api::Service::Command);
                        }
                    })
                    .unwrap();

                thread::Builder::new()
                    .name(format!("{}", api::Service::Ftp))
                    .spawn_scoped(scope, || {
                        if let Err(e) = server_ftp.start(&services) {
                            common::error!("{} error: {e}", api::Service::Ftp);
                        } else {
                            common::debug!("{} terminated", api::Service::Ftp);
                        }
                    })
                    .unwrap();

                thread::Builder::new()
                    .name(format!("{}", api::Service::Socks5))
                    .spawn_scoped(scope, || {
                        if let Err(e) = server_socks5.start(&services) {
                            common::error!("{} error: {e}", api::Service::Socks5);
                        } else {
                            common::debug!("{} terminated", api::Service::Socks5);
                        }
                    })
                    .unwrap();

                if let Err(e) = services.start(api::ServiceKind::Frontend, &svc_to_frontend_receive)
                {
                    common::error!("frontend_to_services error: {e}");
                } else {
                    common::error!("frontend_to_services terminated");
                }
            });
        })
        .unwrap();

    control.start();

    Ok(())
}

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
