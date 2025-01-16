#![allow(clippy::missing_safety_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::cast_possible_wrap)]
#![allow(non_snake_case)]

use std::ffi;

mod headers;

#[no_mangle]
pub unsafe extern "C" fn DriverOpen(
    pVd: headers::PVD,
    pVdOpen: headers::PVDOPEN,
    puiSize: headers::PUINT16,
) -> ffi::c_int {
    todo!("DriverOpen")
}

#[no_mangle]
pub unsafe extern "C" fn DriverClose(
    pVd: headers::PVD,
    pDllClose: headers::PDLLCLOSE,
    puiSize: headers::PUINT16,
) -> ffi::c_int {
    todo!("DriverClose")
}

#[no_mangle]
pub unsafe extern "C" fn DriverInfo(
    pVd: headers::PVD,
    pDllInfo: headers::PDLLINFO,
    puiSize: headers::PUINT16,
) -> ffi::c_int {
    todo!("DriverInfo")
}

#[no_mangle]
pub unsafe extern "C" fn DriverPoll(
    pVd: headers::PVD,
    pDllPoll: headers::PDLLPOLL,
    puiSize: headers::PUINT16,
) -> ffi::c_int {
    todo!("DriverPoll")
}

#[no_mangle]
pub unsafe extern "C" fn DriverQueryInformation(
    pVd: headers::PVD,
    pDllQueryInformation: headers::PVDQUERYINFORMATION,
    puiSize: headers::PUINT16,
) -> ffi::c_int {
    todo!("DriverQueryInformation")
}

#[no_mangle]
pub unsafe extern "C" fn DriverSetInformation(
    pVd: headers::PVD,
    pDllSetInformation: headers::PVDSETINFORMATION,
    puiSize: headers::PUINT16,
) -> ffi::c_int {
    todo!("DriverSetInformation")
}

#[no_mangle]
pub unsafe extern "C" fn DriverGetLastError(
    pVd: headers::PVD,
    pDllLastError: headers::PVDLASTERROR,
) -> ffi::c_int {
    todo!("DriverGetlastError")
}

#[no_mangle]
pub unsafe extern "C" fn ICADataArrival(
    pVd: headers::PVOID,
    uChan: headers::USHORT,
    pBuf: headers::LPBYTE,
    Length: headers::USHORT,
) -> ::std::os::raw::c_int {
    todo!("ICADataArrival")
}

#[cfg(not(target_os = "windows"))]
#[link(name = ":vdapi.a", kind = "static")]
extern "C" {
    fn VdCallWd(
        pVd: headers::PVOID,
        uChan: headers::USHORT,
        pBuf: headers::LPBYTE,
        Length: headers::USHORT,
    ) -> ffi::c_int;
}
#[cfg(target_os = "windows")]
#[link(name = ":vdapi.lib", kind = "static")]
extern "C" {
    fn VdCallWd(
        pVd: headers::PVOID,
        uChan: headers::USHORT,
        pBuf: headers::LPBYTE,
        Length: headers::USHORT,
    ) -> ffi::c_int;
}
