use crate::{api, channel, service};

use std::{io, net, thread};

pub struct FrontendTcpServer {
    service: &'static service::Service,
    server: net::TcpListener,
    custom_data: Option<String>,
    pub(crate) ip: net::IpAddr,
}

impl FrontendTcpServer {
    pub const fn service(&self) -> &service::Service {
        self.service
    }

    pub(crate) const fn custom_data(&self) -> Option<&String> {
        self.custom_data.as_ref()
    }

    pub fn bind(
        service: &'static service::Service,
        tcp: net::SocketAddr,
        custom_data: Option<String>,
    ) -> Result<Self, io::Error> {
        let data = custom_data
            .as_ref()
            .map_or_else(String::new, |data| format!(" ({data})"));

        crate::info!("binding {service}{} clients on {tcp}", data);

        let server = net::TcpListener::bind(tcp)?;
        let ip = server.local_addr()?.ip();

        Ok(Self {
            service,
            server,
            custom_data,
            ip,
        })
    }

    pub fn start<'a>(&'a self, channel: &'a channel::Channel) -> Result<(), io::Error> {
        self.service
            .frontend()
            .and_then(Frontend::tcp)
            .map_or(Ok(()), |frontend_tcp| {
                thread::scope(|scope| {
                    loop {
                        let (client, client_addr) = self.server.accept()?;

                        crate::debug!("new client {client_addr}");

                        thread::Builder::new()
                            .name(format!(
                                "{} {} {client_addr}",
                                service::Kind::Frontend,
                                self.service
                            ))
                            .spawn_scoped(scope, move || {
                                if let Err(e) = (frontend_tcp.handler)(self, scope, client, channel)
                                {
                                    crate::debug!("error: {e}");
                                }
                            })?;
                    }
                })
            })
    }
}

type FrontendHandler<S, C> = for<'a> fn(
    server: &S,
    scope: &'a thread::Scope<'a, '_>,
    client: C,
    channel: &'a channel::Channel,
) -> Result<(), api::Error>;

type FrontendTcpHandler = FrontendHandler<FrontendTcpServer, net::TcpStream>;

pub struct FrontendTcp {
    pub default_port: u16,
    pub(crate) handler: FrontendTcpHandler,
}

pub struct Frontend {
    pub(crate) tcp: Option<FrontendTcp>,
}

impl Frontend {
    pub const fn tcp(&self) -> Option<&FrontendTcp> {
        self.tcp.as_ref()
    }
}
