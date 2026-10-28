#[cfg(feature = "service-input")]
use crate::input;
use crate::service;
#[cfg(feature = "frontend")]
use std::sync;
use std::{fmt, io};

// Adjustments for Dynamic Virtual Channels

// This is the max size of what can be received
// over Dynamic Virtual Channels
pub const PDU_MAX_SIZE: usize = 1600;
// The DYNVC_DATA_FIRST's PDU header can be up to 10 bytes long
pub const PDU_DVC_HEADER_MAX_SIZE: usize = 10;
// This is the max size of data that can be sent in *any* kind of PDU
pub const PDU_DATA_MAX_SIZE: usize = PDU_MAX_SIZE - PDU_DVC_HEADER_MAX_SIZE;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    InvalidChunkType(u8),
    InvalidChunkSize(usize),
    PipelineBroken(String),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Io(e) => write!(fmt, "I/O error: {e}"),
            Self::InvalidChunkType(b) => {
                write!(fmt, "invalid chunk type: 0x{b:x}")
            }
            Self::InvalidChunkSize(s) => {
                write!(fmt, "invalid chunk size: 0x{s:x}")
            }
            Self::PipelineBroken(m) => write!(fmt, "broken pipeline: {m}"),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<crossbeam_channel::RecvError> for Error {
    fn from(e: crossbeam_channel::RecvError) -> Self {
        Self::PipelineBroken(e.to_string())
    }
}

impl<T> From<crossbeam_channel::SendError<T>> for Error {
    fn from(e: crossbeam_channel::SendError<T>) -> Self {
        Self::PipelineBroken(e.to_string())
    }
}

const ID_START: u8 = 0xF0;
const ID_DATA: u8 = 0xF1;
const ID_END: u8 = 0xF2;

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

pub type ClientId = u16;

#[cfg(feature = "frontend")]
static CLIENT_ID_COUNTER: sync::atomic::AtomicU16 = sync::atomic::AtomicU16::new(0);

#[cfg(feature = "frontend")]
pub(crate) fn new_client_id() -> ClientId {
    CLIENT_ID_COUNTER.fetch_add(1, sync::atomic::Ordering::Relaxed)
}

pub struct Chunk(Vec<u8>);

const SERIALIZE_OVERHEAD: usize = 2 /* ClientId */ + 1 /* ChunkType */ + 2 /* len */;

impl Chunk {
    fn new(
        chunk_type: ChunkType,
        client_id: ClientId,
        data: Option<&[u8]>,
    ) -> Result<Self, io::Error> {
        let payload_len = data.as_ref().map_or(0, |data| data.len());
        if payload_len > (PDU_DATA_MAX_SIZE - SERIALIZE_OVERHEAD) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "payload is too large!",
            ));
        }
        let mut content = vec![0u8; SERIALIZE_OVERHEAD + payload_len];
        let id_bytes = client_id.to_le_bytes();
        let mut offset = id_bytes.len();
        content[0..offset].copy_from_slice(&id_bytes);
        content[offset] = chunk_type.serialized();
        offset += 1;
        if let Some(data) = data {
            let payload_len_u16 = u16::try_from(payload_len)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
            let payload_len_bytes = u16::to_le_bytes(payload_len_u16);
            content[offset..offset + payload_len_bytes.len()].copy_from_slice(&payload_len_bytes);
            offset += payload_len_bytes.len();
            content[offset..offset + payload_len].copy_from_slice(data);
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
        let bytes = [self.0[0], self.0[1]];
        u16::from_le_bytes(bytes)
    }

    pub fn chunk_type(&self) -> Result<ChunkType, Error> {
        match self.0[2] {
            ID_START => Ok(ChunkType::Start),
            ID_DATA => Ok(ChunkType::Data),
            ID_END => Ok(ChunkType::End),
            b => Err(Error::InvalidChunkType(b)),
        }
    }

    fn payload_len(&self) -> u16 {
        let data_len_bytes = [self.0[3], self.0[4]];
        u16::from_le_bytes(data_len_bytes)
    }

    pub const fn can_deserialize_from(data: &[u8]) -> Option<usize> {
        let len = data.len();
        if len < SERIALIZE_OVERHEAD {
            return None;
        }
        let payload_len_bytes = [data[3], data[4]];
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
        if !(SERIALIZE_OVERHEAD..=PDU_DATA_MAX_SIZE).contains(&len) {
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
        PDU_DATA_MAX_SIZE - SERIALIZE_OVERHEAD
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

pub enum Message {
    Chunk(Chunk),
    #[cfg(feature = "service-input")]
    InputSetting(input::InputSetting),
    #[cfg(feature = "service-input")]
    InputAction(input::InputAction),
    #[cfg(feature = "service-input")]
    ResetClient,
    Shutdown,
}
