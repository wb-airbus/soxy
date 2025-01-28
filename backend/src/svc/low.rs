use std::{cell, io, os, ptr};
use windows_sys as ws;

pub(crate) struct Svc<'a> {
    open: libloading::Symbol<'a, super::VirtualChannelOpen>,
    query: libloading::Symbol<'a, super::VirtualChannelQuery>,
}

impl<'a> Svc<'a> {
    pub(crate) fn load(
        lib: &'a libloading::Library,
        symbols: &super::SymbolNames,
    ) -> Result<Self, super::Error> {
        unsafe {
            Ok(Self {
                open: lib.get(symbols.open.as_bytes())?,
                query: lib.get(symbols.query.as_bytes())?,
            })
        }
    }

    pub(crate) fn open(&self, mut name: [i8; 8]) -> Result<Handle, super::Error> {
        let wtshandle = unsafe {
            (self.open)(
                ws::Win32::System::RemoteDesktop::WTS_CURRENT_SERVER_HANDLE,
                ws::Win32::System::RemoteDesktop::WTS_CURRENT_SESSION,
                name.as_mut_ptr(),
            )
        };

        if wtshandle.is_null() {
            let err = unsafe { ws::Win32::Foundation::GetLastError() };
            return Err(super::Error::VirtualChannelOpenStaticChannelFailed(err));
        }

        let mut filehandleptr: *mut ws::Win32::Foundation::HANDLE = ptr::null_mut();
        let filehandleptrptr: *mut *mut ws::Win32::Foundation::HANDLE = &mut filehandleptr;
        let mut len = 0;

        common::trace!("VirtualChannelQuery");
        let ret = unsafe {
            (self.query)(
                wtshandle,
                ws::Win32::System::RemoteDesktop::WTSVirtualFileHandle,
                filehandleptrptr.cast(),
                &mut len,
            )
        };
        if ret == ws::Win32::Foundation::FALSE {
            let err = unsafe { ws::Win32::Foundation::GetLastError() };
            return Err(super::Error::VirtualChannelQueryFailed(err));
        }
        if filehandleptr.is_null() {
            let err = unsafe { ws::Win32::Foundation::GetLastError() };
            return Err(super::Error::VirtualChannelQueryFailed(err));
        }

        let filehandle = unsafe { filehandleptr.read() };

        common::trace!("filehandle = {filehandle:?}");

        let mut dfilehandle: ws::Win32::Foundation::HANDLE = ptr::null_mut();

        common::trace!("DuplicateHandle");
        let ret = unsafe {
            ws::Win32::Foundation::DuplicateHandle(
                ws::Win32::System::Threading::GetCurrentProcess(),
                filehandle,
                ws::Win32::System::Threading::GetCurrentProcess(),
                &mut dfilehandle,
                0,
                ws::Win32::Foundation::FALSE,
                ws::Win32::Foundation::DUPLICATE_SAME_ACCESS,
            )
        };
        if ret == ws::Win32::Foundation::FALSE {
            let err = unsafe { ws::Win32::Foundation::GetLastError() };
            return Err(super::Error::DuplicateHandleFailed(err));
        }
        common::trace!("duplicated filehandle = {dfilehandle:?}");

        let h_event = unsafe {
            ws::Win32::System::Threading::CreateEventA(
                ptr::null(),
                ws::Win32::Foundation::FALSE,
                ws::Win32::Foundation::FALSE,
                ptr::null(),
            )
        };

        if h_event.is_null() {
            let err = unsafe { ws::Win32::Foundation::GetLastError() };
            return Err(super::Error::CreateEventFailed(err));
        }

        let anonymous = ws::Win32::System::IO::OVERLAPPED_0 {
            Pointer: ptr::null_mut(),
        };

        let read_overlapped = ws::Win32::System::IO::OVERLAPPED {
            Internal: 0,
            InternalHigh: 0,
            Anonymous: anonymous,
            hEvent: h_event,
        };

        let write_overlapped = ws::Win32::System::IO::OVERLAPPED {
            Internal: 0,
            InternalHigh: 0,
            Anonymous: anonymous,
            hEvent: ptr::null_mut(),
        };

        let read_overlapped = cell::RefCell::new(read_overlapped);
        let write_overlapped = cell::RefCell::new(write_overlapped);

        Ok(Handle {
            filehandle: dfilehandle,
            read_overlapped,
            write_overlapped,
        })
    }
}

pub(crate) struct Handle {
    filehandle: ws::Win32::Foundation::HANDLE,
    read_overlapped: cell::RefCell<ws::Win32::System::IO::OVERLAPPED>,
    write_overlapped: cell::RefCell<ws::Win32::System::IO::OVERLAPPED>,
}

// Because of the *mut content (handle but also in OVERLAPPED
// structure) Rust does not derive Send and Sync. Since we know how
// those data will be used (especially in terms of concurrency) we
// assume to unsafely implement Send and Sync.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl super::Handler for Handle {
    fn read(&self, data: &mut [u8]) -> Result<usize, super::Error> {
        let to_read = os::raw::c_uint::try_from(data.len())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let mut read = 0;

        let mut overlapped = self.read_overlapped.borrow_mut();

        let ret = unsafe {
            ws::Win32::Storage::FileSystem::ReadFile(
                self.filehandle,
                data.as_mut_ptr(),
                to_read,
                &mut read,
                &mut *overlapped,
            )
        };

        if ret == 0 {
            let err = unsafe { ws::Win32::Foundation::GetLastError() };
            if err == ws::Win32::Foundation::ERROR_IO_PENDING {
                let mut read = 0;
                let ret = unsafe {
                    ws::Win32::System::IO::GetOverlappedResult(
                        self.filehandle,
                        &*overlapped,
                        &mut read,
                        ws::Win32::Foundation::TRUE,
                    )
                };
                if ret == ws::Win32::Foundation::FALSE {
                    let err = unsafe { ws::Win32::Foundation::GetLastError() };
                    Err(super::Error::VirtualChannelReadFailed(err))
                } else {
                    Ok(read as usize)
                }
            } else {
                Err(super::Error::VirtualChannelReadFailed(err))
            }
        } else {
            Ok(read as usize)
        }
    }

    fn write(&self, data: &[u8]) -> Result<usize, super::Error> {
        let to_write = os::raw::c_uint::try_from(data.len())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let mut written = 0;

        let mut overlapped = self.write_overlapped.borrow_mut();

        let ret = unsafe {
            ws::Win32::Storage::FileSystem::WriteFile(
                self.filehandle,
                data.as_ptr(),
                to_write,
                &mut written,
                &mut *overlapped,
            )
        };

        if ret == ws::Win32::Foundation::FALSE {
            let err = unsafe { ws::Win32::Foundation::GetLastError() };
            if err == ws::Win32::Foundation::ERROR_IO_PENDING {
                let mut written = 0;
                let ret = unsafe {
                    ws::Win32::System::IO::GetOverlappedResult(
                        self.filehandle,
                        &*overlapped,
                        &mut written,
                        ws::Win32::Foundation::TRUE,
                    )
                };
                if ret == ws::Win32::Foundation::FALSE {
                    let err = unsafe { ws::Win32::Foundation::GetLastError() };
                    Err(super::Error::VirtualChannelReadFailed(err))
                } else {
                    Ok(written as usize)
                }
            } else {
                Err(super::Error::VirtualChannelWriteFailed(err))
            }
        } else {
            Ok(written as usize)
        }
    }
}
