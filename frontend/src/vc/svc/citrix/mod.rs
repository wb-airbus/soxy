#![allow(clippy::missing_safety_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::cast_possible_wrap)]
#![allow(non_snake_case)]

#[cfg(feature = "service-input")]
use crate::client;
use crate::{control, vc};
use common::api;
use std::{ffi, mem, ops::Deref, ptr, slice, sync};

mod headers;
mod vd;

struct ShadowHandle {
    pwd_data: headers::PWD,
    channel_num: headers::USHORT,
    queue_virtual_write: unsafe extern "C" fn(
        headers::LPVOID,
        headers::USHORT,
        headers::LPMEMORY_SECTION,
        headers::USHORT,
        headers::USHORT,
    ) -> ffi::c_int,
    write_last_miss: sync::RwLock<Option<Vec<u8>>>,
    write_queue_receive: crossbeam_channel::Receiver<Vec<u8>>,
}

unsafe impl Send for ShadowHandle {}
unsafe impl Sync for ShadowHandle {}

static SHADOW_HANDLE: sync::RwLock<Option<ShadowHandle>> = sync::RwLock::new(None);

fn DriverOpen(vd: &mut headers::VD, vd_open: &mut headers::VDOPEN) -> Result<(), ffi::c_int> {
    let name = match crate::CONFIG.get() {
        None => {
            common::error!("no config loaded!");
            return Err(1);
        }
        Some(config) => {
            common::info!("SVC(Citrix) virtual channel name is {:?}", config.channel);

            match common::virtual_channel_name(&config.channel) {
                Err(e) => {
                    common::error!("{e}");
                    return Err(1);
                }
                Ok(name) => name,
            }
        }
    };

    let mut wdovc = headers::OPENVIRTUALCHANNEL {
        pVCName: name.as_ptr().cast_mut().cast(),
        ..Default::default()
    };

    let mut query_info = headers::WDQUERYINFORMATION {
        WdInformationClass: headers::_WDINFOCLASS_WdOpenVirtualChannel,
        pWdInformation: ptr::from_mut(&mut wdovc).cast(),
        WdInformationLength: u16::try_from(mem::size_of::<headers::OPENVIRTUALCHANNEL>())
            .expect("value too large"),
        ..Default::default()
    };

    vd::WdQueryInformation(vd, &mut query_info)?;

    let mask = u32::wrapping_shl(1, u32::from(wdovc.Channel));

    vd_open.ChannelMask = mask;

    #[allow(clippy::used_underscore_items)]
    let mut vdwh = headers::VDWRITEHOOK {
        Type: wdovc.Channel,
        pVdData: ptr::from_mut(vd).cast(),
        __bindgen_anon_1: headers::_VDWRITEHOOK__bindgen_ty_1 {
            pProc: Some(ICADataArrival),
        },
        ..Default::default()
    };

    let mut set_info = headers::WDSETINFORMATION {
        WdInformationClass: headers::_WDINFOCLASS_WdVirtualWriteHook,
        pWdInformation: ptr::from_mut(&mut vdwh).cast(),
        WdInformationLength: u16::try_from(mem::size_of::<headers::VDWRITEHOOK>())
            .expect("value too large"),
    };

    vd::WdSetInformation(vd, &mut set_info)?;

    common::debug!("maximum_write_size = {}", vdwh.MaximumWriteSize);

    if usize::from(vdwh.MaximumWriteSize) < (api::PDU_DATA_MAX_SIZE - 1) {
        return Err(headers::CLIENT_ERROR_BUFFER_TOO_SMALL);
    }

    let (handle, shadow) = unsafe { vdwh.__bindgen_anon_2.pQueueVirtualWriteProc.as_ref() }
        .map_or(
            Err(headers::CLIENT_ERROR_NULL_MEM_POINTER),
            |queue_virtual_write| {
                Ok(Handle::new(
                    vdwh.pWdData.cast(),
                    wdovc.Channel,
                    *queue_virtual_write,
                ))
            },
        )?;

    let handle = super::Handle::Citrix(handle);
    let handle = vc::GenericHandle::Static(handle);

    SHADOW_HANDLE.write().unwrap().replace(shadow);

    crate::CONTROL
        .deref()
        .channel_connector()
        .send(control::FromVc::Opened(handle))
        .expect("internal error: failed to send control message");

    Ok(())
}

#[allow(clippy::unnecessary_wraps)]
fn DriverClose(
    _vd: &mut headers::VD,
    _dll_close: &mut headers::DLLCLOSE,
) -> Result<(), ffi::c_int> {
    let _ = SHADOW_HANDLE.write().unwrap().take();

    crate::CONTROL
        .deref()
        .channel_connector()
        .send(control::FromVc::Closed)
        .expect("internal error: failed to send control message");

    Ok(())
}

fn DriverInfo(vd: &headers::VD, dll_info: &mut headers::DLLINFO) -> Result<(), ffi::c_int> {
    let byte_count = u16::try_from(mem::size_of::<headers::VD_C2H>()).expect("value too large");

    if dll_info.ByteCount < byte_count {
        common::debug!("buffer too small: {} < {}", dll_info.ByteCount, byte_count);
        dll_info.ByteCount = byte_count;
        return Err(headers::CLIENT_ERROR_BUFFER_TOO_SMALL);
    }

    let soxy_c2h = dll_info.pBuffer.cast::<headers::SOXY_C2H>();

    let soxy_c2h = unsafe { soxy_c2h.as_mut() }
        .ok_or(headers::CLIENT_ERROR)
        .inspect_err(|_| common::error!("pBuffer is null!"))?;

    let vd_c2h = &mut soxy_c2h.Header;

    vd_c2h.ChannelMask = vd.ChannelMask;

    let module_c2h = &mut vd_c2h.Header;
    module_c2h.ByteCount = byte_count;
    module_c2h.ModuleClass =
        u8::try_from(headers::_MODULECLASS_Module_VirtualDriver).expect("value too large");
    module_c2h.VersionL = 1;
    module_c2h.VersionH = 1;

    let name = match crate::CONFIG.get() {
        None => {
            common::error!("no config loaded!");
            return Err(1);
        }
        Some(config) => match common::virtual_channel_name(&config.channel) {
            Err(e) => {
                common::error!("{e}");
                return Err(1);
            }
            Ok(name) => name,
        },
    };

    module_c2h.HostModuleName[..name.len()].copy_from_slice(&name);

    let flow = &mut vd_c2h.Flow;
    flow.BandwidthQuota = 0;
    flow.Flow = u8::try_from(headers::_VIRTUALFLOWCLASS_VirtualFlow_None).expect("value too large");

    dll_info.ByteCount = byte_count;

    Ok(())
}

// To avoid saturating completely the Citrix queue (which is
// half-duplex) during an upload from the frontend to the backend we
// send at most MAX_CHUNK_BATCH_SEND chunks per poll request
const MAX_CHUNK_BATCH_SEND: usize = 8;

fn DriverPoll(_vd: &mut headers::VD, _dll_poll: &mut headers::DLLPOLL) -> Result<(), ffi::c_int> {
    let binding = SHADOW_HANDLE.read().unwrap();
    let handle = binding.as_ref().ok_or(headers::CLIENT_ERROR)?;

    let mut mem = headers::MEMORY_SECTION::default();

    let mut next = handle
        .write_last_miss
        .write()
        .unwrap()
        .take()
        .or_else(|| handle.write_queue_receive.try_recv().ok());

    let mut batch_send = 0;

    loop {
        match next {
            None => {
                return Ok(());
            }
            Some(mut data) => {
                common::trace!("write data ({} bytes)", data.len());

                let len = u32::try_from(data.len()).expect("write error: data too large ({e})");

                mem.length = len;
                mem.pSection = data.as_mut_ptr();

                let rc = unsafe {
                    (handle.queue_virtual_write)(
                        handle.pwd_data.cast(),
                        handle.channel_num,
                        ptr::from_mut(&mut mem).cast(),
                        1,
                        0,
                    )
                };

                match rc {
                    headers::CLIENT_STATUS_SUCCESS => {
                        batch_send += 1;

                        if batch_send < MAX_CHUNK_BATCH_SEND {
                            next = handle.write_queue_receive.try_recv().ok();
                        } else if handle.write_queue_receive.is_empty() {
                            return Ok(());
                        } else {
                            return Err(headers::CLIENT_STATUS_ERROR_RETRY);
                        }
                    }
                    headers::CLIENT_ERROR_NO_OUTBUF => {
                        common::debug!("no more space, request a retry");
                        handle.write_last_miss.write().unwrap().replace(data);
                        return Err(headers::CLIENT_STATUS_ERROR_RETRY);
                    }
                    _ => {
                        return Err(headers::CLIENT_ERROR);
                    }
                }
            }
        }
    }
}

fn DriverQueryInformation(
    _vd: &mut headers::VD,
    _vd_query_info: &mut headers::VDQUERYINFORMATION,
) -> Result<(), ffi::c_int> {
    todo!()
}

#[allow(clippy::unnecessary_wraps)]
const fn DriverSetInformation(
    _vd: &mut headers::VD,
    _vd_set_info: &mut headers::VDSETINFORMATION,
) -> Result<(), ffi::c_int> {
    Ok(())
}

/*
fn DriverGetLastError(
    _vd: &mut headers::VD,
    _vd_last_error: &mut headers::VDLASTERROR,
) -> Result<(), ffi::c_int> {
    todo!()
}
 */

extern "C" fn ICADataArrival(
    _pVd: headers::PVOID,
    _uChan: headers::USHORT,
    pBuf: headers::LPBYTE,
    Length: headers::USHORT,
) -> ffi::c_int {
    common::trace!("ICADataArrival");

    assert!(
        Length as usize <= (api::Chunk::serialized_overhead() + api::Chunk::max_payload_length())
    );

    let data = unsafe { slice::from_raw_parts(pBuf.cast::<u8>(), Length as usize) };
    let data = Vec::from(data);

    crate::CONTROL
        .deref()
        .channel_connector()
        .send(control::FromVc::Data(data))
        .expect("internal error: failed to send control message");

    headers::CLIENT_STATUS_SUCCESS
}

pub struct Svc {
    #[cfg(feature = "service-input")]
    client: Option<client::Client>,
}

unsafe impl Sync for Svc {}
unsafe impl Send for Svc {}

impl Svc {
    #[cfg(feature = "service-input")]
    const fn new(client: Option<client::Client>) -> Self {
        Self { client }
    }
}

impl vc::VirtualChannel for Svc {
    fn open(&mut self) -> Result<(), vc::Error> {
        Ok(())
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

pub struct Handle {
    write_queue_send: crossbeam_channel::Sender<Vec<u8>>,
}

impl Handle {
    fn new(
        pwd_data: headers::PWD,
        channel_num: headers::USHORT,
        queue_virtual_write: unsafe extern "C" fn(
            headers::LPVOID,
            headers::USHORT,
            headers::LPMEMORY_SECTION,
            headers::USHORT,
            headers::USHORT,
        ) -> ffi::c_int,
    ) -> (Self, ShadowHandle) {
        let (write_queue_send, write_queue_receive) =
            crossbeam_channel::bounded(super::MAX_CHUNKS_IN_FLIGHT);

        (
            Self { write_queue_send },
            ShadowHandle {
                pwd_data,
                channel_num,
                queue_virtual_write,
                write_last_miss: sync::RwLock::new(None),
                write_queue_receive,
            },
        )
    }
}

impl vc::Handle for Handle {
    fn write(&self, data: Vec<u8>) -> Result<(), vc::Error> {
        Ok(self.write_queue_send.send(data)?)
    }

    fn close(&mut self) -> Result<(), vc::Error> {
        let _ = SHADOW_HANDLE.write().unwrap().take();
        Ok(())
    }
}
