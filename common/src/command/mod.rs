#[cfg(feature = "frontend")]
use crate::frontend as sfrontend;
use crate::service;

#[cfg(feature = "backend")]
mod backend;
#[cfg(feature = "frontend")]
mod frontend;

pub static SERVICE: service::Service = service::Service {
    internal: false,
    name: "command",
    #[cfg(feature = "frontend")]
    frontend: Some(sfrontend::Frontend {
        tcp: Some(sfrontend::FrontendTcp {
            default_port: 3031,
            handler: frontend::tcp_frontend_handler,
        }),
    }),
    #[cfg(feature = "backend")]
    backend: Some(service::Backend {
        handler: backend::backend_handler,
    }),
};
