use super::headers;
#[cfg(feature = "service-input")]
use crate::client;
use crate::{control, vc};
use std::{ffi, mem, ops::Deref, ptr, slice, sync};

struct VirtualDriver {
    data: headers::VD,
    procedures: [headers::PDLLPROCEDURE; headers::VDxCOUNT as usize],
}

unsafe impl Send for VirtualDriver {}
unsafe impl Sync for VirtualDriver {}

impl Default for VirtualDriver {
    fn default() -> Self {
        let data = headers::VD::default();
        let procedures = unsafe {
            [
                mem::transmute::<
                    Option<unsafe extern "C" fn(headers::PDLLLINK) -> i32>,
                    headers::PDLLPROCEDURE,
                >(Some(Load)),
                mem::transmute::<
                    Option<
                        unsafe extern "C" fn(
                            headers::PVD,
                            headers::PDLLLINK,
                            headers::PUINT16,
                        ) -> i32,
                    >,
                    headers::PDLLPROCEDURE,
                >(Some(VdUnload)),
                mem::transmute::<
                    Option<
                        unsafe extern "C" fn(
                            headers::PVD,
                            headers::PVDOPEN,
                            headers::PUINT16,
                        ) -> i32,
                    >,
                    headers::PDLLPROCEDURE,
                >(Some(VdOpen)),
                mem::transmute::<
                    Option<
                        unsafe extern "C" fn(
                            headers::PVD,
                            headers::PDLLCLOSE,
                            headers::PUINT16,
                        ) -> i32,
                    >,
                    headers::PDLLPROCEDURE,
                >(Some(VdClose)),
                mem::transmute::<
                    Option<
                        unsafe extern "C" fn(
                            headers::PVD,
                            headers::PDLLINFO,
                            headers::PUINT16,
                        ) -> i32,
                    >,
                    headers::PDLLPROCEDURE,
                >(Some(VdInfo)),
                mem::transmute::<
                    Option<
                        unsafe extern "C" fn(
                            headers::PVD,
                            headers::PDLLPOLL,
                            headers::PUINT16,
                        ) -> i32,
                    >,
                    headers::PDLLPROCEDURE,
                >(Some(VdPoll)),
                mem::transmute::<
                    Option<
                        unsafe extern "C" fn(
                            headers::PVD,
                            headers::PVDQUERYINFORMATION,
                            headers::PUINT16,
                        ) -> i32,
                    >,
                    headers::PDLLPROCEDURE,
                >(Some(VdQueryInformation)),
                mem::transmute::<
                    Option<
                        unsafe extern "C" fn(
                            headers::PVD,
                            headers::PVDSETINFORMATION,
                            headers::PUINT16,
                        ) -> i32,
                    >,
                    headers::PDLLPROCEDURE,
                >(Some(VdSetInformation)),
            ]
        };

        Self { data, procedures }
    }
}

static VD: sync::OnceLock<VirtualDriver> = sync::OnceLock::new();

#[unsafe(no_mangle)]
extern "C" fn Load(pLink: headers::PDLLLINK) -> ffi::c_int {
    common::trace!("Load");

    crate::init();

    let vd = VD.get_or_init(VirtualDriver::default);

    #[cfg(feature = "service-input")]
    let client = client::Client::load_from_entrypoints(0, ptr::null_mut());

    #[cfg(feature = "service-input")]
    let svc = super::Svc::new(client);
    #[cfg(not(feature = "service-input"))]
    let svc = super::Svc {};

    let svc = vc::svc::Svc::Citrix(svc);
    let vc = vc::GenericChannel::Static(svc);

    match unsafe { pLink.as_mut() } {
        None => {
            common::error!("pLink is null!");
            headers::CLIENT_ERROR
        }
        Some(pLink) => {
            pLink.ProcCount = u16::try_from(vd.procedures.len()).expect("value too large");
            pLink.pProcedures = ptr::from_ref(&vd.procedures).cast_mut().cast();
            pLink.pData = ptr::from_ref(&vd.data).cast_mut().cast();

            crate::CONTROL
                .deref()
                .channel_connector()
                .send(control::FromVc::Loaded(vc))
                .expect("internal error: failed to send control message");

            headers::CLIENT_STATUS_SUCCESS
        }
    }
}

extern "C" fn VdUnload(
    _pVd: headers::PVD,
    pLink: headers::PDLLLINK,
    _puiSize: headers::PUINT16,
) -> ffi::c_int {
    common::trace!("VdUnload");

    if let Some(pLink) = unsafe { pLink.as_mut() } {
        pLink.ProcCount = 0;
        pLink.pProcedures = ptr::null_mut();
        pLink.pData = ptr::null_mut();
    }

    crate::CONTROL
        .deref()
        .channel_connector()
        .send(control::FromVc::Terminated)
        .expect("internal error: failed to send control message");

    headers::CLIENT_STATUS_SUCCESS
}

extern "C" fn VdOpen(
    pVd: headers::PVD,
    pVdOpen: headers::PVDOPEN,
    puiSize: headers::PUINT16,
) -> ffi::c_int {
    common::trace!("VdOpen");

    match unsafe { (pVd.as_mut(), pVdOpen.as_mut(), puiSize.as_mut()) } {
        (None, _, _) | (_, None, _) | (_, _, None) => headers::CLIENT_ERROR,
        (Some(pVd), Some(pVdOpen), Some(puiSize)) => {
            pVd.ChannelMask = 0;
            pVd.pWdLink = pVdOpen.pWdLink;
            pVd.LastError = 0;
            pVd.pPrivate = ptr::null_mut();

            if let Err(e) = super::DriverOpen(pVd, pVdOpen) {
                common::error!("DriverOpen failed: {e}");
                return e;
            }

            *puiSize = u16::try_from(mem::size_of::<headers::VDOPEN>()).expect("value too large");

            pVd.ChannelMask = pVdOpen.ChannelMask;

            headers::CLIENT_STATUS_SUCCESS
        }
    }
}

extern "C" fn VdClose(
    pVd: headers::PVD,
    pDllClose: headers::PDLLCLOSE,
    puiSize: headers::PUINT16,
) -> ffi::c_int {
    common::trace!("VdClose");

    match unsafe { (pVd.as_mut(), pDllClose.as_mut(), puiSize.as_mut()) } {
        (None, _, _) | (_, None, _) | (_, _, None) => headers::CLIENT_ERROR,
        (Some(pVd), Some(pDllClose), Some(puiSize)) => {
            if let Err(e) = super::DriverClose(pVd, pDllClose) {
                common::error!("DriverClose failed: {e}");
                return e;
            }

            *puiSize = u16::try_from(mem::size_of::<headers::DLLCLOSE>()).expect("value too large");

            headers::CLIENT_STATUS_SUCCESS
        }
    }
}

extern "C" fn VdInfo(
    pVd: headers::PVD,
    pDllInfo: headers::PDLLINFO,
    puiSize: headers::PUINT16,
) -> ffi::c_int {
    common::trace!("VdInfo");

    match unsafe { (pVd.as_mut(), pDllInfo.as_mut(), puiSize.as_mut()) } {
        (None, _, _) | (_, None, _) | (_, _, None) => headers::CLIENT_ERROR,
        (Some(pVd), Some(pDllInfo), Some(puiSize)) => {
            if let Err(e) = super::DriverInfo(pVd, pDllInfo) {
                if e != headers::CLIENT_ERROR_BUFFER_TOO_SMALL {
                    common::error!("DriverInfo failed: {e}");
                }
                return e;
            }

            *puiSize = u16::try_from(mem::size_of::<headers::DLLINFO>()).expect("value too large");

            headers::CLIENT_STATUS_SUCCESS
        }
    }
}

extern "C" fn VdPoll(
    pVd: headers::PVD,
    pDllPoll: headers::PDLLPOLL,
    puiSize: headers::PUINT16,
) -> ffi::c_int {
    common::trace!("VdPoll");

    match unsafe { (pVd.as_mut(), pDllPoll.as_mut(), puiSize.as_mut()) } {
        (None, _, _) | (_, None, _) | (_, _, None) => headers::CLIENT_ERROR,
        (Some(pVd), Some(pDllPoll), Some(puiSize)) => {
            if let Err(e) = super::DriverPoll(pVd, pDllPoll) {
                if e != headers::CLIENT_STATUS_ERROR_RETRY {
                    common::error!("DriverPoll failed: {e}");
                }
                return e;
            }

            *puiSize = u16::try_from(mem::size_of::<headers::DLLPOLL>()).expect("value too large");

            headers::CLIENT_STATUS_SUCCESS
        }
    }
}

extern "C" fn VdQueryInformation(
    pVd: headers::PVD,
    pVdQueryInformation: headers::PVDQUERYINFORMATION,
    puiSize: headers::PUINT16,
) -> ffi::c_int {
    common::trace!("VdQueryInformation");

    match unsafe { (pVd.as_mut(), pVdQueryInformation.as_mut(), puiSize.as_mut()) } {
        (None, _, _) | (_, None, _) | (_, _, None) => headers::CLIENT_ERROR,
        (Some(pVd), Some(pVdQueryInformation), Some(puiSize)) => {
            if let Err(e) = super::DriverQueryInformation(pVd, pVdQueryInformation) {
                common::error!("DriverQueryInformation failed: {e}");
                return e;
            }

            *puiSize = u16::try_from(mem::size_of::<headers::VDQUERYINFORMATION>())
                .expect("value too large");

            headers::CLIENT_STATUS_SUCCESS
        }
    }
}

extern "C" fn VdSetInformation(
    pVd: headers::PVD,
    pVdSetInformation: headers::PVDSETINFORMATION,
    puiSize: headers::PUINT16,
) -> ffi::c_int {
    common::trace!("VdSetInformation");

    match unsafe { (pVd.as_mut(), pVdSetInformation.as_mut(), puiSize.as_mut()) } {
        (None, _, _) | (_, None, _) | (_, _, None) => headers::CLIENT_ERROR,
        (Some(pVd), Some(pVdSetInformation), Some(_puiSize)) => {
            if let Err(e) = super::DriverSetInformation(pVd, pVdSetInformation) {
                common::error!("DriverSetInformation failed: {e}");
                return e;
            }

            unsafe {
                *puiSize = u16::try_from(mem::size_of::<headers::VDSETINFORMATION>())
                    .expect("value too large");
            }

            headers::CLIENT_STATUS_SUCCESS
        }
    }
}

pub fn WdQueryInformation(
    vd: &mut headers::VD,
    query_info: &mut headers::WDQUERYINFORMATION,
) -> Result<(), ffi::c_int> {
    common::trace!("WdQueryInformation");

    let pLink = unsafe { vd.pWdLink.as_ref() }.ok_or(headers::CLIENT_ERROR_NULL_MEM_POINTER)?;

    let pProcedures = pLink.pProcedures as *const headers::PDLLPROCEDURE;
    let procs: &[headers::PDLLPROCEDURE] =
        unsafe { slice::from_raw_parts(pProcedures, headers::WDxCOUNT as usize) };
    match procs[headers::WDxQUERYINFORMATION as usize].as_ref() {
        None => Err(headers::CLIENT_ERROR_NULL_MEM_POINTER),
        Some(proc) => {
            let mut ui_size = u16::try_from(mem::size_of::<headers::WDQUERYINFORMATION>())
                .expect("value too large");

            let ret = unsafe {
                proc(
                    pLink.pData,
                    ptr::from_mut(query_info).cast(),
                    &raw mut ui_size,
                )
            };

            if ret != headers::CLIENT_STATUS_SUCCESS {
                return Err(ret);
            }

            Ok(())
        }
    }
}

pub fn WdSetInformation(
    vd: &mut headers::VD,
    set_info: &mut headers::WDSETINFORMATION,
) -> Result<(), ffi::c_int> {
    common::trace!("WdSetInformation");

    let pLink = unsafe { vd.pWdLink.as_ref() }.ok_or(headers::CLIENT_ERROR_NULL_MEM_POINTER)?;

    let pProcedures = pLink.pProcedures as *const headers::PDLLPROCEDURE;
    let procs: &[headers::PDLLPROCEDURE] =
        unsafe { slice::from_raw_parts(pProcedures, headers::WDxCOUNT as usize) };
    match procs[headers::WDxSETINFORMATION as usize].as_ref() {
        None => Err(headers::CLIENT_ERROR_NULL_MEM_POINTER),
        Some(proc) => {
            let mut ui_size = u16::try_from(mem::size_of::<headers::WDSETINFORMATION>())
                .expect("value too large");

            let ret = unsafe {
                proc(
                    pLink.pData,
                    ptr::from_mut(set_info).cast(),
                    &raw mut ui_size,
                )
            };

            if ret != headers::CLIENT_STATUS_SUCCESS {
                return Err(ret);
            }

            Ok(())
        }
    }
}
