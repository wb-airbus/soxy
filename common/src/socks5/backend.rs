use super::protocol;
use crate::{api, service};
use std::{io, net};

const SERVICE: api::Service = api::Service::Socks5;
const SERVICE_KIND: api::ServiceKind = api::ServiceKind::Backend;

pub struct Server {}

impl Server {
    fn command_connect(mut stream: service::RdpStream<'_>, to_tcp: &str) -> Result<(), io::Error> {
        crate::info!("connecting to {to_tcp:#?}");

        match net::TcpStream::connect(to_tcp) {
            Err(e) => {
                crate::error!("failed to connect to {to_tcp:#?}: {e}");
                match e.kind() {
                    io::ErrorKind::ConnectionAborted | io::ErrorKind::TimedOut => {
                        protocol::Response::HostUnreachable.send(&mut stream)
                    }
                    io::ErrorKind::ConnectionRefused => {
                        protocol::Response::ConnectionRefused.send(&mut stream)
                    }
                    _ => {
                        crate::error!("failed to connect to {to_tcp:#?}: {e}");
                        protocol::Response::NetworkUnreachable.send(&mut stream)
                    }
                }
            }
            Ok(server) => {
                crate::debug!("connected to {to_tcp:#?}");

                let data = super::encode_addr(&server.local_addr()?)?;
                protocol::Response::Ok(data).send(&mut stream)?;

                crate::debug!("starting stream copy");

                service::double_stream_copy(SERVICE_KIND, SERVICE, stream, server)
            }
        }
    }

    fn command_bind(mut stream: service::RdpStream<'_>) -> Result<(), io::Error> {
        let local_ip = local_ip_address::local_ip().unwrap();
        let from_tcp = net::SocketAddr::new(local_ip, 0);

        crate::info!("binding to {from_tcp}");

        match net::TcpListener::bind(from_tcp) {
            Err(e) => {
                crate::error!("failed to bind to {from_tcp:#?}: {e}");
                protocol::Response::BindFailed.send(&mut stream)
            }
            Ok(server) => {
                let data = super::encode_addr(&server.local_addr()?)?;
                protocol::Response::Ok(data).send(&mut stream)?;

                match server.accept() {
                    Err(e) => {
                        crate::error!("failed to accept on {from_tcp:#?}: {e}");
                        protocol::Response::BindFailed.send(&mut stream)
                    }
                    Ok((client, client_addr)) => {
                        let data = super::encode_addr(&client_addr)?;
                        protocol::Response::Ok(data).send(&mut stream)?;

                        crate::debug!("starting stream copy");

                        service::double_stream_copy(SERVICE_KIND, SERVICE, stream, client)
                    }
                }
            }
        }
    }
}

impl service::Backend for Server {
    fn accept(mut stream: service::RdpStream<'_>) -> Result<(), io::Error> {
        crate::debug!("starting");

        let cmd = protocol::Command::receive(&mut stream)?;

        match cmd {
            protocol::Command::Connect(to_tcp) => Self::command_connect(stream, &to_tcp),
            protocol::Command::Bind => Self::command_bind(stream),
        }
    }
}
