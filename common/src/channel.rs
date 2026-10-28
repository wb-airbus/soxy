#[cfg(all(feature = "frontend", feature = "service-input"))]
use crate::input;
use crate::{api, rdp, service};

#[cfg(feature = "backend")]
use std::collections::hash_map;
#[cfg(feature = "frontend")]
use std::io;
use std::{collections, sync, thread};

pub struct Channel {
    clients:
        sync::RwLock<collections::HashMap<api::ClientId, crossbeam_channel::Sender<api::Chunk>>>,
    to_rdp: crossbeam_channel::Sender<api::Message>,
}

impl Channel {
    pub fn new(to_rdp: crossbeam_channel::Sender<api::Message>) -> Self {
        Self {
            clients: sync::RwLock::new(collections::HashMap::new()),
            to_rdp,
        }
    }

    pub(crate) fn shutdown(&self) {
        let mut clients = self.clients.write().unwrap();

        clients.iter().for_each(|(client_id, client)| {
            let _ = client.send(api::Chunk::end(*client_id));
        });

        clients.clear();

        let _ = self.to_rdp.send(api::Message::Shutdown);
    }

    pub(crate) fn send_chunk(&self, chunk: api::Chunk) -> Result<(), api::Error> {
        if matches!(
            chunk.chunk_type()?,
            api::ChunkType::Start | api::ChunkType::End
        ) {
            crate::debug!("CHANNEL send {chunk} len = {}", chunk.payload().len());
        }
        crate::trace!("CHANNEL send {chunk} len = {}", chunk.payload().len());
        Ok(self.to_rdp.send(api::Message::Chunk(chunk))?)
    }

    #[cfg(all(feature = "frontend", feature = "service-input"))]
    pub(crate) fn reset_client(&self) -> Result<(), api::Error> {
        Ok(self.to_rdp.send(api::Message::ResetClient)?)
    }

    #[cfg(all(feature = "frontend", feature = "service-input"))]
    pub(crate) fn send_input_setting(
        &self,
        setting: input::InputSetting,
    ) -> Result<(), api::Error> {
        Ok(self.to_rdp.send(api::Message::InputSetting(setting))?)
    }

    #[cfg(all(feature = "frontend", feature = "service-input"))]
    pub(crate) fn send_input_action(&self, action: input::InputAction) -> Result<(), api::Error> {
        Ok(self.to_rdp.send(api::Message::InputAction(action))?)
    }

    #[cfg(feature = "frontend")]
    pub(crate) fn connect<'a>(
        &'a self,
        service: &'a service::Service,
    ) -> Result<rdp::RdpStream<'a>, io::Error> {
        let client_id = api::new_client_id();

        let (from_rdp_send, from_rdp_recv) = crossbeam_channel::unbounded();

        let stream = rdp::RdpStream::new(self, service, client_id, from_rdp_recv);

        self.clients
            .write()
            .unwrap()
            .insert(client_id, from_rdp_send);

        if let Err(e) = stream.connect() {
            self.clients.write().unwrap().remove(&client_id);
            return Err(e);
        }

        Ok(stream)
    }

    #[cfg(feature = "backend")]
    fn handle_backend_start<'a>(
        &'a self,
        client_id: api::ClientId,
        payload: &[u8],
        scope: &'a thread::Scope<'a, '_>,
    ) {
        let mut clients = self.clients.write().unwrap();

        match clients.entry(client_id) {
            hash_map::Entry::Occupied(_) => {
                crate::error!("discarding start for already existing client {client_id:x}");
            }
            hash_map::Entry::Vacant(ve) => match service::lookup_bytes(payload) {
                Err(service) => {
                    crate::error!("new client for unknown service {service}!");
                    if let Err(e) = self
                        .to_rdp
                        .send(api::Message::Chunk(api::Chunk::end(client_id)))
                    {
                        crate::debug!("failed to send end for {client_id}: {e}");
                    }
                }
                Ok(service) => {
                    crate::debug!("new {service} client {client_id:x}");

                    match &service.backend {
                        None => {
                            crate::warn!("no backend to handle client {client_id:x}");
                        }
                        Some(backend) => {
                            let (from_rdp_send, from_rdp_recv) = crossbeam_channel::unbounded();

                            let stream =
                                rdp::RdpStream::new(self, service, client_id, from_rdp_recv);

                            ve.insert(from_rdp_send);

                            let thread = thread::Builder::new();
                            #[cfg(feature = "log")]
                            let thread = thread.name(format!(
                                "{} {service} {client_id:x}",
                                service::Kind::Backend
                            ));
                            thread
                                .spawn_scoped(scope, move || {
                                    stream.accept();

                                    if let Err(e) = (backend.handler)(stream) {
                                        crate::debug!("error: {e}");
                                    }

                                    let _ = stream;
                                })
                                .unwrap();
                        }
                    }
                }
            },
        }
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn run(
        &self,
        service_kind: service::Kind,
        from_rdp: &crossbeam_channel::Receiver<api::Message>,
    ) -> Result<(), api::Error> {
        thread::scope(|scope| {
            loop {
                match from_rdp.recv()? {
                    api::Message::Chunk(chunk) => match chunk.chunk_type() {
                        Err(e) => {
                            crate::error!("discarding invalid chunk: {e}");
                        }
                        Ok(chunk_type) => {
                            let client_id = chunk.client_id();

                            match chunk_type {
                                api::ChunkType::Start => {
                                    crate::debug!("CHANNEL received {chunk}");

                                    match service_kind {
                                        #[cfg(feature = "frontend")]
                                        service::Kind::Frontend => {
                                            let _ = scope;
                                            unimplemented!("accept connections");
                                        }
                                        #[cfg(feature = "backend")]
                                        service::Kind::Backend => {
                                            let payload = chunk.payload();
                                            self.handle_backend_start(client_id, payload, scope);
                                        }
                                    }
                                }
                                api::ChunkType::Data => {
                                    crate::trace!("CHANNEL received {chunk}");

                                    match self.clients.read().unwrap().get(&client_id) {
                                        None => {
                                            crate::warn!(
                                                "received Data for unknown client {client_id:x}"
                                            );
                                            let _ = self.to_rdp.send(api::Message::Chunk(
                                                api::Chunk::end(client_id),
                                            ));
                                        }
                                        Some(client) => {
                                            if client.send(chunk).is_err() {
                                                crate::trace!(
                                                    "received Data for disconnected client {client_id:x}"
                                                );
                                            }
                                        }
                                    }
                                }
                                api::ChunkType::End => {
                                    crate::debug!("CHANNEL received {chunk}");

                                    let value = self.clients.write().unwrap().remove(&client_id);
                                    match value {
                                        None => {
                                            crate::warn!(
                                                "received End for unknown client {client_id:x}"
                                            );
                                        }
                                        Some(client) => {
                                            if client.send(chunk).is_err() {
                                                crate::debug!(
                                                    "received End for disconnected client {client_id:x}"
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    #[cfg(feature = "service-input")]
                    api::Message::InputSetting(_) => {
                        crate::error!("discarding input setting request");
                    }
                    #[cfg(feature = "service-input")]
                    api::Message::InputAction(_) => {
                        crate::error!("discarding input action request");
                    }
                    #[cfg(feature = "service-input")]
                    api::Message::ResetClient => {
                        crate::error!("discarding reset client request");
                    }
                    api::Message::Shutdown => {
                        self.shutdown();
                    }
                }
            }
        })
    }
}
