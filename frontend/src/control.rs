use crate::vc::{self, Handle, VirtualChannel};
use common::api;
use std::{fmt, mem, sync, thread};

const FRONTEND_TO_VC_CHANNEL_SIZE: usize = 1;
const FRONTEND_OUTPUT_CHANNEL_SIZE: usize = 64;

#[derive(Debug)]
pub enum Error {
    Api(api::Error),
    Crossbeam(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::Api(e) => write!(f, "API error: {e}"),
            Self::Crossbeam(e) => write!(f, "internal error: {e}"),
        }
    }
}

impl From<api::Error> for Error {
    fn from(e: api::Error) -> Self {
        Self::Api(e)
    }
}

impl From<crossbeam_channel::RecvError> for Error {
    fn from(e: crossbeam_channel::RecvError) -> Self {
        Self::Crossbeam(e.to_string())
    }
}

impl<T> From<crossbeam_channel::SendError<T>> for Error {
    fn from(e: crossbeam_channel::SendError<T>) -> Self {
        Self::Crossbeam(e.to_string())
    }
}

pub enum FromVc {
    Loaded(vc::GenericChannel),
    Opened(vc::GenericHandle),
    Data(Vec<u8>),
    WriteCancelled,
    Closed,
    Terminated,
}

enum State {
    Invalid,
    Loaded(vc::GenericChannel),
    Opened(vc::GenericChannel, vc::GenericHandle),
    Closed(vc::GenericChannel),
    Terminated,
}

impl State {
    fn update<F>(&mut self, f: F)
    where
        F: FnOnce(Self) -> Self,
    {
        let old = mem::replace(self, Self::Invalid);
        *self = f(old);
    }
}

struct FrontendControl {
    input: crossbeam_channel::Receiver<api::Message>,
}

impl FrontendControl {
    fn run(&self, control: &Control) -> Result<(), Error> {
        loop {
            match self.input.recv()? {
                api::Message::Chunk(chunk) => {
                    let state = control.state.read().unwrap();
                    match &*state {
                        State::Opened(_, handle) => {
                            if let Err(e) = handle.write(chunk.serialized()) {
                                common::error!("failed to send chunk: {e}");
                            }
                        }
                        State::Loaded(_) => common::warn!("cannot send chunk in LOADED state"),
                        State::Closed(_) => common::warn!("cannot send chunk in CLOSED state"),
                        State::Terminated => common::warn!("cannot send chunk in TERMINATED state"),
                        State::Invalid => unreachable!("invalid state"),
                    }
                }

                #[cfg(feature = "service-input")]
                api::Message::InputSetting(setting) => {
                    let mut state = control.state.write().unwrap();
                    match &mut *state {
                        State::Loaded(vc) | State::Opened(vc, _) | State::Closed(vc) => {
                            match vc.client_mut() {
                                Some(client) => {
                                    if let Err(e) = client.set(setting) {
                                        common::error!("failed to send input setting: {e}");
                                    }
                                }
                                None => common::warn!("no client available"),
                            }
                        }
                        State::Terminated => {
                            common::warn!("cannot set input setting in TERMINATED state");
                        }
                        State::Invalid => unreachable!("invalid state"),
                    }
                }

                #[cfg(feature = "service-input")]
                api::Message::InputAction(action) => {
                    let state = control.state.read().unwrap();
                    match &*state {
                        State::Loaded(vc) | State::Opened(vc, _) | State::Closed(vc) => {
                            match vc.client() {
                                Some(client) => {
                                    if let Err(e) = client.play(action) {
                                        common::error!("failed to send input action: {e}");
                                    }
                                }
                                None => common::warn!("no client available"),
                            }
                        }
                        State::Terminated => common::warn!("cannot play input in TERMINATED state"),
                        State::Invalid => unreachable!("invalid state"),
                    }
                }

                #[cfg(feature = "service-input")]
                api::Message::ResetClient => {
                    let mut state = control.state.write().unwrap();
                    match &mut *state {
                        State::Loaded(vc) | State::Opened(vc, _) | State::Closed(vc) => {
                            vc.reset_client();
                        }
                        State::Terminated => {
                            common::warn!("cannot reset client in TERMINATED state");
                        }
                        State::Invalid => unreachable!("invalid state"),
                    }
                }

                api::Message::Shutdown => {
                    let mut state = control.state.write().unwrap();
                    match &mut *state {
                        State::Loaded(vc) | State::Closed(vc) => {
                            if let Err(e) = vc.terminate() {
                                common::warn!("failed to terminate old virtual channel: {e}");
                            }
                        }
                        State::Opened(vc, handle) => {
                            if let Err(e) = handle.close() {
                                common::warn!("failed to close opened virtual channel: {e}");
                            }
                            if let Err(e) = vc.terminate() {
                                common::warn!("failed to terminate old virtual channel: {e}");
                            }
                        }
                        State::Terminated => common::debug!("cannot shutdown in TERMINATED state"),
                        State::Invalid => unreachable!("invalid state"),
                    }
                }
            }
        }
    }
}

struct ChannelControl {
    input: crossbeam_channel::Receiver<FromVc>,
    output: crossbeam_channel::Sender<api::Message>,
}

impl ChannelControl {
    #[allow(clippy::too_many_lines)]
    fn run(&self, control: &Control) -> Result<(), Error> {
        let mut in_data =
            Vec::with_capacity(FRONTEND_OUTPUT_CHANNEL_SIZE * common::api::PDU_MAX_SIZE);

        loop {
            match self.input.recv()? {
                FromVc::Loaded(mut vc) => {
                    common::info!("changing to LOADED state");

                    control.state.write().unwrap().update(|state| {
                        match state {
                            State::Loaded(mut vc) | State::Closed(mut vc) => {
                                if let Err(e) = vc.terminate() {
                                    common::error!("failed to terminate old virtual channel: {e}");
                                }
                            }
                            State::Opened(mut vc, mut handle) => {
                                if let Err(e) = handle.close() {
                                    common::error!("failed to close opened virtual channel: {e}");
                                }
                                if let Err(e) = vc.terminate() {
                                    common::error!("failed to terminate old virtual channel: {e}");
                                }
                            }
                            State::Terminated => (),
                            State::Invalid => unreachable!("invalid state"),
                        }

                        if let Err(e) = vc.open() {
                            common::error!("failed to open virtual channel: {e}");
                        }

                        State::Loaded(vc)
                    });
                }

                FromVc::Opened(handle) => {
                    common::info!("changing to OPENED state");

                    control.state.write().unwrap().update(|state| match state {
                        State::Opened(vc, mut handle) => {
                            if let Err(e) = handle.close() {
                                common::error!("failed to close old opened virtual channel: {e}");
                            }
                            State::Opened(vc, handle)
                        }
                        State::Loaded(vc) | State::Closed(vc) => State::Opened(vc, handle),
                        State::Terminated => state,
                        State::Invalid => unreachable!("invalid state"),
                    });
                }

                FromVc::Closed => {
                    common::info!("changing to CLOSED state");

                    control.state.write().unwrap().update(|state| match state {
                        State::Loaded(vc) | State::Opened(vc, _) => {
                            if let Err(e) = self.output.send(api::Message::Shutdown) {
                                common::error!("failed to send shutdown: {e}");
                            }
                            State::Closed(vc)
                        }
                        State::Closed(_) | State::Terminated => state,
                        State::Invalid => unreachable!("invalid state"),
                    });
                }

                FromVc::Terminated => {
                    common::info!("changing to TERMINATED state");

                    control.state.write().unwrap().update(|state| match state {
                        State::Loaded(_) | State::Opened(_, _) => {
                            if let Err(e) = self.output.send(api::Message::Shutdown) {
                                common::error!("failed to send shutdown: {e}");
                            }
                            State::Terminated
                        }
                        State::Closed(_) | State::Terminated => State::Terminated,
                        State::Invalid => unreachable!("invalid state"),
                    });
                }

                FromVc::Data(mut data) => {
                    common::trace!(
                        "FROMVC::DATA in_data == {} bytes ++ {} bytes",
                        in_data.len(),
                        data.len()
                    );

                    if in_data.is_empty() {
                        'inner: loop {
                            match api::Chunk::can_deserialize_from(&data) {
                                None => {
                                    in_data.append(&mut data);
                                    break 'inner;
                                }
                                Some(len) => {
                                    if len == data.len() {
                                        // exactly one chunk
                                        let chunk = api::Chunk::deserialize(data)?;
                                        self.output.send(api::Message::Chunk(chunk))?;
                                        break 'inner;
                                    }

                                    let tail = data.split_off(len);
                                    let head = data;
                                    data = tail;

                                    let chunk = api::Chunk::deserialize(head)?;
                                    self.output.send(api::Message::Chunk(chunk))?;
                                }
                            }
                        }
                    } else {
                        in_data.append(&mut data);

                        'inner: loop {
                            match api::Chunk::can_deserialize_from(&in_data) {
                                None => break 'inner,
                                Some(len) => {
                                    let tail = in_data.split_off(len);
                                    let head = in_data;
                                    in_data = tail;

                                    let chunk = api::Chunk::deserialize(head)?;
                                    self.output.send(api::Message::Chunk(chunk))?;
                                }
                            }
                        }
                    }
                }

                FromVc::WriteCancelled => {
                    let mut state = control.state.write().unwrap();
                    match &mut *state {
                        State::Loaded(vc) | State::Closed(vc) => {
                            if let Err(e) = vc.terminate() {
                                common::error!("failed to terminate old virtual channel: {e}");
                            }
                        }
                        State::Opened(vc, handle) => {
                            if let Err(e) = handle.close() {
                                common::error!("failed to close opened virtual channel: {e}");
                            }
                            if let Err(e) = vc.terminate() {
                                common::error!("failed to terminate old virtual channel: {e}");
                            }
                        }
                        State::Terminated => (),
                        State::Invalid => unreachable!("invalid state"),
                    }

                    self.output.send(api::Message::Shutdown)?;
                }
            }
        }
    }
}

pub struct FrontendConnector {
    pub send: crossbeam_channel::Sender<api::Message>,
    pub recv: crossbeam_channel::Receiver<api::Message>,
}

pub struct ChannelConnector {
    send: crossbeam_channel::Sender<FromVc>,
}

impl ChannelConnector {
    pub fn send(&self, msg: FromVc) -> Result<(), Error> {
        Ok(self.send.send(msg)?)
    }
}

pub struct Control {
    state: sync::RwLock<State>,
    frontend: FrontendControl,
    channel: ChannelControl,
    frontend_connector: sync::RwLock<Option<FrontendConnector>>,
    channel_connector: ChannelConnector,
}

impl Control {
    pub fn new() -> Self {
        let (frontend_in_send, frontend_in_recv) =
            crossbeam_channel::bounded(FRONTEND_TO_VC_CHANNEL_SIZE);
        let (frontend_out_send, frontend_out_recv) =
            crossbeam_channel::bounded(FRONTEND_OUTPUT_CHANNEL_SIZE);
        let (vc_in_send, vc_in_recv) = crossbeam_channel::unbounded();

        let frontend = FrontendControl {
            input: frontend_in_recv,
        };

        let channel = ChannelControl {
            input: vc_in_recv,
            output: frontend_out_send,
        };

        let frontend_connector = FrontendConnector {
            send: frontend_in_send,
            recv: frontend_out_recv,
        };

        let channel_connector = ChannelConnector { send: vc_in_send };

        Self {
            state: sync::RwLock::new(State::Terminated),
            frontend,
            channel,
            frontend_connector: sync::RwLock::new(Some(frontend_connector)),
            channel_connector,
        }
    }

    #[cfg(feature = "dvc")]
    pub fn is_opened(&self) -> bool {
        let state = self.state.read().unwrap();
        matches!(&*state, &State::Opened(_, _))
    }

    pub const fn channel_connector(&self) -> &ChannelConnector {
        &self.channel_connector
    }

    pub fn take_frontend_connector(&self) -> Option<FrontendConnector> {
        self.frontend_connector.write().unwrap().take()
    }

    pub fn start<'a>(&'a self, scope: &'a thread::Scope<'a, '_>) {
        thread::Builder::new()
            .name("control frontend".into())
            .spawn_scoped(scope, || {
                if let Err(e) = self.frontend.run(self) {
                    common::error!("stopped: {e}");
                } else {
                    common::debug!("stopped");
                }
            })
            .unwrap();

        thread::Builder::new()
            .name("control channel".into())
            .spawn_scoped(scope, || {
                if let Err(e) = self.channel.run(self) {
                    common::error!("stopped: {e}");
                } else {
                    common::debug!("stopped");
                }
            })
            .unwrap();
    }
}
