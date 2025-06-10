use std::{io, os, ptr, thread};
use windows_sys as ws;

pub struct Svc<'a> {
    open: libloading::Symbol<'a, super::VirtualChannelOpen>,
    query: libloading::Symbol<'a, super::VirtualChannelQuery>,
    read: libloading::Symbol<'a, super::VirtualChannelRead>,
    write: libloading::Symbol<'a, super::VirtualChannelWrite>,
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
                read: lib.get(symbols.read.as_bytes())?,
                write: lib.get(symbols.write.as_bytes())?,
            })
        }
    }

    pub(crate) fn open(&self, mut name: [i8; 8]) -> Result<Handle<'_>, super::Error> {
        let wtshandle = unsafe {
            (self.open)(
                ws::Win32::System::RemoteDesktop::WTS_CURRENT_SERVER_HANDLE,
                ws::Win32::System::RemoteDesktop::WTS_CURRENT_SESSION,
                name.as_mut_ptr(),
            )
        };

        if wtshandle.is_null() {
            let err = io::Error::last_os_error();
            return Err(super::Error::VirtualChannelOpenStaticChannelFailed(err));
        }

        let mut client_dataptr = ptr::null_mut();
        let mut len = 0;

        let ret = unsafe {
            (self.query)(
                wtshandle,
                ws::Win32::System::RemoteDesktop::WTSVirtualClientData,
                ptr::from_mut(&mut client_dataptr),
                &mut len,
            )
        };

        if ret == ws::Win32::Foundation::FALSE {
            let err = io::Error::last_os_error();
            common::warn!("virtual channel query failed (len = {len}, last error = {err})");
        }

        Ok(Handle {
            read: self.read.clone(),
            write: self.write.clone(),
            wtshandle,
        })
    }
}

pub struct Handle<'a> {
    read: libloading::Symbol<'a, super::VirtualChannelRead>,
    write: libloading::Symbol<'a, super::VirtualChannelWrite>,
    wtshandle: ws::Win32::Foundation::HANDLE,
}

// Because of the *mut content (handle) Rust does not derive Send and
// Sync. Since we know how those data will be used (especially in
// terms of concurrency) we assume to unsafely implement Send and
// Sync.
unsafe impl Send for Handle<'_> {}
unsafe impl Sync for Handle<'_> {}

impl super::Handler for Handle<'_> {
    fn read(&self, data: &mut [u8]) -> Result<usize, super::Error> {
        let to_read = os::raw::c_ulong::try_from(data.len())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let timeout = os::raw::c_ulong::MAX;

        let mut read = 0;

        let ret = unsafe {
            (self.read)(
                self.wtshandle,
                timeout,
                data.as_mut_ptr(),
                to_read,
                &mut read,
            )
        };

        if ret == 0 {
            let err = io::Error::last_os_error();
            Err(super::Error::VirtualChannelReadFailed(err))
        } else {
            Ok(usize::try_from(read)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?)
        }
    }

    fn write(&self, data: &[u8]) -> Result<usize, super::Error> {
        let to_write = os::raw::c_ulong::try_from(data.len())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let mut written = 0;

        common::trace!("write {to_write} bytes");

        loop {
            let ret =
                unsafe { (self.write)(self.wtshandle, data.as_ptr(), to_write, &mut written) };

            if ret == 0 || written != to_write {
                if written == 0 {
                    common::trace!("send buffer seems full, yield now");
                    thread::yield_now();
                    continue;
                }
                if written != to_write {
                    common::error!("partial write: {written} / {to_write}");
                }
                let err = io::Error::last_os_error();
                return Err(super::Error::VirtualChannelWriteFailed(err));
            }

            return Ok(usize::try_from(written)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?);
        }
    }
}
