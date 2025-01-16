use super::protocol;
use crate::{api, service};
use std::{
    io::{self, Write},
    net, thread,
};

const SERVICE: api::Service = api::Service::Socks5;
const SERVICE_KIND: api::ServiceKind = api::ServiceKind::Backend;

pub struct Server {}

impl Server {
    fn command_connect(mut stream: service::RdpStream<'_>, to_tcp: &str) -> Result<(), io::Error> {
        let client_id = stream.client_id();

        crate::info!("connecting to {to_tcp:#?}");

        match net::TcpStream::connect(to_tcp) {
            Err(e) => {
                crate::error!("failed to connect to {to_tcp:#?}: {e}");
                match e.kind() {
                    io::ErrorKind::ConnectionAborted | io::ErrorKind::TimedOut => {
                        protocol::Response::HostUnreachable.send(&mut stream)?;
                    }
                    io::ErrorKind::ConnectionRefused => {
                        protocol::Response::ConnectionRefused.send(&mut stream)?;
                    }
                    _ => {
                        crate::error!("failed to connect to {to_tcp:#?}: {e}");
                        protocol::Response::NetworkUnreachable.send(&mut stream)?;
                    }
                }
            }
            Ok(server) => {
                crate::debug!("connected to {to_tcp:#?}");

                let data = super::encode_addr(&server.local_addr()?)?;
                protocol::Response::Ok(data).send(&mut stream)?;

                crate::debug!("starting stream copy");

                let (stream_read, stream_write) = stream.split();

                let server2 = server.try_clone()?;

                thread::scope(|scope| {
                    thread::Builder::new()
                        .name(format!(
                            "{SERVICE_KIND} {SERVICE} {client_id:x} stream copy"
                        ))
                        .spawn_scoped(scope, move || {
                            let mut stream_read = io::BufReader::new(stream_read);
                            let mut server2 = io::BufWriter::new(server2);
                            if let Err(e) = service::stream_copy(&mut stream_read, &mut server2) {
                                crate::debug!("error: {e}");
                            } else {
                                crate::debug!("stopped");
                            }
                            let _ = server2.flush();
                            if let Ok(server2) = server2.into_inner() {
                                let _ = server2.shutdown(net::Shutdown::Both);
                            }
                            let stream_read = stream_read.into_inner();
                            stream_read.disconnect();
                        })
                        .unwrap();

                    let mut server = io::BufReader::new(server);
                    let mut stream_write = io::BufWriter::new(stream_write);
                    if let Err(e) = service::stream_copy(&mut server, &mut stream_write) {
                        crate::debug!("error: {e}");
                    } else {
                        crate::debug!("stopped");
                    }
                    let _ = stream_write.flush();
                    if let Ok(mut stream_write) = stream_write.into_inner() {
                        let _ = stream_write.disconnect();
                    }
                    let server = server.into_inner();
                    let _ = server.shutdown(net::Shutdown::Both);
                });
            }
        }

        Ok(())
    }

    fn command_bind(mut stream: service::RdpStream<'_>) -> Result<(), io::Error> {
        let client_id = stream.client_id();

        let local_ip = local_ip_address::local_ip().unwrap();
        let from_tcp = net::SocketAddr::new(local_ip, 0);

        crate::info!("binding to {from_tcp}");

        match net::TcpListener::bind(from_tcp) {
            Err(e) => {
                crate::error!("failed to bind to {from_tcp:#?}: {e}");
                protocol::Response::BindFailed.send(&mut stream)?;
            }
            Ok(server) => {
                let data = super::encode_addr(&server.local_addr()?)?;
                protocol::Response::Ok(data).send(&mut stream)?;

                match server.accept() {
                    Err(e) => {
                        crate::error!("failed to accept on {from_tcp:#?}: {e}");
                        protocol::Response::BindFailed.send(&mut stream)?;
                    }
                    Ok((client, client_addr)) => {
                        let data = super::encode_addr(&client_addr)?;
                        protocol::Response::Ok(data).send(&mut stream)?;

                        crate::debug!("starting stream copy");

                        let (stream_read, stream_write) = stream.split();

                        let client2 = client.try_clone()?;

                        thread::scope(|scope| {
                            thread::Builder::new()
                                .name(format!(
                                    "{SERVICE_KIND} {SERVICE} {client_id:x} stream copy"
                                ))
                                .spawn_scoped(scope, move || {
                                    let mut stream_read = io::BufReader::new(stream_read);
                                    let mut client2 = io::BufWriter::new(client2);
                                    if let Err(e) =
                                        service::stream_copy(&mut stream_read, &mut client2)
                                    {
                                        crate::debug!("error: {e}");
                                    } else {
                                        crate::debug!("stopped");
                                    }
                                    let _ = client2.flush();
                                    if let Ok(client2) = client2.into_inner() {
                                        let _ = client2.shutdown(net::Shutdown::Both);
                                    }
                                    let stream_read = stream_read.into_inner();
                                    stream_read.disconnect();
                                })
                                .unwrap();

                            let mut client = io::BufReader::new(client);
                            let mut stream_write = io::BufWriter::new(stream_write);
                            if let Err(e) = service::stream_copy(&mut client, &mut stream_write) {
                                crate::debug!("error: {e}");
                            } else {
                                crate::debug!("stopped");
                            }
                            let _ = stream_write.flush();
                            if let Ok(mut stream_write) = stream_write.into_inner() {
                                let _ = stream_write.disconnect();
                            }
                            let client = client.into_inner();
                            let _ = client.shutdown(net::Shutdown::Both);
                        });
                    }
                }
            }
        }

        Ok(())
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
