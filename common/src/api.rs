use std::{borrow, fmt, io, sync};

const CHUNK_LENGTH: usize = 1600; // this is the max value

const SERVICE_CLIPBOARD: &str = "clipboard";
const SERVICE_COMMAND: &str = "command";
const SERVICE_FTP: &str = "ftp";
const SERVICE_SOCKS5: &str = "socks5";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Service {
    Clipboard,
    Command,
    Ftp,
    Socks5,
}

impl Service {
    const fn value(self) -> &'static str {
        match self {
            Self::Clipboard => SERVICE_CLIPBOARD,
            Self::Command => SERVICE_COMMAND,
            Self::Ftp => SERVICE_FTP,
            Self::Socks5 => SERVICE_SOCKS5,
        }
    }

    const fn as_bytes(self) -> &'static [u8] {
        self.value().as_bytes()
    }
}

impl<'a> TryFrom<&'a [u8]> for Service {
    type Error = borrow::Cow<'a, str>;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        let s = String::from_utf8_lossy(value);
        match s.as_ref() {
            SERVICE_CLIPBOARD => Ok(Self::Clipboard),
            SERVICE_COMMAND => Ok(Self::Command),
            SERVICE_FTP => Ok(Self::Ftp),
            SERVICE_SOCKS5 => Ok(Self::Socks5),
            _ => Err(s),
        }
    }
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.value())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceKind {
    Backend,
    Frontend,
}

impl fmt::Display for ServiceKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Backend => write!(f, "backend"),
            Self::Frontend => write!(f, "frontend"),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    InvalidChunkType(Option<u8>),
    InvalidChunkSize(usize),
    PipelineBroken,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::InvalidChunkType(b) => {
                if let Some(b) = b {
                    write!(fmt, "invalid chunk type: 0x{b:x}")
                } else {
                    write!(fmt, "missing chunk type")
                }
            }
            Self::InvalidChunkSize(s) => {
                write!(fmt, "invalid chunk size: 0x{s:x}")
            }
            Self::PipelineBroken => write!(fmt, "broken pipeline"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<crossbeam_channel::RecvError> for Error {
    fn from(_e: crossbeam_channel::RecvError) -> Self {
        Self::PipelineBroken
    }
}

impl<T> From<crossbeam_channel::SendError<T>> for Error {
    fn from(_e: crossbeam_channel::SendError<T>) -> Self {
        Self::PipelineBroken
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChunkType {
    Start,
    Data,
    End,
}

impl ChunkType {
    const fn serialized(self) -> u8 {
        match self {
            Self::Start => ID_START,
            Self::Data => ID_DATA,
            Self::End => ID_END,
        }
    }
}

impl fmt::Display for ChunkType {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Start => write!(fmt, "Start"),
            Self::Data => write!(fmt, "Data"),
            Self::End => write!(fmt, "End"),
        }
    }
}

const ID_START: u8 = 0x00;
const ID_DATA: u8 = 0x01;
const ID_END: u8 = 0x02;

pub type ClientId = u32;

static CLIENT_ID_COUNTER: sync::atomic::AtomicU32 = sync::atomic::AtomicU32::new(0);

pub(crate) fn new_client_id() -> ClientId {
    CLIENT_ID_COUNTER.fetch_add(1, sync::atomic::Ordering::Relaxed)
}

pub struct Chunk(Vec<u8>);

const SERIALIZE_OVERHEAD: usize = 4 + 1 + 2;

impl Chunk {
    fn new(
        chunk_type: ChunkType,
        client_id: ClientId,
        data: Option<&[u8]>,
    ) -> Result<Self, io::Error> {
        let mut content = Vec::with_capacity(CHUNK_LENGTH);
        content.extend_from_slice(&client_id.to_le_bytes());
        content.push(chunk_type.serialized());
        if let Some(data) = data {
            let payload_len = data.len();
            if payload_len > (CHUNK_LENGTH - SERIALIZE_OVERHEAD) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "payload is too large!",
                ));
            }
            let payload_len = u16::try_from(payload_len)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
            content.extend_from_slice(&u16::to_le_bytes(payload_len));
            content.extend_from_slice(data);
        } else {
            content.extend_from_slice(&0u16.to_le_bytes());
        }
        Ok(Self(content))
    }

    pub fn start(client_id: ClientId, service: Service) -> Result<Self, io::Error> {
        Self::new(ChunkType::Start, client_id, Some(service.as_bytes()))
    }

    pub fn data(client_id: ClientId, data: &[u8]) -> Result<Self, io::Error> {
        Self::new(ChunkType::Data, client_id, Some(data))
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn end(client_id: ClientId) -> Self {
        Self::new(ChunkType::End, client_id, None).expect("infaillible")
    }

    pub fn client_id(&self) -> ClientId {
        let bytes = [self.0[0], self.0[1], self.0[2], self.0[3]];
        u32::from_le_bytes(bytes)
    }

    pub fn chunk_type(&self) -> Result<ChunkType, Error> {
        match self.0.get(4) {
            Some(&ID_START) => Ok(ChunkType::Start),
            Some(&ID_DATA) => Ok(ChunkType::Data),
            Some(&ID_END) => Ok(ChunkType::End),
            b => Err(Error::InvalidChunkType(b.copied())),
        }
    }

    fn payload_len(&self) -> u16 {
        let data_len_bytes = [self.0[5], self.0[6]];
        u16::from_le_bytes(data_len_bytes)
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, Error> {
        let len = data.len();
        if !(SERIALIZE_OVERHEAD..=CHUNK_LENGTH).contains(&len) {
            return Err(Error::InvalidChunkSize(len));
        }
        let mut content = Vec::with_capacity(len);
        content.extend_from_slice(data);
        Ok(Self(content))
    }

    pub const fn serialized_overhead() -> usize {
        SERIALIZE_OVERHEAD
    }

    pub const fn max_payload_length() -> usize {
        CHUNK_LENGTH - SERIALIZE_OVERHEAD
    }

    pub fn payload(&self) -> &[u8] {
        let len = usize::from(self.payload_len());
        &self.0[SERIALIZE_OVERHEAD..(SERIALIZE_OVERHEAD + len)]
    }

    pub fn serialized(self) -> Vec<u8> {
        self.0
    }
}

impl fmt::Display for Chunk {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            fmt,
            "client {:x} chunk_type = {} data = {} byte(s)",
            self.client_id(),
            self.chunk_type().map_err(|_| fmt::Error)?,
            self.payload_len()
        )
    }
}

pub enum ChunkControl {
    Chunk(Chunk),
    Shutdown,
}
