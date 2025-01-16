use crate::svc;
use common::api;
use std::{sync, thread};

const SVC_CHANNEL_SIZE: usize = 256;
const FRONTEND_CHANNEL_SIZE: usize = 1;

#[derive(Clone)]
pub(crate) struct Control {
    state: sync::Arc<sync::RwLock<svc::State>>,
    frontend_input: crossbeam_channel::Receiver<api::ChunkControl>,
    frontend_output: crossbeam_channel::Sender<api::ChunkControl>,
    svc_input: crossbeam_channel::Receiver<svc::Response>,
    svc_output: crossbeam_channel::Sender<svc::Command>,
}

impl Control {
    pub(crate) fn new() -> (
        Self,
        crossbeam_channel::Sender<api::ChunkControl>,
        crossbeam_channel::Receiver<api::ChunkControl>,
        crossbeam_channel::Sender<svc::Response>,
        crossbeam_channel::Receiver<svc::Command>,
    ) {
        let (from_svc_sender, from_svc_receiver) = crossbeam_channel::bounded(SVC_CHANNEL_SIZE);
        let (to_svc_sender, to_svc_receiver) = crossbeam_channel::bounded(SVC_CHANNEL_SIZE);
        let (from_frontend_sender, from_frontend_receiver) =
            crossbeam_channel::bounded(FRONTEND_CHANNEL_SIZE);
        let (to_frontend_sender, to_frontend_receiver) =
            crossbeam_channel::bounded(FRONTEND_CHANNEL_SIZE);

        (
            Self {
                state: sync::Arc::new(sync::RwLock::new(svc::State::Disconnected)),
                svc_input: from_svc_receiver,
                svc_output: to_svc_sender,
                frontend_input: from_frontend_receiver,
                frontend_output: to_frontend_sender,
            },
            from_frontend_sender,
            to_frontend_receiver,
            from_svc_sender,
            to_svc_receiver,
        )
    }

    fn control_from_svc(&self) -> Result<(), crate::Error> {
        loop {
            match self.svc_input.recv()? {
                svc::Response::ChangeState(new_state) => {
                    let mut state = self.state.write().unwrap();
                    common::info!("change state from \"{state:?}\" to \"{new_state:?}\"");
                    *state = new_state.clone();
                    match new_state {
                        svc::State::Initialized => (),
                        svc::State::Connected(name) => {
                            common::info!("connected to {name:?}");
                            self.svc_output.send(svc::Command::Open)?;
                        }
                        svc::State::Disconnected | svc::State::Terminated => {
                            self.frontend_output.send(api::ChunkControl::Shutdown)?;
                            self.svc_output.send(svc::Command::Open)?;
                        }
                    }
                }
                svc::Response::ReceivedChunk(chunk) => {
                    common::trace!("svc -> frontend: {chunk}");
                    self.frontend_output.send(api::ChunkControl::Chunk(chunk))?;
                }
                svc::Response::WriteCancelled => {
                    common::error!("svc: write cancelled");
                    self.svc_output.send(svc::Command::Close)?;
                    self.frontend_output.send(api::ChunkControl::Shutdown)?;
                    self.svc_output.send(svc::Command::Open)?;
                }
            }
        }
    }

    fn control_to_svc(&self) -> Result<(), crate::Error> {
        loop {
            match self.frontend_input.recv()? {
                api::ChunkControl::Shutdown => {
                    self.svc_output.send(svc::Command::Close)?;
                }
                api::ChunkControl::Chunk(chunk) => {
                    self.svc_output.send(svc::Command::SendChunk(chunk))?;
                }
            }
        }
    }

    pub(crate) fn start(self) {
        let myself = self.clone();
        thread::spawn(move || {
            if let Err(e) = myself.control_to_svc() {
                common::error!("control to svc error: {e}");
            }
            common::debug!("control to svc terminated");
        });
        thread::spawn(move || {
            if let Err(e) = self.control_from_svc() {
                common::error!("control from svc error: {e}");
            }
            common::debug!("control from svc terminated");
        });
    }
}
