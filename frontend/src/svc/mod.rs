use common::api;
use std::{collections, ffi, fmt, ptr, slice, string, sync};
#[cfg(target_os = "windows")]
use windows_sys as ws;

#[cfg(feature = "citrix")]
mod citrix;
mod rdp_api;
mod semaphore;

const MAX_CHUNKS_IN_FLIGHT: usize = 64;

#[derive(Clone, Debug)]
pub enum State {
    Initialized,
    Connected(Option<String>),
    Disconnected,
    Terminated,
}

pub(crate) enum Command {
    Open,
    SendChunk(api::Chunk),
    Close,
}

pub(crate) enum Response {
    ChangeState(State),
    ReceivedChunk(api::Chunk),
    WriteCancelled,
}

#[derive(Clone)]
enum Entrypoints {
    Basic(rdp_api::CHANNEL_ENTRY_POINTS),
    Extended(rdp_api::CHANNEL_ENTRY_POINTS_EX),
}

static ENTRYPOINTS: sync::RwLock<Option<Entrypoints>> = sync::RwLock::new(None);

struct WriteStatus {
    sent: sync::RwLock<collections::HashMap<u32, Vec<u8>>>,
    can_send: semaphore::Semaphore,
    counter: sync::atomic::AtomicU32,
}

static WRITE_ACK: sync::RwLock<Option<WriteStatus>> = sync::RwLock::new(None);

pub enum Error {
    NotReady,
    VirtualChannel(u32),
    InvalidState(State),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::NotReady => write!(f, "not ready"),
            Self::VirtualChannel(e) => write!(f, "virtual channel error: {e}"),
            Self::InvalidState(state) => write!(f, "invalid state {state:?}"),
        }
    }
}

enum LowSvc {
    Basic {
        open: rdp_api::VirtualChannelOpen,
        write: rdp_api::VirtualChannelWrite,
        close: rdp_api::VirtualChannelClose,
    },
    Extended {
        open: rdp_api::VirtualChannelOpenEx,
        write: rdp_api::VirtualChannelWriteEx,
        close: rdp_api::VirtualChannelCloseEx,
    },
}

impl From<&Entrypoints> for LowSvc {
    fn from(entry_points: &Entrypoints) -> Self {
        match entry_points {
            Entrypoints::Basic(ep) => Self::Basic {
                open: ep.pVirtualChannelOpen,
                write: ep.pVirtualChannelWrite,
                close: ep.pVirtualChannelClose,
            },
            Entrypoints::Extended(ep) => Self::Extended {
                open: ep.pVirtualChannelOpenEx,
                write: ep.pVirtualChannelWriteEx,
                close: ep.pVirtualChannelCloseEx,
            },
        }
    }
}

impl LowSvc {
    fn open(&self, init_handle: rdp_api::LPVOID) -> Result<u32, Error> {
        let cname = ffi::CString::new(common::VIRTUAL_CHANNEL_NAME).unwrap();
        let cname_ptr = cname.as_ptr();
        let mut name: [ffi::c_char; 8] = [0; 8];
        for (i, name_i) in name.iter_mut().enumerate().take(usize::min(
            cname.count_bytes(),
            rdp_api::CHANNEL_NAME_LEN as usize,
        )) {
            *name_i = unsafe { *cname_ptr.wrapping_add(i) };
        }

        let mut open_handle = 0;

        let rc = unsafe {
            match self {
                Self::Basic { open, .. } => {
                    let open = open.as_ref().ok_or(Error::NotReady)?;
                    open(
                        init_handle,
                        &mut open_handle,
                        name.as_mut_ptr(),
                        Some(channel_open_event),
                    )
                }
                Self::Extended { open, .. } => {
                    let open = open.as_ref().ok_or(Error::NotReady)?;

                    open(
                        init_handle,
                        &mut open_handle,
                        name.as_mut_ptr(),
                        Some(channel_open_event_ex),
                    )
                }
            }
        };

        if rc == rdp_api::CHANNEL_RC_OK {
            Ok(open_handle)
        } else {
            Err(Error::VirtualChannel(rc))
        }
    }

    fn write(
        &self,
        init_handle: rdp_api::LPVOID,
        open_handle: u32,
        data: Vec<u8>,
    ) -> Result<(), Error> {
        if let Some(write_ack) = WRITE_ACK.read().unwrap().as_ref() {
            let counter = write_ack
                .counter
                .fetch_add(1, sync::atomic::Ordering::SeqCst);

            let rc = unsafe {
                match self {
                    Self::Basic { write, .. } => {
                        let write = write.as_ref().ok_or(Error::NotReady)?;

                        #[cfg(not(target_os = "windows"))]
                        let len = u64::try_from(data.len()).map_err(|e| {
                            common::error!("write error: data too large ({e})");
                            Error::VirtualChannel(0)
                        })?;
                        #[cfg(target_os = "windows")]
                        let len = u32::try_from(data.len()).map_err(|e| {
                            common::error!("write error: data too large ({e})");
                            Error::VirtualChannel(0)
                        })?;

                        write_ack.can_send.acquire();

                        write(
                            open_handle,
                            data.as_ptr() as *mut ffi::c_void,
                            len,
                            counter as *mut ffi::c_void,
                        )
                    }
                    Self::Extended { write, .. } => {
                        let write = write.as_ref().ok_or(Error::NotReady)?;

                        #[cfg(not(target_os = "windows"))]
                        let len = u64::try_from(data.len()).map_err(|e| {
                            common::error!("write error: data too large ({e})");
                            Error::VirtualChannel(0)
                        })?;
                        #[cfg(target_os = "windows")]
                        let len = u32::try_from(data.len()).map_err(|e| {
                            common::error!("write error: data too large ({e})");
                            Error::VirtualChannel(0)
                        })?;

                        write_ack.can_send.acquire();

                        write(
                            init_handle,
                            open_handle,
                            data.as_ptr() as *mut ffi::c_void,
                            len,
                            counter as *mut ffi::c_void,
                        )
                    }
                }
            };

            if rc == rdp_api::CHANNEL_RC_OK {
                write_ack.sent.write().unwrap().insert(counter, data);
                Ok(())
            } else {
                write_ack.can_send.release();
                Err(Error::VirtualChannel(rc))
            }
        } else {
            Err(Error::NotReady)
        }
    }

    fn close(&mut self, init_handle: rdp_api::LPVOID, open_handle: u32) -> Result<(), Error> {
        let rc = unsafe {
            match self {
                Self::Basic { close, .. } => {
                    let close = close.as_ref().ok_or(Error::NotReady)?;
                    close(open_handle)
                }
                Self::Extended { close, .. } => {
                    let close = close.as_ref().ok_or(Error::NotReady)?;
                    close(init_handle, open_handle)
                }
            }
        };

        if rc == rdp_api::CHANNEL_RC_OK {
            Ok(())
        } else {
            Err(Error::VirtualChannel(rc))
        }
    }
}

pub(crate) struct Svc {
    init_handle: rdp_api::LPVOID,
    open_handle: Option<u32>,
    lvc: LowSvc,
}

unsafe impl Sync for Svc {}
unsafe impl Send for Svc {}

impl Svc {
    fn new(init_handle: rdp_api::LPVOID, entry_points: &Entrypoints) -> Self {
        Self {
            init_handle,
            open_handle: None,
            lvc: LowSvc::from(entry_points),
        }
    }

    pub(crate) fn open(&mut self) -> Result<(), Error> {
        if self.open_handle.is_none() {
            self.open_handle = Some(self.lvc.open(self.init_handle)?);
        }
        Ok(())
    }

    pub(crate) fn write(&self, data: Vec<u8>) -> Result<(), Error> {
        if let Some(open_handle) = self.open_handle {
            return self.lvc.write(self.init_handle, open_handle, data);
        }
        Err(Error::InvalidState(State::Disconnected))
    }

    pub(crate) fn close(&mut self) -> Result<(), Error> {
        if let Some(open_handle) = self.open_handle {
            self.open_handle = None;
            return self.lvc.close(self.init_handle, open_handle);
        }
        Ok(())
    }
}

pub(crate) static SVC: sync::RwLock<Option<Svc>> = sync::RwLock::new(None);

unsafe fn generic_channel_init_event(
    init_handle: rdp_api::LPVOID,
    event: rdp_api::UINT,
    data: rdp_api::LPVOID,
) {
    match event {
        rdp_api::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_INITIALIZED => {
            common::trace!("channel_init_event called (event = INITIALIZED)");

            let mut gwrite_ack = WRITE_ACK.write().unwrap();
            let _ = gwrite_ack.replace(WriteStatus {
                sent: sync::RwLock::new(collections::HashMap::new()),
                can_send: semaphore::Semaphore::new(MAX_CHUNKS_IN_FLIGHT),
                counter: sync::atomic::AtomicU32::new(0),
            });

            if let Some(ep) = ENTRYPOINTS.read().unwrap().as_ref() {
                let svc = Svc::new(init_handle, ep);

                SVC.write().unwrap().replace(svc);

                if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
                    from_rdp
                        .send(Response::ChangeState(State::Initialized))
                        .expect("internal error: failed to send RDP message");
                }
            }
        }
        rdp_api::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_CONNECTED => {
            common::trace!("channel_init_event called (event = CONNECTED)");
            let server_name = data.cast::<ffi::c_char>();
            let server_name = ffi::CStr::from_ptr(server_name)
                .to_str()
                .ok()
                .map(string::ToString::to_string);
            if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
                from_rdp
                    .send(Response::ChangeState(State::Connected(server_name)))
                    .expect("internal error: failed to send RDP message");
            }
        }
        rdp_api::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_DISCONNECTED => {
            common::trace!("channel_init_event called (event = DISCONNECTED)");
            if let Some(write_ack) = WRITE_ACK.read().unwrap().as_ref() {
                write_ack.sent.write().unwrap().clear();
                write_ack.can_send.reset(MAX_CHUNKS_IN_FLIGHT);
            }
            if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
                from_rdp
                    .send(Response::ChangeState(State::Disconnected))
                    .expect("internal error: failed to send RDP message");
            }
        }
        rdp_api::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_TERMINATED => {
            common::trace!("channel_init_event called (event = TERMINATED)");
            if let Some(write_ack) = WRITE_ACK.read().unwrap().as_ref() {
                write_ack.sent.write().unwrap().clear();
                write_ack.can_send.reset(MAX_CHUNKS_IN_FLIGHT);
            }

            if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
                from_rdp
                    .send(Response::ChangeState(State::Terminated))
                    .expect("internal error: failed to send RDP message");
            }

            let _ = SVC.write().unwrap().take();

            let mut gwrite_ack = WRITE_ACK.write().unwrap();
            let _ = gwrite_ack.take();
        }
        _ => {
            common::error!("unknown channel_init_event {event}!");
        }
    }
}

unsafe extern "C" fn channel_init_event(
    init_handle: rdp_api::LPVOID,
    event: rdp_api::UINT,
    data: rdp_api::LPVOID,
    _data_length: rdp_api::UINT,
) {
    generic_channel_init_event(init_handle, event, data);
}

unsafe extern "C" fn channel_init_event_ex(
    _user_param: rdp_api::LPVOID,
    init_handle: rdp_api::LPVOID,
    event: rdp_api::UINT,
    data: rdp_api::LPVOID,
    _data_length: rdp_api::UINT,
) {
    generic_channel_init_event(init_handle, event, data);
}

unsafe fn generic_channel_open_event(
    event: rdp_api::UINT,
    data: rdp_api::LPVOID,
    data_length: rdp_api::UINT32,
    total_length: rdp_api::UINT32,
) {
    match event {
        rdp_api::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_DATA_RECEIVED => {
            common::trace!("channel_open_event called (event = DATA_RECEIVED, data_length = {data_length}, total_length = {total_length})");
            if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
                assert!(data_length == total_length);
                assert!(
                    data_length as usize
                        <= (api::Chunk::serialized_overhead() + api::Chunk::max_payload_length())
                );
                let data = slice::from_raw_parts(data.cast::<u8>(), data_length as usize);
                match api::Chunk::deserialize(data) {
                    Err(e) => {
                        common::error!("failed to deserialize chunk: {e}");
                    }
                    Ok(chunk) => {
                        from_rdp
                            .send(Response::ReceivedChunk(chunk))
                            .expect("internal error: failed to send RDP message");
                    }
                }
            }
        }

        rdp_api::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_WRITE_CANCELLED => {
            let marker = data as u32;
            common::trace!(
                "channel_open_event called (event = WRITE_CANCELLED, marker = {marker})"
            );
            if let Some(write_ack) = WRITE_ACK.read().unwrap().as_ref() {
                write_ack.sent.write().unwrap().remove(&marker);
                write_ack.can_send.release();
            }
            if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
                from_rdp
                    .send(Response::WriteCancelled)
                    .expect("internal error: failed to send RDP message");
            }
        }

        rdp_api::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_WRITE_COMPLETE => {
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

unsafe extern "C" fn channel_open_event(
    _open_handle: rdp_api::DWORD,
    event: rdp_api::UINT,
    data: rdp_api::LPVOID,
    data_length: rdp_api::UINT32,
    total_length: rdp_api::UINT32,
    _data_flags: rdp_api::UINT32,
) {
    generic_channel_open_event(event, data, data_length, total_length);
}

unsafe extern "C" fn channel_open_event_ex(
    _user_param: rdp_api::LPVOID,
    _open_handle: rdp_api::DWORD,
    event: rdp_api::UINT,
    data: rdp_api::LPVOID,
    data_length: rdp_api::UINT32,
    total_length: rdp_api::UINT32,
    _data_flags: rdp_api::UINT32,
) {
    generic_channel_open_event(event, data, data_length, total_length);
}

#[allow(clippy::too_many_lines)]
unsafe fn generic_virtual_channel_entry(
    entry_points: Entrypoints,
    init_handle: rdp_api::PVOID,
) -> rdp_api::BOOL {
    #[cfg(target_os = "windows")]
    {
        common::debug!("calling WSAStartup");

        let mut data = ws::Win32::Networking::WinSock::WSADATA {
            wVersion: 0,
            wHighVersion: 0,
            iMaxSockets: 0,
            iMaxUdpDg: 0,
            lpVendorInfo: ptr::null_mut(),
            szDescription: [0i8; 257],
            szSystemStatus: [0i8; 129],
        };

        let ret = unsafe { ws::Win32::Networking::WinSock::WSAStartup(0x0202, &mut data) };
        if ret != 0 {
            common::error!("WSAStartup failed 0x{ret:x}");
            return rdp_api::FALSE;
        }
    }

    crate::start();

    let cname = ffi::CString::new(common::VIRTUAL_CHANNEL_NAME).unwrap();
    let cname_ptr = cname.as_ptr();
    let mut name: [ffi::c_char; 8] = [0; 8];
    for (i, name_i) in name.iter_mut().enumerate().take(usize::min(
        cname.count_bytes(),
        rdp_api::CHANNEL_NAME_LEN as usize,
    )) {
        *name_i = unsafe { *cname_ptr.wrapping_add(i) };
    }

    #[allow(clippy::used_underscore_items)]
    let mut channel_def = rdp_api::_CHANNEL_DEF { name, options: 0 };
    let channel_def_ptr: *mut rdp_api::_CHANNEL_DEF = &mut channel_def;

    common::debug!(
        "calling init init_handle = {init_handle:?}, channel_def_ptr = {channel_def_ptr:?})"
    );

    let rc = unsafe {
        match entry_points {
            Entrypoints::Basic(ep) => {
                let mut init_handle: *mut ffi::c_void = ptr::null_mut();
                let init_handle_ptr: *mut *mut ffi::c_void = &mut init_handle;

                if let Some(init) = ep.pVirtualChannelInit {
                    #[cfg(not(target_os = "windows"))]
                    {
                        init(
                            init_handle_ptr,
                            channel_def_ptr,
                            1,
                            u64::from(rdp_api::VIRTUAL_CHANNEL_VERSION_WIN2000),
                            Some(channel_init_event),
                        )
                    }

                    #[cfg(target_os = "windows")]
                    {
                        init(
                            init_handle_ptr,
                            channel_def_ptr,
                            1,
                            rdp_api::VIRTUAL_CHANNEL_VERSION_WIN2000,
                            Some(channel_init_event),
                        )
                    }
                } else {
                    common::error!("invalid pVirtualChannelInit");

                    return rdp_api::FALSE;
                }
            }
            Entrypoints::Extended(ep) => {
                let user_param: *mut ffi::c_void = ptr::null_mut();
                let client_context: *mut ffi::c_void = ptr::null_mut();

                if let Some(init) = ep.pVirtualChannelInitEx {
                    #[cfg(not(target_os = "windows"))]
                    {
                        init(
                            user_param,
                            client_context,
                            init_handle,
                            channel_def_ptr,
                            1,
                            u64::from(rdp_api::VIRTUAL_CHANNEL_VERSION_WIN2000),
                            Some(channel_init_event_ex),
                        )
                    }
                    #[cfg(target_os = "windows")]
                    {
                        init(
                            user_param,
                            client_context,
                            init_handle,
                            channel_def_ptr,
                            1,
                            rdp_api::VIRTUAL_CHANNEL_VERSION_WIN2000,
                            Some(channel_init_event_ex),
                        )
                    }
                } else {
                    common::error!("invalid pVirtualChannelInitEx");
                    return rdp_api::FALSE;
                }
            }
        }
    };

    common::debug!("return from init: {rc}");

    if rc == rdp_api::CHANNEL_RC_OK {
        let mut gep = ENTRYPOINTS.write().unwrap();
        let _ = gep.replace(entry_points);
        rdp_api::TRUE
    } else {
        common::error!("bad return from init: {rc}");
        rdp_api::FALSE
    }
}

#[no_mangle]
pub unsafe extern "C" fn VirtualChannelEntry(
    entry_points: rdp_api::PCHANNEL_ENTRY_POINTS,
) -> rdp_api::BOOL {
    generic_virtual_channel_entry(Entrypoints::Basic(*entry_points), ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn VirtualChannelEntryEx(
    entry_points: rdp_api::PCHANNEL_ENTRY_POINTS_EX,
    init_handle: rdp_api::PVOID,
) -> rdp_api::BOOL {
    generic_virtual_channel_entry(Entrypoints::Extended(*entry_points), init_handle)
}
