use crate::util;
use std::io;

const ID_READ: u8 = 0x0;
const ID_WRITE_TEXT: u8 = 0x1;

pub enum Command {
    Read,
    WriteText(String),
}

impl Command {
    #[cfg(feature = "frontend")]
    pub(crate) fn send<W>(&self, stream: &mut W) -> Result<(), io::Error>
    where
        W: io::Write,
    {
        match self {
            Self::Read => {
                let buf = [ID_READ; 1];
                stream.write_all(&buf)?;
            }
            Self::WriteText(t) => {
                let buf = [ID_WRITE_TEXT; 1];
                stream.write_all(&buf)?;

                util::serialize_string(stream, t)?;
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
            ID_READ => Ok(Self::Read),
            ID_WRITE_TEXT => {
                let text = util::deserialize_string(stream)?;
                Ok(Self::WriteText(text))
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

const ID_TEXT: u8 = 0x0;
const ID_FAILED: u8 = 0x1;
const ID_WRITE_DONE: u8 = 0x2;

pub enum Response {
    Text(String),
    Failed,
    WriteDone,
}

impl Response {
    #[cfg(feature = "backend")]
    pub(crate) fn send<W>(&self, stream: &mut W) -> Result<(), io::Error>
    where
        W: io::Write,
    {
        match self {
            Self::Text(t) => {
                let buf = [ID_TEXT; 1];
                stream.write_all(&buf)?;

                util::serialize_string(stream, t)?;
            }
            Self::Failed => {
                let buf = [ID_FAILED; 1];
                stream.write_all(&buf)?;
            }
            Self::WriteDone => {
                let buf = [ID_WRITE_DONE; 1];
                stream.write_all(&buf)?;
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
            ID_TEXT => {
                let t = util::deserialize_string(stream)?;
                Ok(Self::Text(t))
            }
            ID_FAILED => Ok(Self::Failed),
            ID_WRITE_DONE => Ok(Self::WriteDone),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid response",
            )),
        }
    }
}
