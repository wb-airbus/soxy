use crate::service;

#[cfg(feature = "backend")]
mod backend;
#[cfg(feature = "frontend")]
mod frontend;

pub(crate) static SERVICE: service::Service = service::Service {
    name: "stage0",
    #[cfg(feature = "frontend")]
    tcp_frontend: Some(service::TcpFrontend {
        default_port: 1081,
        handler: frontend::tcp_handler,
    }),
    #[cfg(feature = "backend")]
    backend: service::Backend {
        handler: backend::handler,
    },
};
