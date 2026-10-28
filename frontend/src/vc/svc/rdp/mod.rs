use super::semaphore;
#[cfg(feature = "service-input")]
use crate::client;
use crate::{config, control, vc};
use std::{collections, mem, ops::Deref, ptr, slice, sync};

mod headers;

struct WriteStatus {
    sent: sync::RwLock<collections::HashMap<u32, Vec<u8>>>,
    can_send: semaphore::Semaphore,
    counter: sync::atomic::AtomicU32,
}

static WRITE_ACK: sync::RwLock<Option<WriteStatus>> = sync::RwLock::new(None);

#[derive(Clone)]
enum Entrypoints {
    Basic(headers::CHANNEL_ENTRY_POINTS),
    #[cfg(target_os = "windows")]
    Extended(headers::CHANNEL_ENTRY_POINTS_EX_WINDOWS),
    #[cfg(not(target_os = "windows"))]
    Extended(headers::CHANNEL_ENTRY_POINTS_EX_FREERDP),
}

static TMP_RDP_SVC: sync::RwLock<Option<Svc>> = sync::RwLock::new(None);

fn generic_channel_init_event(
    init_handle: headers::LPVOID,
    event: headers::UINT,
    _data: headers::LPVOID,
) {
    match event {
        headers::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_INITIALIZED => {
            common::debug!("channel_init_event called (event = INITIALIZED)");
        }
        headers::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_CONNECTED => {
            common::debug!("channel_init_event called (event = CONNECTED)");
            let _ = WRITE_ACK.write().unwrap().replace(WriteStatus {
                sent: sync::RwLock::new(collections::HashMap::new()),
                can_send: semaphore::Semaphore::new(vc::svc::MAX_CHUNKS_IN_FLIGHT),
                counter: sync::atomic::AtomicU32::new(0),
            });

            let value = TMP_RDP_SVC.write().unwrap().take();
            if let Some(mut svc) = value {
                svc.init_handle = init_handle;
                let svc = super::Svc::Rdp(svc);
                let vc = vc::GenericChannel::Static(svc);
                crate::CONTROL
                    .deref()
                    .channel_connector()
                    .send(control::FromVc::Loaded(vc))
                    .expect("internal error: failed to send control message");
            }
        }
        headers::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_DISCONNECTED => {
            common::debug!("channel_init_event called (event = DISCONNECTED)");

            if let Some(write_ack) = WRITE_ACK.read().unwrap().as_ref() {
                write_ack.sent.write().unwrap().clear();
                write_ack.can_send.reset(vc::svc::MAX_CHUNKS_IN_FLIGHT);
            }
            crate::CONTROL
                .deref()
                .channel_connector()
                .send(control::FromVc::Closed)
                .expect("internal error: failed to send control message");
        }
        headers::RDP_SVC_CHANNEL_EVENT_CHANNEL_EVENT_TERMINATED => {
            common::debug!("channel_init_event called (event = TERMINATED)");

            if let Some(write_ack) = WRITE_ACK.read().unwrap().as_ref() {
                write_ack.sent.write().unwrap().clear();
                write_ack.can_send.reset(vc::svc::MAX_CHUNKS_IN_FLIGHT);
            }

            crate::CONTROL
                .deref()
                .channel_connector()
                .send(control::FromVc::Terminated)
                .expect("internal error: failed to send control message");

            let _ = WRITE_ACK.write().unwrap().take();
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

            let data =
                unsafe { slice::from_raw_parts(data.cast::<u8>(), data_length as usize) }.to_vec();

            crate::CONTROL
                .deref()
                .channel_connector()
                .send(control::FromVc::Data(data))
                .expect("internal error: failed to send control message");
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

            crate::CONTROL
                .deref()
                .channel_connector()
                .send(control::FromVc::WriteCancelled)
                .expect("internal error: failed to send control message");
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
fn generic_virtual_channel_entry(
    config: &config::Config,
    svc: Svc,
    init_handle: headers::PVOID,
) -> Result<(), ()> {
    let mut channel_def = headers::CHANNEL_DEF::default();

    let name = match common::virtual_channel_name(&config.channel) {
        Err(e) => {
            common::error!("{e}");
            return Err(());
        }
        Ok(name) => name,
    };

    channel_def.name.copy_from_slice(&name);

    let channel_def_ptr: headers::PCHANNEL_DEF = &raw mut channel_def;

    common::debug!(
        "calling init init_handle = {init_handle:?}, channel_def_ptr = {channel_def_ptr:?})"
    );

    #[cfg(not(target_os = "windows"))]
    let version_requested = headers::ULONG::from(headers::VIRTUAL_CHANNEL_VERSION_WIN2000);
    #[cfg(target_os = "windows")]
    let version_requested = headers::VIRTUAL_CHANNEL_VERSION_WIN2000;

    let rc = match svc.entrypoints {
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
            Some(init) => {
                #[cfg(target_os = "windows")]
                unsafe {
                    init(
                        ptr::null_mut(),
                        init_handle,
                        channel_def_ptr,
                        1,
                        version_requested,
                        Some(channel_init_event_ex),
                    )
                }

                #[cfg(not(target_os = "windows"))]
                unsafe {
                    init(
                        ptr::null_mut(),
                        ptr::null_mut(),
                        init_handle,
                        channel_def_ptr,
                        1,
                        version_requested,
                        Some(channel_init_event_ex),
                    )
                }
            }
        },
    };

    if rc == headers::CHANNEL_RC_OK {
        let _ = TMP_RDP_SVC.write().unwrap().replace(svc);
        Ok(())
    } else {
        common::error!("bad return from init: {rc}");
        Err(())
    }
}

#[unsafe(no_mangle)]
extern "C" fn VirtualChannelEntry(entry_points: headers::PCHANNEL_ENTRY_POINTS) -> headers::BOOL {
    match crate::bootstrap() {
        Err(e) => {
            eprintln!("{e}");
            headers::FALSE
        }
        Ok(config) => {
            common::debug!("CALLED VirtualChannelEntry");

            // Defensive hardening: refuse to run if the host passes an invalid pointer/structure.
            // This prevents mstsc.exe from crashing if the ABI/structure layout doesn't match.
            if entry_points.is_null() || (entry_points as usize) < 0x10000 {
                common::error!(
                    "VirtualChannelEntry: invalid entry_points pointer: {entry_points:p}"
                );
                return headers::FALSE;
            }
            let expected =
                headers::DWORD::try_from(mem::size_of::<headers::CHANNEL_ENTRY_POINTS>())
                    .expect("size_of CHANNEL_ENTRY_POINTS to DWORD");
            let cb_size = unsafe { (*entry_points).cbSize };
            if cb_size < expected {
                common::error!(
                    "VirtualChannelEntry: unexpected cbSize={cb_size} (expected >= {expected}); refusing to load"
                );
                return headers::FALSE;
            }

            if generic_virtual_channel_entry(config, Svc::from(entry_points), ptr::null_mut())
                .is_err()
            {
                headers::FALSE
            } else {
                crate::init();
                headers::TRUE
            }
        }
    }
}

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
extern "C" fn VirtualChannelEntryEx(
    entry_points: headers::PCHANNEL_ENTRY_POINTS_EX_WINDOWS,
    init_handle: headers::PVOID,
) -> headers::BOOL {
    match crate::bootstrap() {
        Err(e) => {
            eprintln!("{e}");
            headers::FALSE
        }
        Ok(config) => {
            common::debug!("CALLED VirtualChannelEntryEx (Windows)");

            // Defensive hardening: refuse to run if the host passes an invalid pointer/structure.
            // This prevents mstsc.exe from crashing if the ABI/structure layout doesn't match.
            if entry_points.is_null() || (entry_points as usize) < 0x10000 {
                common::error!(
                    "VirtualChannelEntryEx: invalid entry_points pointer: {entry_points:p}"
                );
                return headers::FALSE;
            }
            let expected = headers::DWORD::try_from(mem::size_of::<
                headers::CHANNEL_ENTRY_POINTS_EX_WINDOWS,
            >())
            .expect("size_of CHANNEL_ENTRY_POINTS_EX_WINDOWS to DWORD");
            let cb_size = unsafe { (*entry_points).cbSize };
            if cb_size < expected {
                common::error!(
                    "VirtualChannelEntryEx: unexpected cbSize={cb_size} (expected >= {expected}); refusing to load"
                );
                return headers::FALSE;
            }

            if generic_virtual_channel_entry(config, Svc::from(entry_points), init_handle).is_err()
            {
                headers::FALSE
            } else {
                crate::init();
                headers::TRUE
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
#[unsafe(no_mangle)]
extern "C" fn VirtualChannelEntryEx(
    entry_points: headers::PCHANNEL_ENTRY_POINTS_EX_FREERDP,
    init_handle: headers::PVOID,
) -> headers::BOOL {
    match crate::bootstrap() {
        Err(e) => {
            eprintln!("{e}");
            headers::FALSE
        }
        Ok(config) => {
            common::debug!("CALLED VirtualChannelEntryEx (Freerdp)");

            if generic_virtual_channel_entry(config, Svc::from(entry_points), init_handle).is_err()
            {
                headers::FALSE
            } else {
                crate::init();
                headers::TRUE
            }
        }
    }
}

pub struct Svc {
    entrypoints: Entrypoints,
    init_handle: headers::LPVOID,
    #[cfg(feature = "service-input")]
    client: Option<client::Client>,
}

impl From<headers::PCHANNEL_ENTRY_POINTS> for Svc {
    fn from(pep: headers::PCHANNEL_ENTRY_POINTS) -> Self {
        let ep = unsafe { *pep };
        // service-input client detection is only implemented for FreeRDP/Citrix-style clients.
        // When loaded inside Microsoft's mstsc.exe, attempting to interpret the RDP entrypoints
        // as those client-specific structures can lead to invalid memory reads and crash mstsc.
        #[cfg(feature = "service-input")]
        let client = {
            #[cfg(target_os = "windows")]
            {
                None
            }
            #[cfg(not(target_os = "windows"))]
            {
                client::Client::load_from_entrypoints(ep.cbSize, pep.cast())
            }
        };
        let entrypoints = Entrypoints::Basic(ep);
        Self {
            entrypoints,
            init_handle: ptr::null_mut(),
            #[cfg(feature = "service-input")]
            client,
        }
    }
}

#[cfg(target_os = "windows")]
impl From<headers::PCHANNEL_ENTRY_POINTS_EX_WINDOWS> for Svc {
    fn from(pep: headers::PCHANNEL_ENTRY_POINTS_EX_WINDOWS) -> Self {
        let ep = unsafe { *pep };
        // See comment in the Basic entrypoints impl: mstsc's entrypoints are *not* FreeRDP/Citrix.
        #[cfg(feature = "service-input")]
        let client = None;
        let entrypoints = Entrypoints::Extended(ep);
        Self {
            entrypoints,
            init_handle: ptr::null_mut(),
            #[cfg(feature = "service-input")]
            client,
        }
    }
}

#[cfg(not(target_os = "windows"))]
impl From<headers::PCHANNEL_ENTRY_POINTS_EX_FREERDP> for Svc {
    fn from(pep: headers::PCHANNEL_ENTRY_POINTS_EX_FREERDP) -> Self {
        let ep = unsafe { *pep };
        #[cfg(feature = "service-input")]
        let client = client::Client::load_from_entrypoints(ep.cbSize, pep.cast());
        let entrypoints = Entrypoints::Extended(ep);
        Self {
            entrypoints,
            init_handle: ptr::null_mut(),
            #[cfg(feature = "service-input")]
            client,
        }
    }
}

impl vc::VirtualChannel for Svc {
    fn open(&mut self) -> Result<(), vc::Error> {
        let mut name = match crate::CONFIG.get() {
            None => {
                return Err(vc::Error::InvalidChannelName("no config loaded!".into()));
            }
            Some(config) => {
                common::info!("SVC(RDP) name is {:?}", config.channel);

                match common::virtual_channel_name(&config.channel) {
                    Err(e) => {
                        return Err(vc::Error::InvalidChannelName(e));
                    }
                    Ok(name) => name,
                }
            }
        };

        let mut open_handle = 0;

        common::debug!("open virtual channel {name:?}");

        let rc = match self.entrypoints {
            Entrypoints::Basic(ep) => {
                let open = ep.pVirtualChannelOpen.as_ref().ok_or(vc::Error::NotReady)?;
                unsafe {
                    open(
                        self.init_handle,
                        &raw mut open_handle,
                        name.as_mut_ptr(),
                        Some(channel_open_event),
                    )
                }
            }
            Entrypoints::Extended(ep) => {
                let open = ep
                    .pVirtualChannelOpenEx
                    .as_ref()
                    .ok_or(vc::Error::NotReady)?;
                unsafe {
                    open(
                        self.init_handle,
                        &raw mut open_handle,
                        name.as_mut_ptr(),
                        Some(channel_open_event_ex),
                    )
                }
            }
        };

        if rc == headers::CHANNEL_RC_OK {
            let handle = Handle {
                entrypoints: self.entrypoints.clone(),
                init: self.init_handle,
                open: open_handle,
            };
            let handle = super::Handle::Rdp(handle);
            let handle = vc::GenericHandle::Static(handle);

            crate::CONTROL
                .deref()
                .channel_connector()
                .send(control::FromVc::Opened(handle))
                .expect("internal error: failed to send control message");

            Ok(())
        } else {
            Err(vc::Error::VirtualChannel(rc))
        }
    }

    #[cfg(feature = "service-input")]
    fn client(&self) -> Option<&client::Client> {
        self.client.as_ref()
    }

    #[cfg(feature = "service-input")]
    fn client_mut(&mut self) -> Option<&mut client::Client> {
        self.client.as_mut()
    }

    fn terminate(&mut self) -> Result<(), vc::Error> {
        Ok(())
    }
}

unsafe impl Sync for Svc {}
unsafe impl Send for Svc {}

pub struct Handle {
    entrypoints: Entrypoints,
    init: headers::LPVOID,
    open: u32,
}

impl vc::Handle for Handle {
    fn write(&self, mut data: Vec<u8>) -> Result<(), vc::Error> {
        match WRITE_ACK.read().unwrap().as_ref() {
            None => Err(vc::Error::NotReady),
            Some(write_ack) => {
                let counter = write_ack
                    .counter
                    .fetch_add(1, sync::atomic::Ordering::SeqCst);

                #[cfg(not(target_os = "windows"))]
                let len = headers::ULONG::try_from(data.len()).map_err(|e| {
                    common::error!("write error: data too large ({e})");
                    vc::Error::VirtualChannel(0)
                })?;
                #[cfg(target_os = "windows")]
                let len = u32::try_from(data.len()).map_err(|e| {
                    common::error!("write error: data too large ({e})");
                    vc::Error::VirtualChannel(0)
                })?;

                let rc = match self.entrypoints {
                    Entrypoints::Basic(ep) => {
                        let write = ep
                            .pVirtualChannelWrite
                            .as_ref()
                            .ok_or(vc::Error::NotReady)?;

                        write_ack.can_send.acquire();

                        common::trace!("write {len} bytes");

                        unsafe {
                            write(
                                self.open,
                                data.as_mut_ptr().cast(),
                                len,
                                counter as headers::LPVOID,
                            )
                        }
                    }
                    Entrypoints::Extended(ep) => {
                        let write = ep
                            .pVirtualChannelWriteEx
                            .as_ref()
                            .ok_or(vc::Error::NotReady)?;

                        write_ack.can_send.acquire();

                        common::trace!("write {len} bytes");

                        unsafe {
                            write(
                                self.init,
                                self.open,
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
                    Err(vc::Error::VirtualChannel(rc))
                }
            }
        }
    }

    fn close(&mut self) -> Result<(), vc::Error> {
        let rc = match self.entrypoints {
            Entrypoints::Basic(ep) => {
                let close = ep
                    .pVirtualChannelClose
                    .as_ref()
                    .ok_or(vc::Error::NotReady)?;
                unsafe { close(self.open) }
            }
            Entrypoints::Extended(ep) => {
                let close = ep
                    .pVirtualChannelCloseEx
                    .as_ref()
                    .ok_or(vc::Error::NotReady)?;
                unsafe { close(self.init, self.open) }
            }
        };

        if rc == headers::CHANNEL_RC_OK {
            Ok(())
        } else {
            Err(vc::Error::VirtualChannel(rc))
        }
    }
}

unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
