#[cfg(feature = "frontend")]
use crate::frontend as sfrontend;
use crate::service;

#[cfg(feature = "backend")]
mod backend;
#[cfg(feature = "frontend")]
mod frontend;
mod protocol;

pub static SERVICE: service::Service = service::Service {
    internal: false,
    name: "clipboard",
    #[cfg(feature = "frontend")]
    frontend: Some(sfrontend::Frontend {
        tcp: Some(sfrontend::FrontendTcp {
            default_port: 3032,
            handler: frontend::tcp_handler,
        }),
    }),
    #[cfg(feature = "backend")]
    backend: Some(service::Backend {
        handler: backend::handler,
    }),
};
