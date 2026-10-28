use super::protocol;
use crate::{api, channel, frontend, rdp, service};
use std::{
    fmt,
    io::{self, Read, Write},
    net, thread,
};

const SERVICE_KIND: service::Kind = service::Kind::Frontend;

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

fn handshake(stream: &mut net::TcpStream) -> Result<protocol::Command, Error> {
    // client greeting
    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf)?;

    // client version ?
    if buf[0] != protocol::VERSION {
        return Err(Error::UnsupportedVersion(buf[0]));
    }

    let nb_auth = buf[1];

    // client proposed authentication methods
    let mut buf = vec![0u8; nb_auth as usize];
    stream.read_exact(&mut buf)?;

    // server supports only 0x0 NO AUTHENTICATION
    if !buf.into_iter().any(|b| b == protocol::AUTHENTICATION_NONE) {
        return Err(Error::UnsupportedAuthentication(
            protocol::AUTHENTICATION_NONE,
        ));
    }

    // server proposes NO AUTHENTICATION
    let buf = [protocol::VERSION, protocol::AUTHENTICATION_NONE];
    stream.write_all(&buf)?;
    stream.flush()?;

    Ok(protocol::Command::read(stream)?)
}

fn command_connect(
    mut stream: net::TcpStream,
    mut client_rdp: rdp::RdpStream<'_>,
) -> Result<(), io::Error> {
    let resp = protocol::Response::receive(&mut client_rdp)?;
    resp.answer_to_client(&mut stream)?;

    if !resp.is_ok() {
        return Ok(());
    }

    crate::debug!("starting stream copy");

    service::double_stream_copy(SERVICE_KIND, &super::SERVICE, client_rdp, stream, true)
}

fn command_bind(
    mut stream: net::TcpStream,
    mut client_rdp: rdp::RdpStream<'_>,
) -> Result<(), io::Error> {
    // for the bind operation on the backend
    let resp = protocol::Response::receive(&mut client_rdp)?;
    resp.answer_to_client(&mut stream)?;

    if !resp.is_ok() {
        return Ok(());
    }

    // waiting for the connection of a client to the bounded port on the backend
    let resp = protocol::Response::receive(&mut client_rdp)?;
    resp.answer_to_client(&mut stream)?;

    if !resp.is_ok() {
        return Ok(());
    }

    crate::debug!("starting stream copy");

    service::double_stream_copy(SERVICE_KIND, &super::SERVICE, client_rdp, stream, true)
}

pub fn tcp_handler(
    _server: &frontend::FrontendTcpServer,
    _scope: &thread::Scope,
    mut stream: net::TcpStream,
    channel: &channel::Channel,
) -> Result<(), api::Error> {
    match handshake(&mut stream) {
        Err(e) => match e {
            Error::Io(e) => Err(api::Error::Io(e)),
            Error::UnsupportedVersion(_) | Error::UnsupportedAuthentication(_) => {
                let buf = [protocol::VERSION, 0xFF];
                stream.write_all(&buf)?;
                stream.flush()?;
                Ok(())
            }
            Error::UnsupportedCommand(_) => {
                let buf = [
                    protocol::VERSION,
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
                stream.write_all(&buf)?;
                stream.flush()?;
                Ok(())
            }
            Error::AddressTypeNotSupported(_) => {
                let buf = [
                    protocol::VERSION,
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
                stream.write_all(&buf)?;
                stream.flush()?;
                Ok(())
            }
        },
        Ok(command) => {
            let mut client_rdp = channel.connect(&super::SERVICE)?;

            command.send(&mut client_rdp)?;

            match command {
                protocol::Command::Connect(_) => Ok(command_connect(stream, client_rdp)?),
                protocol::Command::Bind => Ok(command_bind(stream, client_rdp)?),
            }
        }
    }
}
