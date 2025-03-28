#![allow(clippy::missing_safety_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::cast_possible_wrap)]
#![allow(non_snake_case)]

use common::api;
use std::{ffi, fmt, mem, ptr, slice, sync};

mod headers;
mod vd;

pub enum Error {
    NullPointers,
    InvalidMaximumWriteSize(u16),
    NotReady,
    Disconnected,
    VirtualChannel(i32),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::NullPointers => write!(f, "null pointers"),
            Self::InvalidMaximumWriteSize(s) => write!(f, "invalid maximum write is: {s}"),
            Self::NotReady => write!(f, "not ready"),
            Self::Disconnected => write!(f, "disconnected"),
            Self::VirtualChannel(e) => write!(f, "virtual channel error: {e}"),
        }
    }
}

static HANDLE: sync::RwLock<Option<Handle>> = sync::RwLock::new(None);

struct Handle {
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
    write_queue_send: crossbeam_channel::Sender<Vec<u8>>,
    write_queue_receive: crossbeam_channel::Receiver<Vec<u8>>,
}

unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

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
    ) -> Self {
        let (write_queue_send, write_queue_receive) =
            crossbeam_channel::bounded(super::MAX_CHUNKS_IN_FLIGHT);
        Self {
            pwd_data,
            channel_num,
            queue_virtual_write,
            write_last_miss: sync::RwLock::new(None),
            write_queue_send,
            write_queue_receive,
        }
    }

    fn queue(&self, data: Vec<u8>) {
        self.write_queue_send.send(data).ok();
    }
}

pub fn DriverOpen(vd: &mut headers::VD, vd_open: &mut headers::VDOPEN) -> Result<(), ffi::c_int> {
    let mut handle = HANDLE.write().unwrap();

    if handle.is_some() {
        return Ok(());
    }

    let mut wdovc = headers::OPENVIRTUALCHANNEL {
        pVCName: ptr::from_ref(common::VIRTUAL_CHANNEL_NAME)
            .cast_mut()
            .cast(),
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

    if usize::from(vdwh.MaximumWriteSize) < (api::CHUNK_LENGTH - 1) {
        return Err(headers::CLIENT_ERROR_BUFFER_TOO_SMALL);
    }

    unsafe { vdwh.__bindgen_anon_2.pQueueVirtualWriteProc.as_ref() }.map_or(
        Err(headers::CLIENT_ERROR_NULL_MEM_POINTER),
        |queue_virtual_write| {
            let _ = handle.replace(Handle::new(
                vdwh.pWdData.cast(),
                wdovc.Channel,
                *queue_virtual_write,
            ));
            Ok(())
        },
    )
}

#[allow(clippy::unnecessary_wraps)]
pub fn DriverClose(
    _vd: &mut headers::VD,
    _dll_close: &mut headers::DLLCLOSE,
) -> Result<(), ffi::c_int> {
    let _ = HANDLE.write().unwrap().take();
    Ok(())
}

pub fn DriverInfo(vd: &headers::VD, dll_info: &mut headers::DLLINFO) -> Result<(), ffi::c_int> {
    let byte_count = u16::try_from(mem::size_of::<headers::VD_C2H>()).expect("value too large");

    if dll_info.ByteCount < byte_count {
        common::debug!("buffer too small: {} < {}", dll_info.ByteCount, byte_count);
        dll_info.ByteCount = byte_count;
        return Err(headers::CLIENT_ERROR_BUFFER_TOO_SMALL);
    }

    let soxy_c2h = dll_info.pBuffer.cast::<headers::SOXY_C2H>();

    match unsafe { soxy_c2h.as_mut() } {
        None => {
            common::error!("pBuffer is null!");
            Err(headers::CLIENT_ERROR)
        }
        Some(soxy_c2h) => {
            let vd_c2h = &mut soxy_c2h.Header;

            vd_c2h.ChannelMask = vd.ChannelMask;

            let module_c2h = &mut vd_c2h.Header;
            module_c2h.ByteCount = byte_count;
            module_c2h.ModuleClass =
                u8::try_from(headers::_MODULECLASS_Module_VirtualDriver).expect("value too large");
            module_c2h.VersionL = 1;
            module_c2h.VersionH = 1;

            for (i, b) in common::VIRTUAL_CHANNEL_NAME
                .to_bytes_with_nul()
                .iter()
                .enumerate()
            {
                module_c2h.HostModuleName[i] = *b;
            }

            let flow = &mut vd_c2h.Flow;
            flow.BandwidthQuota = 0;
            flow.Flow =
                u8::try_from(headers::_VIRTUALFLOWCLASS_VirtualFlow_None).expect("value too large");

            dll_info.ByteCount = byte_count;

            Ok(())
        }
    }
}

// To avoid saturating completely the Citrix queue (which is
// half-duplex) during an upload from the frontend to the backend we
// send at most MAX_CHUNK_BATCH_SEND chunks per poll request
const MAX_CHUNK_BATCH_SEND: usize = 8;

pub fn DriverPoll(
    _vd: &mut headers::VD,
    _dll_poll: &mut headers::DLLPOLL,
) -> Result<(), ffi::c_int> {
    let binding = HANDLE.read().unwrap();
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

pub fn DriverQueryInformation(
    _vd: &mut headers::VD,
    _vd_query_info: &mut headers::VDQUERYINFORMATION,
) -> Result<(), ffi::c_int> {
    todo!()
}

#[allow(clippy::unnecessary_wraps)]
pub fn DriverSetInformation(
    _vd: &mut headers::VD,
    _vd_set_info: &mut headers::VDSETINFORMATION,
) -> Result<(), ffi::c_int> {
    Ok(())
}

/*
pub fn DriverGetLastError(
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

    if let Some(from_rdp) = crate::SVC_TO_CONTROL.get() {
        assert!(
            Length as usize
                <= (api::Chunk::serialized_overhead() + api::Chunk::max_payload_length())
        );

        let data = unsafe { slice::from_raw_parts(pBuf.cast::<u8>(), Length as usize) };
        let data = Vec::from(data);

        from_rdp
            .send(super::Response::ReceivedData(data))
            .expect("internal error: failed to send RDP message");
    }

    headers::CLIENT_STATUS_SUCCESS
}

#[derive(Default)]
pub struct Svc {}

unsafe impl Sync for Svc {}
unsafe impl Send for Svc {}

impl super::SvcImplementation for Svc {
    #[allow(clippy::too_many_lines)]
    fn open(&mut self) -> Result<(), super::Error> {
        if HANDLE.read().unwrap().is_none() {
            Err(super::Error::Citrix(Error::Disconnected))
        } else {
            Ok(())
        }
    }

    fn write(&self, data: Vec<u8>) -> Result<(), super::Error> {
        HANDLE.read().unwrap().as_ref().map_or(
            Err(super::Error::Citrix(Error::Disconnected)),
            |handle| {
                handle.queue(data);
                Ok(())
            },
        )
    }

    fn close(&mut self) -> Result<(), super::Error> {
        let _ = HANDLE.write().unwrap().take();
        Ok(())
    }
}
