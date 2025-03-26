use crate::{api, clipboard, command, ftp, input, socks5, stage0};
#[cfg(feature = "backend")]
use std::collections::hash_map;
use std::{
    collections, fmt,
    io::{self, Write},
    net::{self, TcpStream},
    sync, thread,
};

const CLIENT_CHUNK_BUFFER_SIZE: usize = 16;

pub struct Channel {
    clients:
        sync::RwLock<collections::HashMap<api::ClientId, crossbeam_channel::Sender<api::Chunk>>>,
    to_rdp: crossbeam_channel::Sender<api::ChannelControl>,
}

impl Channel {
    pub fn new(to_rdp: crossbeam_channel::Sender<api::ChannelControl>) -> Self {
        Self {
            clients: sync::RwLock::new(collections::HashMap::new()),
            to_rdp,
        }
    }

    pub(crate) fn shutdown(&self) {
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

    fn send_chunk(&self, chunk: api::Chunk) -> Result<(), api::Error> {
        self.to_rdp.send(api::ChannelControl::SendChunk(chunk))?;
        Ok(())
    }

    #[cfg(feature = "frontend")]
    pub(crate) fn reset_client(&self) -> Result<(), api::Error> {
        self.to_rdp.send(api::ChannelControl::ResetClient)?;
        Ok(())
    }

    #[cfg(feature = "frontend")]
    pub(crate) fn send_input_setting(
        &self,
        setting: input::InputSetting,
    ) -> Result<(), api::Error> {
        self.to_rdp
            .send(api::ChannelControl::SendInputSetting(setting))?;
        Ok(())
    }

    #[cfg(feature = "frontend")]
    pub(crate) fn send_input_action(&self, action: input::InputAction) -> Result<(), api::Error> {
        self.to_rdp
            .send(api::ChannelControl::SendInputAction(action))?;
        Ok(())
    }

    #[cfg(feature = "frontend")]
    pub(crate) fn connect<'a>(&'a self, service: &'a Service) -> Result<RdpStream<'a>, io::Error> {
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

    #[cfg(feature = "backend")]
    fn handle_backend_start<'a>(
        &'a self,
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
            hash_map::Entry::Vacant(ve) => match lookup_bytes(payload) {
                Err(service) => {
                    crate::error!("new client for unknown service {service}!");
                    self.send_chunk(api::Chunk::end(client_id))?;
                }
                Ok(service) => {
                    crate::debug!("new {service} client {client_id:x}");

                    let (from_rdp_send, from_rdp_recv) =
                        crossbeam_channel::bounded(CLIENT_CHUNK_BUFFER_SIZE);
                    ve.insert(from_rdp_send);

                    let stream = RdpStream::new(self, service, client_id, from_rdp_recv);
                    stream.accept()?;

                    thread::Builder::new()
                        .name(format!("{} {service} {client_id:x}", Kind::Backend))
                        .spawn_scoped(scope, move || {
                            if let Err(e) = (service.backend.handler)(stream) {
                                crate::debug!("error: {e}");
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
        service_kind: Kind,
        from_rdp: &crossbeam_channel::Receiver<api::ChannelControl>,
    ) -> Result<(), api::Error> {
        thread::scope(|scope| loop {
            let control_chunk = from_rdp.recv()?;

            match control_chunk {
                api::ChannelControl::Shutdown => {
                    self.shutdown();
                }
                api::ChannelControl::ResetClient => {
                    crate::error!("discarding reset client request");
                }
                api::ChannelControl::SendInputSetting(_) => {
                    crate::error!("discarding input setting request");
                }
                api::ChannelControl::SendInputAction(_) => {
                    crate::error!("discarding input action request");
                }
                api::ChannelControl::SendChunk(chunk) => match chunk.chunk_type() {
                    Err(_) => {
                        crate::error!("discarding invalid chunk");
                    }
                    Ok(chunk_type) => {
                        let client_id = chunk.client_id();

                        match chunk_type {
                            api::ChunkType::Start => match service_kind {
                                #[cfg(feature = "frontend")]
                                Kind::Frontend => {
                                    let _ = scope;
                                    unimplemented!("accept connections");
                                }
                                #[cfg(feature = "backend")]
                                Kind::Backend => {
                                    let payload = chunk.payload();
                                    self.handle_backend_start(client_id, payload, scope)?;
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
                                    let _ = self.send_chunk(api::Chunk::end(client_id));
                                }
                            }
                            api::ChunkType::End => {
                                let value = self
                                    .clients
                                    .write()
                                    .map_err(|e| {
                                        io::Error::new(io::ErrorKind::BrokenPipe, e.to_string())
                                    })?
                                    .remove(&client_id);
                                if let Some(client) = value {
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
    service: &'a Service,
    client_id: api::ClientId,
    state: RdpStreamState,
}

impl RdpStreamCommon<'_> {
    #[cfg(feature = "backend")]
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

    #[cfg(feature = "frontend")]
    fn connect(&mut self) -> Result<(), io::Error> {
        match &self.state {
            RdpStreamState::Ready => {
                self.channel
                    .send_chunk(api::Chunk::start(self.client_id, self.service)?)
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
                let _ = self.channel.send_chunk(api::Chunk::end(self.client_id));
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
    fn new(channel: &'a Channel, service: &'a Service, client_id: api::ClientId) -> Self {
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

    fn is_connected(&self) -> bool {
        self.0.read().unwrap().state.is_connected()
    }

    #[cfg(feature = "backend")]
    fn accept(&self) -> Result<(), io::Error> {
        self.0.write().unwrap().accept()
    }

    #[cfg(feature = "frontend")]
    fn connect(&self) -> Result<(), io::Error> {
        self.0.write().unwrap().connect()
    }

    fn send_chunk(&self, chunk: api::Chunk) -> Result<(), io::Error> {
        self.0
            .write()
            .unwrap()
            .channel
            .send_chunk(chunk)
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e.to_string()))
    }

    fn disconnected(&self) {
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
        service: &'a Service,
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

    pub(crate) fn client_id(&self) -> api::ClientId {
        self.control.client_id()
    }

    #[cfg(feature = "backend")]
    fn accept(&self) -> Result<(), io::Error> {
        self.control.accept()
    }

    #[cfg(feature = "frontend")]
    fn connect(&self) -> Result<(), io::Error> {
        self.control.connect()
    }

    pub(crate) fn disconnect(&mut self) -> Result<(), io::Error> {
        self.writer.flush()?;
        self.control.disconnect();
        Ok(())
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

pub(crate) struct RdpReader<'a> {
    control: RdpStreamControl<'a>,
    from_rdp: crossbeam_channel::Receiver<api::Chunk>,
    last: Option<(api::Chunk, usize)>,
}

impl<'a> RdpReader<'a> {
    const fn new(
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

#[derive(Clone)]
pub(crate) struct RdpWriter<'a> {
    control: RdpStreamControl<'a>,
    buffer: [u8; api::Chunk::max_payload_length()],
    buffer_len: usize,
}

impl<'a> RdpWriter<'a> {
    const fn new(control: RdpStreamControl<'a>) -> Self {
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

            if let Err(e) = self.control.send_chunk(chunk) {
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

pub(crate) fn double_stream_copy(
    service_kind: Kind,
    service: &Service,
    rdp_stream: RdpStream<'_>,
    tcp_stream: TcpStream,
) -> Result<(), io::Error> {
    let client_id = rdp_stream.client_id();

    let (rdp_stream_read, rdp_stream_write) = rdp_stream.split();

    let tcp_stream2 = tcp_stream.try_clone()?;

    thread::scope(|scope| {
        thread::Builder::new()
            .name(format!(
                "{service_kind} {service} {client_id:x} stream copy"
            ))
            .spawn_scoped(scope, move || {
                let mut rdp_stream_read = io::BufReader::new(rdp_stream_read);
                let mut tcp_stream2 = io::BufWriter::new(tcp_stream2);
                if let Err(e) = stream_copy(&mut rdp_stream_read, &mut tcp_stream2) {
                    crate::debug!("error: {e}");
                } else {
                    crate::debug!("stopped");
                }
                let _ = tcp_stream2.flush();
                if let Ok(tcp_stream2) = tcp_stream2.into_inner() {
                    let _ = tcp_stream2.shutdown(net::Shutdown::Both);
                }
                let rdp_stream_read = rdp_stream_read.into_inner();
                rdp_stream_read.disconnect();
            })
            .unwrap();

        let mut tcp_stream = io::BufReader::new(tcp_stream);
        let mut rdp_stream_write = io::BufWriter::new(rdp_stream_write);
        if let Err(e) = stream_copy(&mut tcp_stream, &mut rdp_stream_write) {
            crate::debug!("error: {e}");
        } else {
            crate::debug!("stopped");
        }
        let _ = rdp_stream_write.flush();
        if let Ok(mut rdp_stream_write) = rdp_stream_write.into_inner() {
            let _ = rdp_stream_write.disconnect();
        }
        let tcp_stream = tcp_stream.into_inner();
        let _ = tcp_stream.shutdown(net::Shutdown::Both);

        Ok(())
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    #[cfg(feature = "backend")]
    Backend,
    #[cfg(feature = "frontend")]
    Frontend,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            #[cfg(feature = "backend")]
            Self::Backend => write!(f, "backend"),
            #[cfg(feature = "frontend")]
            Self::Frontend => write!(f, "frontend"),
        }
    }
}

#[cfg(feature = "frontend")]
pub struct TcpFrontendServer {
    service: &'static Service,
    server: net::TcpListener,
    pub(crate) ip: net::IpAddr,
}

#[cfg(feature = "frontend")]
impl TcpFrontendServer {
    pub fn service(&self) -> &Service {
        self.service
    }

    pub fn bind(service: &'static Service, tcp: net::SocketAddr) -> Result<Self, io::Error> {
        crate::info!("accepting {service} clients on {tcp}");

        let server = net::TcpListener::bind(tcp)?;
        let ip = server.local_addr()?.ip();

        Ok(Self {
            service,
            server,
            ip,
        })
    }

    pub fn start<'a>(&'a self, channel: &'a Channel) -> Result<(), io::Error> {
        thread::scope(|scope| loop {
            let (client, client_addr) = self.server.accept()?;

            crate::debug!("new client {client_addr}");

            thread::Builder::new()
                .name(format!("{} {} {client_addr}", Kind::Frontend, self.service))
                .spawn_scoped(scope, move || match self.service.tcp_frontend.as_ref() {
                    None => {
                        crate::error!("no TCP frontend for {}", self.service);
                    }
                    Some(frontend) => {
                        if let Err(e) = (frontend.handler)(&self, scope, client, channel) {
                            crate::debug!("error: {e}");
                        }
                    }
                })
                .unwrap();
        })
    }
}

#[cfg(feature = "frontend")]
type FrontendHandler<S, C> = for<'a> fn(
    server: &S,
    scope: &'a thread::Scope<'a, '_>,
    client: C,
    channel: &'a Channel,
) -> Result<(), api::Error>;

#[cfg(feature = "frontend")]
type TcpFrontendHandler = FrontendHandler<TcpFrontendServer, net::TcpStream>;

#[cfg(feature = "frontend")]
pub struct TcpFrontend {
    pub(crate) default_port: u16,
    pub(crate) handler: TcpFrontendHandler,
}

#[cfg(feature = "frontend")]
impl TcpFrontend {
    pub const fn default_port(&self) -> u16 {
        self.default_port
    }
}

#[cfg(feature = "backend")]
type BackendHandler = fn(stream: RdpStream<'_>) -> Result<(), io::Error>;

#[cfg(feature = "backend")]
pub(crate) struct Backend {
    pub(crate) handler: BackendHandler,
}

pub struct Service {
    pub(crate) name: &'static str,
    #[cfg(feature = "frontend")]
    pub(crate) tcp_frontend: Option<TcpFrontend>,
    #[cfg(feature = "backend")]
    pub(crate) backend: Backend,
}

impl Service {
    pub const fn name(&self) -> &'static str {
        self.name
    }

    #[cfg(feature = "frontend")]
    pub fn tcp_frontend(&self) -> Option<&TcpFrontend> {
        self.tcp_frontend.as_ref()
    }
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(feature = "backend")]
fn lookup_bytes(bytes: &[u8]) -> Result<&'static Service, String> {
    let name = String::from_utf8_lossy(bytes).to_string();
    lookup(&name).ok_or(name)
}

pub fn lookup(name: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|s| s.name == name).map(|s| *s)
}

pub static SERVICES: [&Service; 6] = [
    &clipboard::SERVICE,
    &command::SERVICE,
    &input::SERVICE,
    &ftp::SERVICE,
    &socks5::SERVICE,
    &stage0::SERVICE,
];
