use crate::{api, channel, service};

use std::{
    cmp, fmt,
    io::{self, Write},
    net, sync,
};

enum State {
    ReadWrite(crossbeam_channel::Receiver<api::Chunk>),
    ReadOnly(crossbeam_channel::Receiver<api::Chunk>),
    WriteOnly,
    Closed,
}

impl State {
    fn will_send(&self) -> Result<(), api::Error> {
        match self {
            Self::ReadWrite(_) | Self::WriteOnly => Ok(()),
            Self::ReadOnly(_) | Self::Closed => Err(api::Error::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                format!("cannot send in state {self}"),
            ))),
        }
    }

    fn will_receive(&self) -> Result<&crossbeam_channel::Receiver<api::Chunk>, api::Error> {
        match self {
            Self::ReadWrite(from_rdp) | Self::ReadOnly(from_rdp) => Ok(from_rdp),
            Self::WriteOnly | Self::Closed => Err(api::Error::Io(io::Error::new(
                io::ErrorKind::BrokenPipe,
                format!("cannot receive in state {self}"),
            ))),
        }
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::ReadWrite(_) => write!(f, "ReadWWrite"),
            Self::ReadOnly(_) => write!(f, "ReadOnly"),
            Self::WriteOnly => write!(f, "WriteOnly"),
            Self::Closed => write!(f, "Close"),
        }
    }
}

struct Handle<'a> {
    channel: &'a channel::Channel,
    service: &'a service::Service,
    client_id: api::ClientId,
    state: sync::RwLock<State>,
}

impl<'a> Handle<'a> {
    const fn new(
        channel: &'a channel::Channel,
        service: &'a service::Service,
        client_id: api::ClientId,
        from_rdp: crossbeam_channel::Receiver<api::Chunk>,
    ) -> Self {
        Self {
            channel,
            service,
            client_id,
            state: sync::RwLock::new(State::ReadWrite(from_rdp)),
        }
    }

    fn send(&self, chunk: api::Chunk) -> Result<(), api::Error> {
        self.state.read().unwrap().will_send()?;

        if matches!(chunk.chunk_type()?, api::ChunkType::End) {
            crate::debug!("RDP send End for {:x}", self.client_id);

            let mut state = self.state.write().unwrap();

            match &mut *state {
                State::ReadWrite(from_rdp) => {
                    *state = State::ReadOnly(from_rdp.clone());
                }
                State::WriteOnly => {
                    *state = State::Closed;
                }
                State::ReadOnly(_) | State::Closed => (),
            }
        }

        crate::trace!("RDP send {chunk}");

        self.channel.send_chunk(chunk)?;

        Ok(())
    }

    fn receive(&self) -> Result<api::Chunk, api::Error> {
        let chunk = self.state.read().unwrap().will_receive()?.recv()?;

        crate::trace!("RDP receive {chunk}");

        let chunk_type = chunk.chunk_type()?;

        if matches!(chunk_type, api::ChunkType::End) {
            crate::debug!("RDP received End for {:x}", self.client_id);

            let mut state = self.state.write().unwrap();

            match &mut *state {
                State::ReadWrite(_) => {
                    *state = State::WriteOnly;
                }
                State::ReadOnly(_) => {
                    *state = State::Closed;
                }
                State::WriteOnly | State::Closed => (),
            }
        }

        Ok(chunk)
    }

    fn close(&'a self, mode: net::Shutdown) {
        let mut state = self.state.write().unwrap();
        let mut send_end = false;
        match mode {
            net::Shutdown::Both => {
                match &mut *state {
                    State::ReadWrite(_) | State::WriteOnly => {
                        send_end = true;
                    }
                    State::ReadOnly(_) | State::Closed => {}
                }
                *state = State::Closed;
            }
            net::Shutdown::Read => match &mut *state {
                State::ReadWrite(_) => {
                    *state = State::WriteOnly;
                }
                State::ReadOnly(_) => {
                    *state = State::Closed;
                }
                State::WriteOnly | State::Closed => {}
            },
            net::Shutdown::Write => match &mut *state {
                State::ReadWrite(from_rdp) => {
                    send_end = true;
                    *state = State::ReadOnly(from_rdp.clone());
                }
                State::ReadOnly(_) => {
                    send_end = true;
                    *state = State::Closed;
                }
                State::WriteOnly | State::Closed => {}
            },
        }

        if send_end && let Err(e) = self.channel.send_chunk(api::Chunk::end(self.client_id)) {
            crate::debug!("failed to send end for {}: {e}", self.client_id);
        }
    }
}

impl Drop for Handle<'_> {
    fn drop(&mut self) {
        crate::trace!("!! DROP RDP handle");
        self.close(net::Shutdown::Both);
    }
}

pub struct RdpStream<'a> {
    handle: sync::Arc<Handle<'a>>,
    reader: RdpReader<'a>,
    writer: RdpWriter<'a>,
}

impl<'a> RdpStream<'a> {
    pub(crate) fn new(
        channel: &'a channel::Channel,
        service: &'a service::Service,
        client_id: api::ClientId,
        from_rdp: crossbeam_channel::Receiver<api::Chunk>,
    ) -> Self {
        let handle = sync::Arc::new(Handle::new(channel, service, client_id, from_rdp));
        let reader = RdpReader::new(handle.clone());
        let writer = RdpWriter::new(handle.clone());
        Self {
            handle,
            reader,
            writer,
        }
    }

    #[cfg(any(feature = "frontend", feature = "log"))]
    pub(crate) fn client_id(&self) -> api::ClientId {
        self.handle.client_id
    }

    #[cfg(feature = "backend")]
    pub(crate) fn accept(&self) {
        crate::trace!(
            "accepted {} 0x{:x}",
            self.handle.service,
            self.handle.client_id
        );
    }

    #[cfg(feature = "frontend")]
    pub(crate) fn connect(&self) -> Result<(), io::Error> {
        self.handle
            .send(api::Chunk::start(self.client_id(), self.handle.service)?)
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))
    }

    pub(crate) fn split(self) -> (RdpReader<'a>, RdpWriter<'a>) {
        (self.reader, self.writer)
    }
}

impl io::Read for RdpStream<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        self.reader.read(buf)
    }
}

impl io::Write for RdpStream<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        self.writer.flush()
    }
}

pub struct RdpReader<'a> {
    handle: sync::Arc<Handle<'a>>,
    read_pending: Option<(api::Chunk, usize)>,
}

impl<'a> RdpReader<'a> {
    const fn new(handle: sync::Arc<Handle<'a>>) -> Self {
        Self {
            handle,
            read_pending: None,
        }
    }
}

impl Drop for RdpReader<'_> {
    fn drop(&mut self) {
        crate::trace!("!! DROP RDP reader");
        self.handle.close(net::Shutdown::Read);
    }
}

impl io::Read for RdpReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        let buf_len = buf.len();

        if self.read_pending.is_none() {
            let chunk = self
                .handle
                .receive()
                .map_err(|e| io::Error::new(io::ErrorKind::ConnectionAborted, e.to_string()))?;
            let payload = chunk.payload();
            let payload_len = payload.len();

            if payload_len == 0 {
                return Ok(0);
            }

            self.read_pending = Some((chunk, 0));
        }

        let (last, last_offset) = self.read_pending.as_mut().unwrap();
        let last_payload = last.payload();
        let last_payload_len = last_payload.len();
        let last_len = last_payload_len - *last_offset;

        if last_len <= buf_len {
            buf[..last_len].copy_from_slice(&last_payload[*last_offset..]);
            self.read_pending = None;
            return Ok(last_len);
        }

        buf.copy_from_slice(&last_payload[*last_offset..*last_offset + buf_len]);
        *last_offset += buf_len;

        Ok(buf_len)
    }
}

#[derive(Clone)]
pub struct RdpWriter<'a> {
    handle: sync::Arc<Handle<'a>>,
    write_pending: Vec<u8>,
}

impl<'a> RdpWriter<'a> {
    fn new(handle: sync::Arc<Handle<'a>>) -> Self {
        Self {
            handle,
            write_pending: Vec::with_capacity(api::Chunk::max_payload_length()),
        }
    }
}

impl Drop for RdpWriter<'_> {
    fn drop(&mut self) {
        crate::trace!("!! DROP RDP writer");
        let _ = self.flush();
        self.handle.close(net::Shutdown::Write);
    }
}

impl io::Write for RdpWriter<'_> {
    fn write(&mut self, mut buf: &[u8]) -> Result<usize, io::Error> {
        crate::trace!("RDP write {} bytes", buf.len());

        let mut written = 0;

        if !self.write_pending.is_empty() {
            let buf_len = buf.len();
            let prev_len = self.write_pending.len();
            let remaining_len = self.write_pending.capacity() - prev_len;
            let can_write = usize::min(buf_len, remaining_len);

            self.write_pending.extend_from_slice(&buf[0..can_write]);

            match can_write.cmp(&remaining_len) {
                cmp::Ordering::Less => {
                    return Ok(can_write);
                }
                cmp::Ordering::Equal => {
                    self.flush()?;
                    return Ok(can_write);
                }
                cmp::Ordering::Greater => {
                    self.flush()?;
                    written = can_write;
                    buf = &buf[can_write..];
                }
            }
        }

        buf[written..]
            .chunks(api::Chunk::max_payload_length())
            .flat_map(|buf| api::Chunk::data(self.handle.client_id, buf))
            .try_fold(written, |written, chunk| {
                let chunk_len = chunk.payload().len();

                if chunk_len < self.write_pending.capacity() {
                    // last chunk
                    self.write_pending.extend_from_slice(chunk.payload());
                } else {
                    // chunk is expected size
                    self.handle.send(chunk).map_err(|e| {
                        io::Error::new(io::ErrorKind::ConnectionAborted, e.to_string())
                    })?;
                }

                Ok(written + chunk_len)
            })
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        crate::trace!("RDP flush {} bytes", self.write_pending.len());

        if !self.write_pending.is_empty() {
            let chunk = api::Chunk::data(self.handle.client_id, &self.write_pending)?;
            self.write_pending.clear();

            self.handle
                .send(chunk)
                .map_err(|e| io::Error::new(io::ErrorKind::ConnectionAborted, e.to_string()))?;
        }

        Ok(())
    }
}
