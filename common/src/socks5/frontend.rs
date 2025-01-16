use super::protocol;
use crate::{api, service};
use std::{
    fmt,
    io::{self, Read, Write},
    net, thread,
};

const SERVICE: api::Service = api::Service::Socks5;
const SERVICE_KIND: api::ServiceKind = api::ServiceKind::Frontend;

#[derive(Debug)]
enum Error {
    UnsupportedVersion(u8),
    UnsupportedAuthentication(u8),
    Io(io::Error),
    UnsupportedCommand(u8),
    AddressTypeNotSupported(u8),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<protocol::Error> for Error {
    fn from(e: protocol::Error) -> Self {
        match e {
            protocol::Error::Io(e) => Self::Io(e),
            protocol::Error::UnsupportedVersion(v) => Self::UnsupportedVersion(v),
            protocol::Error::UnsupportedCommand(c) => Self::UnsupportedCommand(c),
            protocol::Error::AddressTypeNotSupported(t) => Self::AddressTypeNotSupported(t),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::UnsupportedVersion(v) => write!(f, "unsupported version {v}"),
            Self::UnsupportedAuthentication(v) => {
                write!(f, "unsupported authentication {v}")
            }
            Self::UnsupportedCommand(v) => write!(f, "unsupported command {v}"),
            Self::AddressTypeNotSupported(v) => write!(f, "address type not supported {v}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

pub struct Server {
    server: net::TcpListener,
    server_ip: net::IpAddr,
}

impl Server {
    fn accept(&self, stream: net::TcpStream) -> Client {
        Client::new(stream, self.server_ip)
    }
}

impl service::Frontend for Server {
    fn bind(tcp: net::SocketAddr) -> Result<Self, io::Error> {
        let server = net::TcpListener::bind(tcp)?;
        crate::info!("accepting {SERVICE} clients on {}", server.local_addr()?);
        let server_ip = server.local_addr()?.ip();
        Ok(Self { server, server_ip })
    }

    fn start(&mut self, channel: &service::Channel) -> Result<(), io::Error> {
        thread::scope(|scope| loop {
            let (client, client_addr) = self.server.accept()?;

            crate::debug!("new client {client_addr}");

            let client = self.accept(client);

            thread::Builder::new()
                .name(format!("{SERVICE_KIND} {SERVICE} {client_addr}"))
                .spawn_scoped(scope, move || {
                    if let Err(e) = client.start(channel) {
                        crate::error!("error: {e}");
                    }
                })?;
        })
    }
}

struct Client {
    stream: net::TcpStream,
    //server_ip: net::IpAddr,
}

impl Client {
    fn new(stream: net::TcpStream, _server_ip: net::IpAddr) -> Self {
        Self { stream } //, server_ip }
    }

    fn handshake(&mut self) -> Result<protocol::Command, Error> {
        // client greeting
        let mut buf = [0; 2];
        self.stream.read_exact(&mut buf)?;

        // client version ?
        if buf[0] != super::VERSION {
            return Err(Error::UnsupportedVersion(buf[0]));
        }

        let nb_auth = buf[1];

        // client proposed authentication methods
        let mut buf = vec![0; nb_auth as usize];
        self.stream.read_exact(&mut buf)?;

        // server supports only 0x0 NO AUTHENTICATION
        if !buf.into_iter().any(|b| b == protocol::AUTHENTICATION_NONE) {
            return Err(Error::UnsupportedAuthentication(
                protocol::AUTHENTICATION_NONE,
            ));
        }

        // server proposes NO AUTHENTICATION
        let buf = [super::VERSION, protocol::AUTHENTICATION_NONE];
        self.stream.write_all(&buf)?;
        self.stream.flush()?;

        Ok(protocol::Command::read(&mut self.stream)?)
    }

    fn command_connect(mut self, mut client_rdp: service::RdpStream<'_>) -> Result<(), io::Error> {
        let client_id = client_rdp.client_id();

        let resp = protocol::Response::receive(&mut client_rdp)?;
        resp.answer_to_client(&mut self.stream)?;

        if !resp.is_ok() {
            let _ = self.stream.shutdown(net::Shutdown::Both);
            return Ok(());
        }

        let (client_rdp_read, client_rdp_write) = client_rdp.split();

        let stream2 = self.stream.try_clone()?;

        thread::scope(|scope| {
            thread::Builder::new()
                .name(format!(
                    "{SERVICE_KIND} {SERVICE} {client_id:x} rdp to server"
                ))
                .spawn_scoped(scope, move || {
                    let mut client_rdp_read = io::BufReader::new(client_rdp_read);
                    let mut stream2 = io::BufWriter::new(stream2);
                    if let Err(e) = service::stream_copy(&mut client_rdp_read, &mut stream2) {
                        crate::debug!("error: {e}");
                    } else {
                        crate::debug!("stopped");
                    }
                    let _ = stream2.flush();
                    if let Ok(stream2) = stream2.into_inner() {
                        let _ = stream2.shutdown(net::Shutdown::Both);
                    }
                    let client_rdp_read = client_rdp_read.into_inner();
                    client_rdp_read.disconnect();
                })
                .unwrap();

            let mut client_rdp_write = io::BufWriter::new(client_rdp_write);
            let mut stream = io::BufReader::new(self.stream);
            if let Err(e) = service::stream_copy(&mut stream, &mut client_rdp_write) {
                crate::debug!("error: {e}");
            } else {
                crate::debug!("stopped");
            }
            let _ = client_rdp_write.flush();
            if let Ok(mut client_rdp_write) = client_rdp_write.into_inner() {
                let _ = client_rdp_write.disconnect();
            }
            let stream = stream.into_inner();
            let _ = stream.shutdown(net::Shutdown::Both);

            Ok(())
        })
    }

    fn command_bind(mut self, mut client_rdp: service::RdpStream<'_>) -> Result<(), io::Error> {
        let client_id = client_rdp.client_id();

        // for the bind operation on the backend
        let resp = protocol::Response::receive(&mut client_rdp)?;
        resp.answer_to_client(&mut self.stream)?;

        if !resp.is_ok() {
            let _ = self.stream.shutdown(net::Shutdown::Both);
            return Ok(());
        }

        // waiting for the connection of a client to the bounded port on the backend
        let resp = protocol::Response::receive(&mut client_rdp)?;
        resp.answer_to_client(&mut self.stream)?;

        if !resp.is_ok() {
            let _ = self.stream.shutdown(net::Shutdown::Both);
            return Ok(());
        }

        let (client_rdp_read, client_rdp_write) = client_rdp.split();

        let stream2 = self.stream.try_clone()?;

        thread::scope(|scope| {
            thread::Builder::new()
                .name(format!(
                    "{SERVICE_KIND} {SERVICE} {client_id:x} rdp to server"
                ))
                .spawn_scoped(scope, move || {
                    let mut client_rdp_read = io::BufReader::new(client_rdp_read);
                    let mut stream2 = io::BufWriter::new(stream2);
                    if let Err(e) = service::stream_copy(&mut client_rdp_read, &mut stream2) {
                        crate::debug!("error: {e}");
                    } else {
                        crate::debug!("stopped");
                    }
                    let _ = stream2.flush();
                    if let Ok(stream2) = stream2.into_inner() {
                        let _ = stream2.shutdown(net::Shutdown::Both);
                    }
                    let client_rdp_read = client_rdp_read.into_inner();
                    client_rdp_read.disconnect();
                })
                .unwrap();

            let mut stream = io::BufReader::new(self.stream);
            let mut client_rdp_write = io::BufWriter::new(client_rdp_write);
            if let Err(e) = service::stream_copy(&mut stream, &mut client_rdp_write) {
                crate::debug!("error: {e}");
            } else {
                crate::debug!("stopped");
            }
            let _ = client_rdp_write.flush();
            if let Ok(mut client_rdp_write) = client_rdp_write.into_inner() {
                let _ = client_rdp_write.disconnect();
            }
            let stream = stream.into_inner();
            let _ = stream.shutdown(net::Shutdown::Both);

            Ok(())
        })
    }

    fn start(mut self, channel: &service::Channel) -> Result<(), io::Error> {
        match self.handshake() {
            Err(e) => match e {
                Error::Io(e) => Err(e),
                Error::UnsupportedVersion(_) => {
                    let buf = [super::VERSION, 0xFF];
                    self.stream.write_all(&buf)?;
                    self.stream.flush()?;
                    Ok(())
                }
                Error::UnsupportedAuthentication(_) => {
                    let buf = [super::VERSION, 0xFF];
                    self.stream.write_all(&buf)?;
                    self.stream.flush()?;
                    Ok(())
                }
                Error::UnsupportedCommand(_) => {
                    let buf = [
                        super::VERSION,
                        0x07,
                        0x00,
                        0x01,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                    ];
                    self.stream.write_all(&buf)?;
                    self.stream.flush()?;
                    Ok(())
                }
                Error::AddressTypeNotSupported(_) => {
                    let buf = [
                        super::VERSION,
                        0x08,
                        0x00,
                        0x01,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                        0x00,
                    ];
                    self.stream.write_all(&buf)?;
                    self.stream.flush()?;
                    Ok(())
                }
            },
            Ok(command) => {
                let mut client_rdp = channel.connect(SERVICE)?;

                command.send(&mut client_rdp)?;

                match command {
                    protocol::Command::Connect(_) => self.command_connect(client_rdp),
                    protocol::Command::Bind => self.command_bind(client_rdp),
                }
            }
        }
    }
}
