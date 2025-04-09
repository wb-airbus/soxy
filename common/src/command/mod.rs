use crate::service;

#[cfg(feature = "backend")]
mod backend;
#[cfg(feature = "frontend")]
mod frontend;

pub(crate) static SERVICE: service::Service = service::Service {
    name: "command",
    #[cfg(feature = "frontend")]
    tcp_frontend: Some(service::TcpFrontend {
        default_port: 3031,
        handler: frontend::tcp_frontend_handler,
    }),
    #[cfg(feature = "backend")]
    backend: service::Backend {
        handler: backend::backend_handler,
    },
};
