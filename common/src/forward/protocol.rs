use crate::util;
use std::io;

const ID_COMMAND_CONNECT: u8 = 0xF1;

pub enum Command {
    Connect(String),
}

impl Command {
    #[cfg(feature = "frontend")]
    pub(crate) fn send<W>(&self, stream: &mut W) -> Result<(), io::Error>
    where
        W: io::Write,
    {
        match self {
            Self::Connect(dest) => {
                let buf = [ID_COMMAND_CONNECT; 1];
                stream.write_all(&buf)?;
                util::serialize_string(stream, dest)?;
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
            ID_COMMAND_CONNECT => {
                let dest = util::deserialize_string(stream)?;
                Ok(Self::Connect(dest))
            }
            _ => {
                #[cfg(not(feature = "log"))]
                {
                    Err(io::Error::new(io::ErrorKind::InvalidData, ""))
                }

                #[cfg(feature = "log")]
                {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid command",
                    ))
                }
            }
        }
    }
}

const ID_RESPONSE_CONNECTED: u8 = 0xE0;
const ID_RESPONSE_ERROR: u8 = 0xE1;

pub enum Response {
    Connected,
    Error(String),
}

impl Response {
    #[cfg(feature = "backend")]
    pub(crate) fn send<W>(&self, stream: &mut W) -> Result<(), io::Error>
    where
        W: io::Write,
    {
        match self {
            Self::Connected => {
                let buf = [ID_RESPONSE_CONNECTED; 1];
                stream.write_all(&buf)?;
            }
            Self::Error(msg) => {
                let buf = [ID_RESPONSE_ERROR; 1];
                stream.write_all(&buf)?;
                util::serialize_string(stream, msg)?;
            }
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

        match buf[0] {
            ID_RESPONSE_CONNECTED => Ok(Self::Connected),
            ID_RESPONSE_ERROR => {
                let msg = util::deserialize_string(stream)?;
                Ok(Self::Error(msg))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid response",
            )),
        }
    }
}
