use crate::service;

#[cfg(feature = "backend")]
mod backend;
#[cfg(feature = "frontend")]
mod frontend;
mod protocol;

pub(crate) static SERVICE: service::Service = service::Service {
    name: "socks5",
    #[cfg(feature = "frontend")]
    tcp_frontend: Some(service::TcpFrontend {
        default_port: 1080,
        handler: frontend::tcp_handler,
    }),
    #[cfg(feature = "backend")]
    backend: service::Backend {
        handler: backend::handler,
    },
};
