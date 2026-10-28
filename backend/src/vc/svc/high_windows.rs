use crate::vc;
use std::{ffi, io, ops, os, ptr, thread};
use windows_sys as ws;

pub struct Svc<'a> {
    open: libloading::Symbol<'a, vc::VirtualChannelOpen>,
    query: libloading::Symbol<'a, vc::VirtualChannelQuery>,
    read: libloading::Symbol<'a, vc::VirtualChannelRead>,
    write: libloading::Symbol<'a, vc::VirtualChannelWrite>,
    close: libloading::Symbol<'a, vc::VirtualChannelClose>,
}

impl<'a> vc::VirtualChannel<'a> for Svc<'a> {
    type Handle = Handle<'a>;

    fn load(libs: &'a vc::Libraries) -> Result<Self, vc::Error> {
        if let Some(citrix) = libs.citrix() {
            unsafe {
                Ok(Self {
                    open: citrix.get(b"WFVirtualChannelOpen")?,
                    query: citrix.get(b"WFVirtualChannelQuery")?,
                    read: citrix.get(b"WFVirtualChannelRead")?,
                    write: citrix.get(b"WFVirtualChannelWrite")?,
                    close: citrix.get(b"WFVirtualChannelClose")?,
                })
            }
        } else if let Some(horizon) = libs.horizon() {
            unsafe {
                Ok(Self {
                    open: horizon.get(b"VDP_VirtualChannelOpen")?,
                    query: horizon.get(b"VDP_VirtualChannelQuery")?,
                    read: horizon.get(b"VDP_VirtualChannelRead")?,
                    write: horizon.get(b"VDP_VirtualChannelWrite")?,
                    close: horizon.get(b"VDP_VirtualChannelClose")?,
                })
            }
        } else {
            Err(vc::Error::NoLibraryFound)
        }
    }

    fn open(&self, name: [ffi::c_char; 8]) -> Result<Self::Handle, vc::Error> {
        common::debug!("trying to open SVC(high)");

        let wtshandle = unsafe {
            (self.open)(
                ws::Win32::System::RemoteDesktop::WTS_CURRENT_SERVER_HANDLE,
                ws::Win32::System::RemoteDesktop::WTS_CURRENT_SESSION,
                name.as_ptr().cast_mut(),
            )
        };

        if wtshandle.is_null() {
            let err = io::Error::last_os_error();
            return Err(vc::Error::OpenChannelFailed(err.to_string()));
        }

        let mut client_dataptr = ptr::null_mut();
        let mut len = 0;

        common::trace!("VirtualChannelQuery");
        let ret = unsafe {
            (self.query)(
                wtshandle,
                ws::Win32::System::RemoteDesktop::WTSVirtualClientData,
                ptr::from_mut(&mut client_dataptr),
                &raw mut len,
            )
        };

        if ret == ws::Win32::Foundation::FALSE {
            let err = io::Error::last_os_error();
            common::warn!("virtual channel query failed (len = {len}, last error = {err})");
        }

        #[cfg(not(feature = "log"))]
        let name = None;

        #[cfg(feature = "log")]
        let name = Some(format!("SVC(high) {:?}", unsafe {
            ffi::CStr::from_ptr(name.as_ptr())
        }));

        Ok(Handle {
            name,
            wtshandle,
            read: self.read.clone(),
            write: self.write.clone(),
            close: self.close.clone(),
        })
    }
}

pub struct Handle<'a> {
    name: Option<String>,
    wtshandle: ws::Win32::Foundation::HANDLE,
    read: libloading::Symbol<'a, vc::VirtualChannelRead>,
    write: libloading::Symbol<'a, vc::VirtualChannelWrite>,
    close: libloading::Symbol<'a, vc::VirtualChannelClose>,
}

// Because of the *mut content (handle) Rust does not derive Send and
// Sync. Since we know how those data will be used (especially in
// terms of concurrency) we assume to unsafely implement Send and
// Sync.
unsafe impl Send for Handle<'_> {}
unsafe impl Sync for Handle<'_> {}

impl vc::Handle for Handle<'_> {
    fn display_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    fn read(&self, data: &mut [u8]) -> Result<ops::Range<usize>, vc::Error> {
        let to_read = os::raw::c_ulong::try_from(data.len())
            .map_err(|e| vc::Error::ReadFailed(e.to_string()))?;

        let timeout = os::raw::c_ulong::MAX;

        let mut read = 0;

        let ret = unsafe {
            (self.read)(
                self.wtshandle,
                timeout,
                data.as_mut_ptr(),
                to_read,
                &raw mut read,
            )
        };

        if ret == ws::Win32::Foundation::FALSE {
            let err = io::Error::last_os_error();
            Err(vc::Error::ReadFailed(err.to_string()))
        } else {
            let read = usize::try_from(read).map_err(|e| vc::Error::ReadFailed(e.to_string()))?;
            Ok(0..read)
        }
    }

    fn write(&self, data: &[u8]) -> Result<usize, vc::Error> {
        let to_write = os::raw::c_ulong::try_from(data.len())
            .map_err(|e| vc::Error::WriteFailed(e.to_string()))?;

        let mut written = 0;

        common::trace!("write {to_write} bytes");

        loop {
            let ret =
                unsafe { (self.write)(self.wtshandle, data.as_ptr(), to_write, &raw mut written) };

            if ret == ws::Win32::Foundation::FALSE || written != to_write {
                if written == 0 {
                    common::trace!("send buffer seems full, yield now");
                    thread::yield_now();
                    continue;
                }
                if written != to_write {
                    common::error!("partial write: {written} / {to_write}");
                }
                let err = io::Error::last_os_error();
                return Err(vc::Error::WriteFailed(err.to_string()));
            }

            return usize::try_from(written).map_err(|e| vc::Error::WriteFailed(e.to_string()));
        }
    }

    fn close(self) -> Result<(), vc::Error> {
        let ret = unsafe { (self.close)(self.wtshandle) };
        if ret == ws::Win32::Foundation::FALSE {
            let err = io::Error::last_os_error();
            Err(vc::Error::CloseChannelFailed(err.to_string()))
        } else {
            Ok(())
        }
    }
}
