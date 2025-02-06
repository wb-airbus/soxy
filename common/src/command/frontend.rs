use crate::{api, service};
use std::{io, net, thread};

const SERVICE: api::Service = api::Service::Command;
const SERVICE_KIND: api::ServiceKind = api::ServiceKind::Frontend;

pub struct Server {
    server: net::TcpListener,
}

impl Server {
    const fn accept(stream: net::TcpStream) -> Client {
        Client { stream }
    }
}

impl service::Frontend for Server {
    fn bind(tcp: net::SocketAddr) -> Result<Self, io::Error> {
        let server = net::TcpListener::bind(tcp)?;
        crate::info!("accepting {SERVICE} clients on {}", server.local_addr()?);
        Ok(Self { server })
    }

    fn start(&mut self, channel: &service::Channel) -> Result<(), io::Error> {
        thread::scope(|scope| loop {
            let (client, client_addr) = self.server.accept()?;

            crate::debug!("new client {client_addr}");

            let client = Self::accept(client);

            thread::Builder::new()
                .name(format!("{SERVICE_KIND} {SERVICE} {client_addr}"))
                .spawn_scoped(scope, move || {
                    if let Err(e) = client.start(channel) {
                        crate::debug!("error: {e}");
                    }
                })?;
        })
    }
}

struct Client {
    stream: net::TcpStream,
}

impl Client {
    fn start(self, channel: &service::Channel) -> Result<(), io::Error> {
        let client_rdp = channel.connect(SERVICE)?;
        service::double_stream_copy(SERVICE_KIND, SERVICE, client_rdp, self.stream)
    }
}
