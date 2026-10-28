use super::protocol;
use crate::{rdp, service, util};
use std::{
    io::{self, Write},
    net,
};

const SERVICE_KIND: service::Kind = service::Kind::Backend;

fn encode_addr(addr: &net::SocketAddr) -> Result<Vec<u8>, io::Error> {
    let mut data = Vec::with_capacity(192);

    match addr {
        net::SocketAddr::V4(ipv4) => {
            data.write_all(&[1u8; 1])?;
            data.write_all(&ipv4.ip().octets())?;
        }
        net::SocketAddr::V6(ipv6) => {
            data.write_all(&[4u8; 1])?;
            data.write_all(&ipv6.ip().octets())?;
        }
    }
    data.write_all(&addr.port().to_be_bytes())?;

    Ok(data)
}

fn command_connect(mut stream: rdp::RdpStream<'_>, to_tcp: &str) -> Result<(), io::Error> {
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

            let data = encode_addr(&server.local_addr()?)?;
            protocol::Response::Ok(data).send(&mut stream)?;

            crate::debug!("starting stream copy");

            service::double_stream_copy(SERVICE_KIND, &super::SERVICE, stream, server, true)
        }
    }
}

fn command_bind(mut stream: rdp::RdpStream<'_>) -> Result<(), io::Error> {
    match util::find_best_address() {
        Err(e) => {
            crate::error!("failed to enumerate network interfaces: {e}");
            protocol::Response::NetworkUnreachable.send(&mut stream)
        }
        Ok(util::BestAddress { cidr4, cidr6 }) => {
            match cidr4
                .map(|(ip, _)| net::IpAddr::from(ip))
                .or(cidr6.map(|(ip, _)| net::IpAddr::from(ip)))
            {
                None => {
                    crate::error!("failed to find a suitable network interfaces");
                    protocol::Response::NetworkUnreachable.send(&mut stream)
                }
                Some(ip) => {
                    let from_tcp = net::SocketAddr::new(ip, 0);

                    crate::info!("binding to {from_tcp}");

                    match net::TcpListener::bind(from_tcp) {
                        Err(e) => {
                            crate::error!("failed to bind to {from_tcp:#?}: {e}");
                            protocol::Response::BindFailed.send(&mut stream)
                        }
                        Ok(server) => {
                            let data = encode_addr(&server.local_addr()?)?;
                            protocol::Response::Ok(data).send(&mut stream)?;

                            match server.accept() {
                                Err(e) => {
                                    crate::error!("failed to accept on {from_tcp:#?}: {e}");
                                    protocol::Response::BindFailed.send(&mut stream)
                                }
                                Ok((client, client_addr)) => {
                                    let data = encode_addr(&client_addr)?;
                                    protocol::Response::Ok(data).send(&mut stream)?;

                                    crate::debug!("starting stream copy");

                                    service::double_stream_copy(
                                        SERVICE_KIND,
                                        &super::SERVICE,
                                        stream,
                                        client,
                                        true,
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn handler(mut stream: rdp::RdpStream<'_>) -> Result<(), io::Error> {
    crate::debug!("starting");

    let cmd = protocol::Command::receive(&mut stream)?;

    match cmd {
        protocol::Command::Connect(to_tcp) => command_connect(stream, &to_tcp),
        protocol::Command::Bind => command_bind(stream),
    }
}
