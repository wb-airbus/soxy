use super::semaphore;
use crate::client;
use std::{collections, ffi, fmt, ptr, slice, string, sync};

mod headers;

pub enum Error {
    NotReady,
    Disconnected,
    VirtualChannel(u32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::NotReady => write!(f, "not ready"),
            Self::Disconnected => write!(f, "disconnected"),
            Self::VirtualChannel(e) => write!(f, "virtual channel error: {e}"),
        }
    }
}

struct WriteStatus {
    sent: sync::RwLock<collections::HashMap<u32, Vec<u8>>>,
    can_send: semaphore::Semaphore,
    counter: sync::atomic::AtomicU32,
}

static WRITE_ACK: sync::RwLock<Option<WriteStatus>> = sync::RwLock::new(None);

enum Entrypoints {
    Basic(headers::CHANNEL_ENTRY_POINTS),
    Extended(headers::CHANNEL_ENTRY_POINTS_EX),
}

struct RdpSvc {
    entrypoints: Entrypoints,
    client: Option<client::Client>,
}

impl From<headers::PCHANNEL_ENTRY_POINTS> for RdpSvc {
    fn from(pep: headers::PCHANNEL_ENTRY_POINTS) -> Self {
        let ep = unsafe { *pep };
        let client = client::Client::load_from_entrypoints(ep.cbSize, pep.cast());
        let entrypoints = Entrypoints::Basic(ep);
        Self {
            entrypoints,
            client,
        }
    }
}

impl From<headers::PCHANNEL_ENTRY_POINTS_EX> for RdpSvc {
    fn from(pep: headers::PCHANNEL_ENTRY_POINTS_EX) -> Self {
        let ep = unsafe { *pep };
        let client = client::Client::load_from_entrypoints(ep.cbSize, pep.cast());
        let entrypoints = Entrypoints::Extended(ep);
        Self {
            entrypoints,
            client,
        }
    }
}

impl RdpSvc {
    fn open(&mut self, init_handle: headers::LPVOID) -> Result<u32, Error> {
        let mut open_handle = 0;

        let rc = match self.entrypoints {
            Entrypoints::Basic(ep) => {
                let open = ep.pVirtualChannelOpen.as_ref().ok_or(Error::NotReady)?;
                unsafe {
                    open(
                        init_handle,
                        &mut open_handle,
                        ptr::from_ref(common::VIRTUAL_CHANNEL_NAME)
                            .cast_mut()
                            .cast(),
                        Some(channel_open_event),
                    )
                }
            }
            Entrypoints::Extended(ep) => {
                let open = ep.pVirtualChannelOpenEx.as_ref().ok_or(Error::NotReady)?;
                unsafe {
                    open(
                        init_handle,
                        &mut open_handle,
                        ptr::from_ref(common::VIRTUAL_CHANNEL_NAME)
                            .cast_mut()
                            .cast(),
                        Some(channel_open_event_ex),
                    )
                }
            }
        };

        if rc == headers::CHANNEL_RC_OK {
            Ok(open_handle)
        } else {
            Err(Error::VirtualChannel(rc))
        }
    }

    fn write(
        &self,
        init_handle: headers::LPVOID,
        open_handle: u32,
        mut data: Vec<u8>,
    ) -> Result<(), Error> {
        match WRITE_ACK.read().unwrap().as_ref() {
            None => Err(Error::NotReady),
            Some(write_ack) => {
                let counter = write_ack
                    .counter
                    .fetch_add(1, sync::atomic::Ordering::SeqCst);

                #[cfg(not(target_os = "windows"))]
                let len = headers::ULONG::try_from(data.len()).map_err(|e| {
                    common::error!("write error: data too large ({e})");
                    Error::VirtualChannel(0)
                })?;
                #[cfg(target_os = "windows")]
                let len = u32::try_from(data.len()).map_err(|e| {
                    common::error!("write error: data too large ({e})");
                    Error::VirtualChannel(0)
                })?;

                let rc = match self.entrypoints {
                    Entrypoints::Basic(ep) => {
                        let write = ep.pVirtualChannelWrite.as_ref().ok_or(Error::NotReady)?;

                        write_ack.can_send.acquire();

                        common::trace!("write {len} bytes");

                        unsafe {
                            write(
                                open_handle,
                                data.as_mut_ptr().cast(),
                                len,
                                counter as headers::LPVOID,
                            )
                        }
                    }
                    Entrypoints::Extended(ep) => {
                        let write = ep.pVirtualChannelWriteEx.as_ref().ok_or(Error::NotReady)?;

                        write_ack.can_send.acquire();

                        common::trace!("write {len} bytes");

                        unsafe {
                            write(
                                init_handle,
                                open_handle,
                                data.as_mut_ptr().cast(),
                                len,
                                counter as headers::LPVOID,
                            )
                        }
                    }
                };

                if rc == headers::CHANNEL_RC_OK {
                    write_ack.sent.write().unwrap().insert(counter, data);
                    Ok(())
                } else {
                    write_ack.can_send.release();
                    Err(Error::VirtualChannel(rc))
                }
            }
        }
    }

    fn close(&mut self, init_handle: headers::LPVOID, open_handle: u32) -> Result<(), Error> {
        let rc = match self.entrypoints {
            Entrypoints::Basic(ep) => {
                let close = ep.pVirtualChannelClose.as_ref().ok_or(Error::NotReady)?;
                unsafe { close(open_handle) }
            }
            Entrypoints::Extended(ep) => {
                let close = ep.pVirtualChannelCloseEx.as_ref().ok_or(Error::NotReady)?;
                unsafe { close(init_handle, open_handle) }
            }
        };

        if rc == headers::CHANNEL_RC_OK {
            Ok(())
        } else {
            Err(Error::VirtualChannel(rc))
        }
    }

    fn reset_client(&mut self) {
        if let Some(client) = self.client_mut() {
            client.reset();
        }
    }

    fn client(&self) -> Option<&client::Client> {
        self.client.as_ref()
    }

    fn client_mut(&mut self) -> Option<&mut client::Client> {
        self.client.as_mut()
    }
}

unsafe impl Sync for RdpSvc {}
unsafe impl Send for RdpSvc {}

static TMP_RDP_SVC: sync::RwLock<Option<RdpSvc>> = sync::RwLock::new(None);

fn generic_channel_init_event(
    init_handle: headers::LPVOID,
    event: headers::UINT,
    data: headers::LPVOID,
) {
    match event {
        headers::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_INITIALIZED => {
            common::trace!("channel_init_event called (event = INITIALIZED)");

            let mut gwrite_ack = WRITE_ACK.write().unwrap();
            let _ = gwrite_ack.replace(WriteStatus {
                sent: sync::RwLock::new(collections::HashMap::new()),
                can_send: semaphore::Semaphore::new(crate::svc::MAX_CHUNKS_IN_FLIGHT),
                counter: sync::atomic::AtomicU32::new(0),
            });

            if let Some(rsvc) = TMP_RDP_SVC.write().unwrap().take() {
                let svc = Svc::new(init_handle, rsvc);
                let svc = super::Svc::Rdp(svc);

                super::SVC.write().unwrap().replace(svc);

                if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
                    from_rdp
                        .send(super::Response::ChangeState(super::State::Initialized))
                        .expect("internal error: failed to send RDP message");
                }
            }
        }
        headers::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_CONNECTED => {
            common::trace!("channel_init_event called (event = CONNECTED)");
            let server_name = data.cast::<ffi::c_char>();
            let server_name = unsafe {
                ffi::CStr::from_ptr(server_name)
                    .to_str()
                    .ok()
                    .map(string::ToString::to_string)
            };
            if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
                from_rdp
                    .send(super::Response::ChangeState(super::State::Connected(
                        server_name,
                    )))
                    .expect("internal error: failed to send RDP message");
            }
        }
        headers::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_DISCONNECTED => {
            common::trace!("channel_init_event called (event = DISCONNECTED)");
            if let Some(write_ack) = WRITE_ACK.read().unwrap().as_ref() {
                write_ack.sent.write().unwrap().clear();
                write_ack.can_send.reset(crate::svc::MAX_CHUNKS_IN_FLIGHT);
            }
            if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
                from_rdp
                    .send(super::Response::ChangeState(super::State::Disconnected))
                    .expect("internal error: failed to send RDP message");
            }
        }
        headers::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_TERMINATED => {
            common::trace!("channel_init_event called (event = TERMINATED)");
            if let Some(write_ack) = WRITE_ACK.read().unwrap().as_ref() {
                write_ack.sent.write().unwrap().clear();
                write_ack.can_send.reset(crate::svc::MAX_CHUNKS_IN_FLIGHT);
            }

            if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
                from_rdp
                    .send(super::Response::ChangeState(super::State::Terminated))
                    .expect("internal error: failed to send RDP message");
            }

            let _ = super::SVC.write().unwrap().take();

            let mut gwrite_ack = WRITE_ACK.write().unwrap();
            let _ = gwrite_ack.take();
        }
        _ => {
            common::error!("unknown channel_init_event {event}!");
        }
    }
}

extern "C" fn channel_init_event(
    init_handle: headers::LPVOID,
    event: headers::UINT,
    data: headers::LPVOID,
    _data_length: headers::UINT,
) {
    generic_channel_init_event(init_handle, event, data);
}

extern "C" fn channel_init_event_ex(
    _user_param: headers::LPVOID,
    init_handle: headers::LPVOID,
    event: headers::UINT,
    data: headers::LPVOID,
    _data_length: headers::UINT,
) {
    generic_channel_init_event(init_handle, event, data);
}

fn generic_channel_open_event(
    event: headers::UINT,
    data: headers::LPVOID,
    data_length: headers::UINT32,
    total_length: headers::UINT32,
) {
    match event {
        headers::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_DATA_RECEIVED => {
            common::trace!(
                "channel_open_event called (event = DATA_RECEIVED, data_length = {data_length}, total_length = {total_length})"
            );
            if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
                common::trace!("read {data_length} bytes");

                let data =
                    unsafe { slice::from_raw_parts(data.cast::<u8>(), data_length as usize) };
                let data = Vec::from(data);
                from_rdp
                    .send(super::Response::ReceivedData(data))
                    .expect("internal error: failed to send RDP message");
            }
        }

        headers::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_WRITE_CANCELLED => {
            let marker = data as u32;
            common::debug!(
                "channel_open_event called (event = WRITE_CANCELLED, marker = {marker})"
            );
            if let Some(write_ack) = WRITE_ACK.read().unwrap().as_ref() {
                write_ack.sent.write().unwrap().remove(&marker);
                write_ack.can_send.release();
            }
            if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
                from_rdp
                    .send(super::Response::WriteCancelled)
                    .expect("internal error: failed to send RDP message");
            }
        }

        headers::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_WRITE_COMPLETE => {
            let marker = data as u32;
            common::trace!("channel_open_event called (event = WRITE_COMPLETE, marker = {marker})");
            if let Some(write_ack) = WRITE_ACK.read().unwrap().as_ref() {
                write_ack.sent.write().unwrap().remove(&marker);
                write_ack.can_send.release();
            }
        }

        _ => {
            common::error!("unknown channel_open_event {event}!");
        }
    }
}

extern "C" fn channel_open_event(
    _open_handle: headers::DWORD,
    event: headers::UINT,
    data: headers::LPVOID,
    data_length: headers::UINT32,
    total_length: headers::UINT32,
    _data_flags: headers::UINT32,
) {
    generic_channel_open_event(event, data, data_length, total_length);
}

extern "C" fn channel_open_event_ex(
    _user_param: headers::LPVOID,
    _open_handle: headers::DWORD,
    event: headers::UINT,
    data: headers::LPVOID,
    data_length: headers::UINT32,
    total_length: headers::UINT32,
    _data_flags: headers::UINT32,
) {
    generic_channel_open_event(event, data, data_length, total_length);
}

#[allow(clippy::too_many_lines)]
fn generic_virtual_channel_entry(rsvc: RdpSvc, init_handle: headers::PVOID) -> Result<(), ()> {
    let mut channel_def = headers::CHANNEL_DEF::default();
    for (i, b) in common::VIRTUAL_CHANNEL_NAME
        .to_bytes_with_nul()
        .iter()
        .enumerate()
    {
        channel_def.name[i] = i8::try_from(*b).map_err(|_| {
            common::error!("invalid channel name");
        })?;
    }

    let channel_def_ptr: headers::PCHANNEL_DEF = &mut channel_def;

    common::debug!(
        "calling init init_handle = {init_handle:?}, channel_def_ptr = {channel_def_ptr:?})"
    );

    #[cfg(not(target_os = "windows"))]
    let version_requested = headers::ULONG::from(headers::VIRTUAL_CHANNEL_VERSION_WIN2000);
    #[cfg(target_os = "windows")]
    let version_requested = headers::VIRTUAL_CHANNEL_VERSION_WIN2000;

    let rc = match rsvc.entrypoints {
        Entrypoints::Basic(ep) => {
            let mut init_handle = ptr::null_mut();

            match ep.pVirtualChannelInit {
                None => {
                    common::error!("invalid pVirtualChannelInit");
                    return Err(());
                }
                Some(init) => unsafe {
                    init(
                        ptr::from_mut(&mut init_handle),
                        channel_def_ptr,
                        1,
                        version_requested,
                        Some(channel_init_event),
                    )
                },
            }
        }
        Entrypoints::Extended(ep) => match ep.pVirtualChannelInitEx {
            None => {
                common::error!("invalid pVirtualChannelInitEx");
                return Err(());
            }
            Some(init) => unsafe {
                init(
                    ptr::null_mut(),
                    ptr::null_mut(),
                    init_handle,
                    channel_def_ptr,
                    1,
                    version_requested,
                    Some(channel_init_event_ex),
                )
            },
        },
    };

    if rc == headers::CHANNEL_RC_OK {
        let _ = TMP_RDP_SVC.write().unwrap().replace(rsvc);
        Ok(())
    } else {
        common::error!("bad return from init: {rc}");
        Err(())
    }
}

#[unsafe(no_mangle)]
extern "C" fn VirtualChannelEntry(entry_points: headers::PCHANNEL_ENTRY_POINTS) -> headers::BOOL {
    crate::start();

    match generic_virtual_channel_entry(RdpSvc::from(entry_points), ptr::null_mut()) {
        Ok(()) => headers::TRUE,
        Err(()) => headers::FALSE,
    }
}

#[unsafe(no_mangle)]
extern "C" fn VirtualChannelEntryEx(
    entry_points: headers::PCHANNEL_ENTRY_POINTS_EX,
    init_handle: headers::PVOID,
) -> headers::BOOL {
    crate::start();

    match generic_virtual_channel_entry(RdpSvc::from(entry_points), init_handle) {
        Ok(()) => headers::TRUE,
        Err(()) => headers::FALSE,
    }
}

pub struct Svc {
    init_handle: headers::LPVOID,
    open_handle: Option<u32>,
    rsvc: RdpSvc,
}

impl Svc {
    fn new(init_handle: headers::LPVOID, rsvc: RdpSvc) -> Self {
        Self {
            init_handle,
            open_handle: None,
            rsvc,
        }
    }
}

impl super::SvcImplementation for Svc {
    fn open(&mut self) -> Result<(), super::Error> {
        if self.open_handle.is_some() {
            return Ok(());
        }
        let open_handle = self
            .rsvc
            .open(self.init_handle)
            .map_err(super::Error::Rdp)?;
        self.open_handle.replace(open_handle);
        Ok(())
    }

    fn write(&self, data: Vec<u8>) -> Result<(), super::Error> {
        self.open_handle
            .map_or(Err(super::Error::Rdp(Error::Disconnected)), |open_handle| {
                self.rsvc
                    .write(self.init_handle, open_handle, data)
                    .map_err(super::Error::Rdp)
            })
    }

    fn close(&mut self) -> Result<(), super::Error> {
        match self.open_handle.take() {
            None => Ok(()),
            Some(open_handle) => self
                .rsvc
                .close(self.init_handle, open_handle)
                .map_err(super::Error::Rdp),
        }
    }

    fn reset_client(&mut self) {
        self.rsvc.reset_client();
    }

    fn client(&self) -> Option<&client::Client> {
        self.rsvc.client()
    }

    fn client_mut(&mut self) -> Option<&mut client::Client> {
        self.rsvc.client_mut()
    }
}

unsafe impl Sync for Svc {}
unsafe impl Send for Svc {}
