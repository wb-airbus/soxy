use crate::{api, clipboard, ftp, socks5};
use std::{
    collections::{self, hash_map},
    io::{self, Write},
    net, sync, thread,
};

const CLIENT_CHUNK_BUFFER_SIZE: usize = 16;

pub struct Channel {
    clients:
        sync::RwLock<collections::HashMap<api::ClientId, crossbeam_channel::Sender<api::Chunk>>>,
    to_rdp: crossbeam_channel::Sender<api::ChunkControl>,
}

impl Channel {
    pub fn new(to_rdp: crossbeam_channel::Sender<api::ChunkControl>) -> Self {
        Self {
            clients: sync::RwLock::new(collections::HashMap::new()),
            to_rdp,
        }
    }

    pub fn shutdown(&self) {
        match self.clients.write() {
            sync::LockResult::Err(e) => {
                crate::error!("failed to acquire lock to shutdown channel: {e}");
            }
            sync::LockResult::Ok(mut clients) => {
                clients.iter().for_each(|(client_id, client)| {
                    let _ = client.send(api::Chunk::end(*client_id));
                });
                clients.clear();
            }
        }
    }

    fn forget(&self, client_id: api::ClientId) {
        let _ = self.clients.write().unwrap().remove(&client_id);
    }

    fn send(&self, chunk: api::Chunk) -> Result<(), api::Error> {
        self.to_rdp.send(api::ChunkControl::Chunk(chunk))?;
        Ok(())
    }

    pub fn connect(&self, service: api::Service) -> Result<RdpStream, io::Error> {
        let client_id = api::new_client_id();

        let (from_rdp_send, from_rdp_recv) = crossbeam_channel::bounded(CLIENT_CHUNK_BUFFER_SIZE);

        self.clients
            .write()
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))?
            .insert(client_id, from_rdp_send);

        let stream = RdpStream::new(self, service, client_id, from_rdp_recv);
        match stream.connect() {
            Err(e) => {
                self.forget(client_id);
                Err(e)
            }
            Ok(()) => Ok(stream),
        }
    }

    fn handle_backend_start<'a>(
        &'a self,
        service_kind: api::ServiceKind,
        client_id: api::ClientId,
        payload: &[u8],
        scope: &'a thread::Scope<'a, '_>,
    ) -> Result<(), api::Error> {
        match self
            .clients
            .write()
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))?
            .entry(client_id)
        {
            hash_map::Entry::Occupied(_) => {
                crate::error!("discarding start for already existing client {client_id:x}");
            }
            hash_map::Entry::Vacant(ve) => match api::Service::try_from(payload) {
                Err(service) => {
                    crate::error!("new client for unknown service {service}!");
                    self.send(api::Chunk::end(client_id))?;
                }
                Ok(service) => {
                    crate::debug!("new {service} client {client_id:x}");

                    let (from_rdp_send, from_rdp_recv) =
                        crossbeam_channel::bounded(CLIENT_CHUNK_BUFFER_SIZE);
                    ve.insert(from_rdp_send);

                    let stream = RdpStream::new(self, service, client_id, from_rdp_recv);
                    stream.accept()?;

                    thread::Builder::new()
                        .name(format!("{service_kind} {service} {client_id:x}"))
                        .spawn_scoped(scope, move || match service {
                            api::Service::Clipboard => {
                                if let Err(e) = clipboard::backend::Server::accept(stream) {
                                    crate::debug!("error: {e}");
                                }
                            }
                            api::Service::Ftp => {
                                if let Err(e) = ftp::backend::Server::accept(stream) {
                                    crate::error!("error: {e}");
                                }
                            }
                            api::Service::Socks5 => {
                                if let Err(e) = socks5::backend::Server::accept(stream) {
                                    crate::error!("error: {e}");
                                }
                            }
                        })
                        .unwrap();
                }
            },
        }

        Ok(())
    }

    pub fn start(
        &self,
        service_kind: api::ServiceKind,
        from_rdp: &crossbeam_channel::Receiver<api::ChunkControl>,
    ) -> Result<(), api::Error> {
        thread::scope(|scope| loop {
            let control_chunk = from_rdp.recv()?;

            match control_chunk {
                api::ChunkControl::Shutdown => {
                    self.shutdown();
                }
                api::ChunkControl::Chunk(chunk) => match chunk.chunk_type() {
                    Err(_) => {
                        crate::error!("discarding invalid chunk: {chunk}");
                    }
                    Ok(chunk_type) => {
                        let client_id = chunk.client_id();
                        let payload = chunk.payload();

                        match chunk_type {
                            api::ChunkType::Start => match service_kind {
                                api::ServiceKind::Frontend => {
                                    unimplemented!("accept connections");
                                }
                                api::ServiceKind::Backend => {
                                    self.handle_backend_start(
                                        service_kind,
                                        client_id,
                                        payload,
                                        scope,
                                    )?;
                                }
                            },
                            api::ChunkType::Data => {
                                if let Some(client) = self
                                    .clients
                                    .read()
                                    .map_err(|e| {
                                        io::Error::new(io::ErrorKind::BrokenPipe, e.to_string())
                                    })?
                                    .get(&client_id)
                                {
                                    if client.send(chunk).is_err() {
                                        crate::warn!(
                                            "error sending to disconnected client {client_id:x}"
                                        );
                                    }
                                } else {
                                    crate::debug!(
                                        "discarding chunk for unknown client {client_id:x}"
                                    );
                                    let _ = self.send(api::Chunk::end(client_id));
                                }
                            }
                            api::ChunkType::End => {
                                if let Some(client) = self
                                    .clients
                                    .write()
                                    .map_err(|e| {
                                        io::Error::new(io::ErrorKind::BrokenPipe, e.to_string())
                                    })?
                                    .remove(&client_id)
                                {
                                    if client.send(chunk).is_err() {
                                        crate::warn!(
                                            "error sending to disconnected client {client_id:x}"
                                        );
                                    }
                                } else {
                                    crate::debug!(
                                        "discarding chunk for unknown client {client_id:x}"
                                    );
                                }
                            }
                        }
                    }
                },
            }
        })
    }
}

enum RdpStreamState {
    Ready,
    Connected,
    Disconnected,
}

impl RdpStreamState {
    const fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }
}

struct RdpStreamCommon<'a> {
    channel: &'a Channel,
    service: api::Service,
    client_id: api::ClientId,
    state: RdpStreamState,
}

impl RdpStreamCommon<'_> {
    fn accept(&mut self) -> Result<(), io::Error> {
        match &self.state {
            RdpStreamState::Ready => {
                crate::debug!("{} accept {:x}", self.service, self.client_id);
                self.state = RdpStreamState::Connected;
                Ok(())
            }
            RdpStreamState::Connected => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "already connected",
            )),
            RdpStreamState::Disconnected => {
                Err(io::Error::new(io::ErrorKind::Interrupted, "disconnected"))
            }
        }
    }

    fn connect(&mut self) -> Result<(), io::Error> {
        match &self.state {
            RdpStreamState::Ready => {
                self.channel
                    .send(api::Chunk::start(self.client_id, self.service)?)
                    .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))?;
                crate::debug!("connect",);
                self.state = RdpStreamState::Connected;
                Ok(())
            }
            RdpStreamState::Connected => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "already connected",
            )),
            RdpStreamState::Disconnected => {
                Err(io::Error::new(io::ErrorKind::Interrupted, "disconnected"))
            }
        }
    }

    fn disconnected(&mut self) {
        crate::debug!("disconnected",);
        self.channel.forget(self.client_id);
        self.state = RdpStreamState::Disconnected;
    }

    fn disconnect(&mut self) {
        match &self.state {
            RdpStreamState::Ready => {
                self.disconnected();
            }
            RdpStreamState::Connected => {
                crate::debug!("disconnecting",);
                let _ = self.channel.send(api::Chunk::end(self.client_id));
                self.disconnected();
            }
            RdpStreamState::Disconnected => (),
        }
    }
}

impl Drop for RdpStreamCommon<'_> {
    fn drop(&mut self) {
        self.disconnect();
    }
}

#[derive(Clone)]
struct RdpStreamControl<'a>(sync::Arc<sync::RwLock<RdpStreamCommon<'a>>>);

impl<'a> RdpStreamControl<'a> {
    fn new(channel: &'a Channel, service: api::Service, client_id: api::ClientId) -> Self {
        Self(sync::Arc::new(sync::RwLock::new(RdpStreamCommon {
            channel,
            service,
            client_id,
            state: RdpStreamState::Ready,
        })))
    }

    fn client_id(&self) -> api::ClientId {
        self.0.read().unwrap().client_id
    }

    fn service(&self) -> api::Service {
        self.0.read().unwrap().service
    }

    fn is_connected(&self) -> bool {
        self.0.read().unwrap().state.is_connected()
    }

    fn accept(&self) -> Result<(), io::Error> {
        self.0.write().unwrap().accept()
    }

    fn connect(&self) -> Result<(), io::Error> {
        self.0.write().unwrap().connect()
    }

    fn send(&self, chunk: api::Chunk) -> Result<(), io::Error> {
        self.0
            .write()
            .unwrap()
            .channel
            .send(chunk)
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))
    }

    fn disconnected(&mut self) {
        self.0.write().unwrap().disconnected();
    }

    fn disconnect(&self) {
        self.0.write().unwrap().disconnect();
    }
}

pub struct RdpStream<'a> {
    reader: RdpReader<'a>,
    writer: RdpWriter<'a>,
    control: RdpStreamControl<'a>,
}

impl<'a> RdpStream<'a> {
    fn new(
        channel: &'a Channel,
        service: api::Service,
        client_id: api::ClientId,
        from_rdp: crossbeam_channel::Receiver<api::Chunk>,
    ) -> Self {
        let control = RdpStreamControl::new(channel, service, client_id);

        let reader = RdpReader::new(control.clone(), from_rdp);
        let writer = RdpWriter::new(control.clone());

        Self {
            reader,
            writer,
            control,
        }
    }

    pub fn client_id(&self) -> api::ClientId {
        self.control.client_id()
    }

    pub fn service(&self) -> api::Service {
        self.control.service()
    }

    pub fn accept(&self) -> Result<(), io::Error> {
        self.control.accept()
    }

    pub fn connect(&self) -> Result<(), io::Error> {
        self.control.connect()
    }

    pub fn disconnect(&mut self) -> Result<(), io::Error> {
        self.writer.flush()?;
        self.control.disconnect();
        Ok(())
    }

    pub fn split(self) -> (RdpReader<'a>, RdpWriter<'a>) {
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
    control: RdpStreamControl<'a>,
    from_rdp: crossbeam_channel::Receiver<api::Chunk>,
    last: Option<(api::Chunk, usize)>,
}

impl<'a> RdpReader<'a> {
    fn new(
        control: RdpStreamControl<'a>,
        from_rdp: crossbeam_channel::Receiver<api::Chunk>,
    ) -> Self {
        Self {
            control,
            from_rdp,
            last: None,
        }
    }

    pub(crate) fn disconnect(&self) {
        self.control.disconnect();
    }
}

impl io::Read for RdpReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        if !self.control.is_connected() {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "ended"));
        }

        if self.last.is_none() {
            let chunk = self
                .from_rdp
                .recv()
                .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))?;
            let chunk_type = chunk.chunk_type();
            let payload = chunk.payload();
            let payload_len = payload.len();
            if matches!(chunk_type, Ok(api::ChunkType::End)) {
                self.control.disconnected();
                return Ok(0);
            }
            if payload_len == 0 {
                return Ok(0);
            }
            if payload_len <= buf.len() {
                buf[0..payload_len].copy_from_slice(payload);
                return Ok(payload_len);
            }
            self.last = Some((chunk, 0));
        }

        let (last, last_offset) = self.last.as_mut().unwrap();
        let last_payload = last.payload();
        let last_payload_len = last_payload.len();
        let last_len = last_payload_len - *last_offset;
        let buf_len = buf.len();

        if last_len <= buf_len {
            buf[0..last_len].copy_from_slice(&last_payload[*last_offset..]);
            self.last = None;
            return Ok(last_len);
        }

        buf.copy_from_slice(&last_payload[*last_offset..*last_offset + buf_len]);
        *last_offset += buf_len;

        Ok(buf_len)
    }
}

pub struct RdpWriter<'a> {
    control: RdpStreamControl<'a>,
    buffer: [u8; api::Chunk::max_payload_length()],
    buffer_len: usize,
}

impl<'a> RdpWriter<'a> {
    fn new(control: RdpStreamControl<'a>) -> Self {
        Self {
            control,
            buffer: [0u8; api::Chunk::max_payload_length()],
            buffer_len: 0,
        }
    }

    pub(crate) fn disconnect(&mut self) -> Result<(), io::Error> {
        self.flush()?;
        self.control.disconnect();
        Ok(())
    }
}

impl Drop for RdpWriter<'_> {
    fn drop(&mut self) {
        let _ = self.flush();
        let _ = self.buffer;
        let _ = self.control;
    }
}

impl io::Write for RdpWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        if !self.control.is_connected() {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "ended"));
        }

        let buf_len = buf.len();
        let remaining_len = self.buffer.len() - self.buffer_len;

        if buf_len <= remaining_len {
            self.buffer[self.buffer_len..(self.buffer_len + buf_len)].copy_from_slice(buf);
            self.buffer_len += buf_len;
            if self.buffer.len() == self.buffer_len {
                self.flush()?;
            }
            Ok(buf_len)
        } else {
            self.buffer[self.buffer_len..].copy_from_slice(&buf[0..remaining_len]);
            self.buffer_len += remaining_len;

            self.flush()?;

            if remaining_len < buf_len {
                let len = usize::min(buf_len - remaining_len, self.buffer.len());
                self.buffer[0..len].copy_from_slice(&buf[remaining_len..(remaining_len + len)]);
                self.buffer_len = len;
                Ok(remaining_len + len)
            } else {
                Ok(remaining_len)
            }
        }
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        if 0 < self.buffer_len {
            let chunk =
                api::Chunk::data(self.control.client_id(), &self.buffer[0..self.buffer_len])?;
            self.buffer_len = 0;

            if let Err(e) = self.control.send(chunk) {
                self.control.disconnected();
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, e));
            }
        }
        Ok(())
    }
}

pub(crate) fn stream_copy<R, W>(from: &mut R, to: &mut W) -> Result<(), io::Error>
where
    R: io::Read,
    W: io::Write,
{
    let mut buf = vec![0u8; CLIENT_CHUNK_BUFFER_SIZE * api::Chunk::max_payload_length()];

    loop {
        let read = from.read(&mut buf)?;
        if read == 0 {
            return Ok(());
        }
        to.write_all(&buf[0..read])?;
        to.flush()?;
        thread::yield_now();
    }
}

pub trait Frontend: Sized {
    fn bind(tcp: net::SocketAddr) -> Result<Self, io::Error>;

    fn start(&mut self, channel: &Channel) -> Result<(), io::Error>;
}

pub trait Backend: Sized {
    fn accept(stream: RdpStream<'_>) -> Result<(), io::Error>;
}
