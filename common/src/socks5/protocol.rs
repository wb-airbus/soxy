use crate::util;
use std::io;
#[cfg(feature = "frontend")]
use std::net;

#[cfg(feature = "frontend")]
pub const VERSION: u8 = 0x05;
#[cfg(feature = "frontend")]
pub const AUTHENTICATION_NONE: u8 = 0x00;

const ID_CMD_CONNECT: u8 = 0xC1;
const ID_CMD_BIND: u8 = 0xC2;

#[cfg(feature = "frontend")]
pub enum Error {
    Io(io::Error),
    UnsupportedVersion(u8),
    UnsupportedCommand(u8),
    AddressTypeNotSupported(u8),
}

#[cfg(feature = "frontend")]
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Debug)]
pub enum Command {
    Connect(String),
    Bind,
}

impl Command {
    #[cfg(feature = "frontend")]
    pub(crate) fn read<R>(reader: &mut R) -> Result<Self, Error>
    where
        R: io::Read,
    {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;

        if buf[0] != VERSION {
            let ret = buf[0];
            //let buf = [buf[0], 0x07, 0x00];
            //self.stream.write_all(&buf)?;
            //self.stream.flush()?;
            return Err(Error::UnsupportedVersion(ret));
        }

        // server reserved byte must be 0
        /*
        if buf[2] != 0x00 {
            todo!("invalid reserved field value (!= 0)")
        }
         */

        let dest: String = match buf[3] {
            // ipv4
            0x01 => {
                let mut buf = [0u8; 4];
                reader.read_exact(&mut buf)?;
                let ip = u32::from_be_bytes(buf);
                let ip = net::Ipv4Addr::from_bits(ip);
                let ip = net::IpAddr::V4(ip);

                let mut buf = [0u8; 2];
                reader.read_exact(&mut buf)?;
                let port = u16::from_be_bytes(buf);

                crate::info!("connect to {ip}:{port}");

                format!("{ip}:{port}")
            }
            // domain name
            0x03 => {
                let mut len = [0u8; 1];
                reader.read_exact(&mut len)?;
                let mut buf = vec![0x0; len[0] as usize];
                reader.read_exact(&mut buf)?;
                let name = String::from_utf8_lossy(&buf).to_string();

                let mut buf = [0u8; 2];
                reader.read_exact(&mut buf)?;
                let port = u16::from_be_bytes(buf);

                crate::info!("connect to {name}:{port}",);

                format!("{name}:{port}")
            }
            // ipv6
            0x04 => {
                let mut buf = [0u8; 16];
                reader.read_exact(&mut buf)?;
                let ip = u128::from_be_bytes(buf);
                let ip = net::Ipv6Addr::from_bits(ip);
                let ip = net::IpAddr::V6(ip);

                let mut buf = [0u8; 2];
                reader.read_exact(&mut buf)?;
                let port = u16::from_be_bytes(buf);

                crate::info!("connect to {ip}:{port}",);

                format!("{ip}:{port}")
            }
            t => return Err(Error::AddressTypeNotSupported(t)),
        };

        crate::trace!("READ {buf:?}");

        match buf[1] {
            // CONNECT
            0x01 => Ok(Self::Connect(dest)),

            // BIND
            0x02 => Ok(Self::Bind),

            c => Err(Error::UnsupportedCommand(c)),
        }
    }

    #[cfg(feature = "frontend")]
    pub(crate) fn send<W>(&self, stream: &mut W) -> Result<(), io::Error>
    where
        W: io::Write,
    {
        match self {
            Self::Connect(to_tcp) => {
                let buf = [ID_CMD_CONNECT; 1];
                stream.write_all(&buf)?;

                util::serialize_string(stream, to_tcp)?;
            }
            Self::Bind => {
                let buf = [ID_CMD_BIND; 1];
                stream.write_all(&buf)?;
            }
        }
        stream.flush()
    }

    #[cfg(feature = "backend")]
    pub(crate) fn receive<R>(stream: &mut R) -> Result<Self, io::Error>
    where
        R: io::Read,
    {
        let mut buf = [0u8; 1];
        stream.read_exact(&mut buf)?;

        match buf[0] {
            ID_CMD_CONNECT => {
                let to_tcp = util::deserialize_string(stream)?;
                Ok(Self::Connect(to_tcp))
            }
            ID_CMD_BIND => Ok(Self::Bind),
            v => unimplemented!("unsupported socks command {v}"),
        }
    }
}

const ID_RESP_OK: u8 = 0xD0;
const ID_RESP_NETWORK_UNREACHABLE: u8 = 0xD1;
const ID_RESP_HOST_UNREACHABLE: u8 = 0xD2;
const ID_RESP_CONNECTION_REFUSED: u8 = 0xD3;
const ID_RESP_BIND_FAILED: u8 = 0xD4;

#[derive(Debug)]
pub enum Response {
    Ok(Vec<u8>),
    NetworkUnreachable,
    HostUnreachable,
    ConnectionRefused,
    BindFailed,
}

#[cfg(feature = "frontend")]
const RSP_OK: u8 = 0x00;
#[cfg(feature = "frontend")]
const RSP_GENERAL_SOCKS_SERVER_FAILURE: u8 = 0x01;
//const RSP_CONNECTION_NOT_ALLOWED: u8 = 0x02;
#[cfg(feature = "frontend")]
const RSP_NETWORK_UNREACHABLE: u8 = 0x03;
#[cfg(feature = "frontend")]
const RSP_HOST_UNREACHABLE: u8 = 0x04;
#[cfg(feature = "frontend")]
const RSP_CONNECTION_REFUSED: u8 = 0x05;
//const RSP_TTL_EXPIRED: u8 = 0x06;
//const RSP_COMMAND_NOT_SUPPORTED: u8 = 0x07;
//const RSP_ADDRESS_TYPE_NOT_SUPPORTED: u8 = 0x08;

impl Response {
    #[cfg(feature = "frontend")]
    pub const fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    #[cfg(feature = "frontend")]
    pub(crate) fn answer_to_client<W>(&self, writer: &mut W) -> Result<(), io::Error>
    where
        W: io::Write,
    {
        match self {
            Self::NetworkUnreachable => {
                let buf = [
                    VERSION,
                    RSP_NETWORK_UNREACHABLE,
                    0x00,
                    0x01,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                ];
                writer.write_all(&buf)?;
            }
            Self::HostUnreachable => {
                let buf = [
                    VERSION,
                    RSP_HOST_UNREACHABLE,
                    0x00,
                    0x01,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                ];
                writer.write_all(&buf)?;
            }
            Self::ConnectionRefused => {
                let buf = [
                    VERSION,
                    RSP_CONNECTION_REFUSED,
                    0x00,
                    0x01,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                ];
                writer.write_all(&buf)?;
            }
            Self::BindFailed => {
                let buf = [
                    VERSION,
                    RSP_GENERAL_SOCKS_SERVER_FAILURE,
                    0x00,
                    0x01,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                ];
                writer.write_all(&buf)?;
            }
            Self::Ok(data) => {
                writer.write_all(&[VERSION, RSP_OK, 0x00])?;
                writer.write_all(data)?;
            }
        }
        writer.flush()
    }

    #[cfg(feature = "backend")]
    pub(crate) fn send<W>(&self, stream: &mut W) -> Result<(), io::Error>
    where
        W: io::Write,
    {
        let (id, data) = match self {
            Self::Ok(data) => (ID_RESP_OK, Some(data)),
            Self::NetworkUnreachable => (ID_RESP_NETWORK_UNREACHABLE, None),
            Self::HostUnreachable => (ID_RESP_HOST_UNREACHABLE, None),
            Self::ConnectionRefused => (ID_RESP_CONNECTION_REFUSED, None),
            Self::BindFailed => (ID_RESP_BIND_FAILED, None),
        };
        let buf = [id; 1];
        stream.write_all(&buf)?;
        if let Some(data) = data {
            let len = u32::try_from(data.len())
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
            stream.write_all(&len.to_le_bytes())?;
            stream.write_all(data)?;
        }
        stream.flush()
    }

    #[cfg(feature = "frontend")]
    pub(crate) fn receive<R>(stream: &mut R) -> Result<Self, io::Error>
    where
        R: io::Read,
    {
        let mut buf = [0u8; 1];
        stream.read_exact(&mut buf)?;
        let code = buf[0];

        match code {
            ID_RESP_OK => {
                let mut buf = [0u8; 4];
                stream.read_exact(&mut buf)?;
                let len = u32::from_le_bytes(buf);

                let mut data = vec![0u8; len as usize];
                stream.read_exact(&mut data)?;

                Ok(Self::Ok(data))
            }
            ID_RESP_NETWORK_UNREACHABLE => Ok(Self::NetworkUnreachable),
            ID_RESP_HOST_UNREACHABLE => Ok(Self::HostUnreachable),
            ID_RESP_CONNECTION_REFUSED => Ok(Self::ConnectionRefused),
            ID_RESP_BIND_FAILED => Ok(Self::BindFailed),
            v => unimplemented!("unsupported socks response {v}"),
        }
    }
}
