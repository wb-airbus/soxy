use crate::{input, service};
#[cfg(feature = "frontend")]
use std::sync;
use std::{fmt, io};

pub const CHUNK_LENGTH: usize = 1600; // this is the max value

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

#[cfg(feature = "frontend")]
static CLIENT_ID_COUNTER: sync::atomic::AtomicU32 = sync::atomic::AtomicU32::new(0);

#[cfg(feature = "frontend")]
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

    pub fn start(client_id: ClientId, service: &service::Service) -> Result<Self, io::Error> {
        Self::new(ChunkType::Start, client_id, Some(service.name().as_bytes()))
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

    pub fn can_deserialize_from(data: &[u8]) -> Option<usize> {
        let len = data.len();
        if len < SERIALIZE_OVERHEAD {
            return None;
        }
        let payload_len_bytes = [data[5], data[6]];
        let payload_len = u16::from_le_bytes(payload_len_bytes);
        let expected_len = SERIALIZE_OVERHEAD + payload_len as usize;
        if len < expected_len {
            return None;
        }
        Some(expected_len)
    }

    pub fn deserialize_from(data: &[u8]) -> Result<Self, Error> {
        let content = Vec::from(data);
        Self::deserialize(content)
    }

    pub fn deserialize(content: Vec<u8>) -> Result<Self, Error> {
        let len = content.len();
        if !(SERIALIZE_OVERHEAD..=CHUNK_LENGTH).contains(&len) {
            return Err(Error::InvalidChunkSize(len));
        }
        let res = Self(content);
        if SERIALIZE_OVERHEAD + res.payload_len() as usize == len {
            Ok(res)
        } else {
            Err(Error::InvalidChunkSize(len))
        }
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

pub enum ChannelControl {
    SendChunk(Chunk),
    SendInputSetting(input::InputSetting),
    SendInputAction(input::InputAction),
    ResetClient,
    Shutdown,
}
