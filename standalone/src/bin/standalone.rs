use common::{
    api, clipboard, ftp,
    service::{self, Frontend},
    socks5,
};
use std::{net, thread};

const CHANNEL_SIZE: usize = 256;

fn main() {
    common::init_logs().expect("failed to initialize log");

    let from_tcp_clipboard =
        net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), 3032);
    let from_tcp_ftp =
        net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), 2021);
    let from_tcp_socks5 =
        net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), 1080);

    let mut frontend_server_clipboard = match clipboard::frontend::Server::bind(from_tcp_clipboard)
    {
        Err(e) => {
            common::error!("failed to bind to {from_tcp_clipboard}: {e}");
            return;
        }
        Ok(frontend_server) => frontend_server,
    };

    let mut frontend_server_ftp = match ftp::frontend::Server::bind(from_tcp_ftp) {
        Err(e) => {
            common::error!("failed to bind to {from_tcp_ftp}: {e}");
            return;
        }
        Ok(frontend_server) => frontend_server,
    };

    let mut frontend_server_socks5 = match socks5::frontend::Server::bind(from_tcp_socks5) {
        Err(e) => {
            common::error!("failed to bind to {from_tcp_socks5}: {e}");
            return;
        }
        Ok(frontend_server) => frontend_server,
    };

    let (frontend_to_backend_send, frontend_to_backend_receive) =
        crossbeam_channel::bounded(CHANNEL_SIZE);
    let (backend_to_frontend_send, backend_to_frontend_receive) =
        crossbeam_channel::bounded(CHANNEL_SIZE);

    let backend_channel = service::Channel::new(backend_to_frontend_send);
    let frontend_channel = service::Channel::new(frontend_to_backend_send);

    thread::scope(|scope| {
        thread::Builder::new()
            .name("backend".into())
            .spawn_scoped(scope, || {
                if let Err(e) =
                    backend_channel.start(api::ServiceKind::Backend, &frontend_to_backend_receive)
                {
                    common::error!("backend error: {e}");
                } else {
                    common::debug!("backend has stopped");
                }
            })
            .unwrap();

        thread::Builder::new()
            .name("frontend".into())
            .spawn_scoped(scope, || {
                if let Err(e) =
                    frontend_channel.start(api::ServiceKind::Frontend, &backend_to_frontend_receive)
                {
                    common::error!("frontend error: {e}");
                } else {
                    common::debug!("frontend has stopped");
                }
            })
            .unwrap();

        thread::Builder::new()
            .name(format!("frontend {}", api::Service::Clipboard))
            .spawn_scoped(scope, || {
                if let Err(e) = frontend_server_clipboard.start(&frontend_channel) {
                    common::error!("error: {e}");
                } else {
                    common::debug!("stopped");
                }
            })
            .unwrap();

        thread::Builder::new()
            .name(format!("frontend {}", api::Service::Ftp))
            .spawn_scoped(scope, || {
                if let Err(e) = frontend_server_ftp.start(&frontend_channel) {
                    common::error!("error: {e}");
                } else {
                    common::debug!("stopped");
                }
            })
            .unwrap();

        thread::Builder::new()
            .name(format!("frontend {}", api::Service::Socks5))
            .spawn_scoped(scope, || {
                if let Err(e) = frontend_server_socks5.start(&frontend_channel) {
                    common::error!("error: {e}");
                } else {
                    common::debug!("stopped");
                }
            })
            .unwrap();
    });
}
