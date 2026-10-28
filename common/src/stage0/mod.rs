#[cfg(feature = "frontend")]
use crate::frontend as sfrontend;
use crate::service;

#[cfg(feature = "frontend")]
mod frontend;

pub static SERVICE: service::Service = service::Service {
    internal: false,
    name: "stage0",
    #[cfg(feature = "frontend")]
    frontend: Some(sfrontend::Frontend {
        tcp: Some(sfrontend::FrontendTcp {
            default_port: 1082,
            handler: frontend::tcp_handler,
        }),
    }),
    #[cfg(feature = "backend")]
    backend: None,
};
