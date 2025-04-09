use std::io;

const ID_READ: u8 = 0x0;
const ID_WRITE_TEXT: u8 = 0x1;

pub enum Command {
    Read,
    WriteText(Vec<u8>),
}

impl Command {
    #[cfg(feature = "frontend")]
    pub(crate) fn send<W>(&self, stream: &mut W) -> Result<(), io::Error>
    where
        W: io::Write,
    {
        match self {
            Self::Read => {
                let mut buf = [0u8; 1];
                buf[0] = ID_READ;
                stream.write_all(&buf)?;
            }
            Self::WriteText(d) => {
                let mut buf = [0u8; 1];
                buf[0] = ID_WRITE_TEXT;
                stream.write_all(&buf)?;

                let len = d.len();
                stream.write_all(&len.to_le_bytes())?;
                stream.write_all(d)?;
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
                let mut buf = [0u8; 8];
                stream.read_exact(&mut buf)?;
                let len = u64::from_le_bytes(buf);
                let len = usize::try_from(len)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

                let mut buf = vec![0u8; len];
                stream.read_exact(&mut buf)?;

                Ok(Self::WriteText(Vec::from(buf)))
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid command",
            )),
        }
    }
}

const ID_TEXT: u8 = 0x0;
const ID_FAILED: u8 = 0x1;
const ID_WRITE_DONE: u8 = 0x2;

pub enum Response {
    Text(Vec<u8>),
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
            Self::Text(s) => {
                let mut buf = [0u8; 1];
                buf[0] = ID_TEXT;
                stream.write_all(&buf)?;

                let len = s.len();
                stream.write_all(&len.to_le_bytes())?;
                stream.write_all(s)?;
            }
            Self::Failed => {
                let mut buf = [0u8; 1];
                buf[0] = ID_FAILED;
                stream.write_all(&buf)?;
            }
            Self::WriteDone => {
                let mut buf = [0u8; 1];
                buf[0] = ID_WRITE_DONE;
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
                let mut buf = [0u8; 8];
                stream.read_exact(&mut buf)?;
                let len = u64::from_le_bytes(buf);
                let len = usize::try_from(len)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

                let mut buf = vec![0u8; len];
                stream.read_exact(&mut buf)?;

                Ok(Self::Text(Vec::from(buf)))
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
