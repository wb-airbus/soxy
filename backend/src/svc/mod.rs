use std::{ffi, fmt, io, os};
use windows_sys as ws;

mod high;
#[cfg(target_os = "windows")]
mod low;

pub enum Error {
    LibraryNotFound,
    LibraryLoading(libloading::Error),
    #[cfg(target_os = "windows")]
    WsaStartupFailed(i32),
    Io(io::Error),
    VirtualChannelOpenStaticChannelFailed(io::Error),
    VirtualChannelReadFailed(io::Error),
    VirtualChannelWriteFailed(io::Error),
    #[cfg(target_os = "windows")]
    VirtualChannelQueryFailed(io::Error),
    #[cfg(target_os = "windows")]
    DuplicateHandleFailed(io::Error),
    #[cfg(target_os = "windows")]
    CreateEventFailed(io::Error),
    InvalidChannelName,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::LibraryNotFound => write!(f, "library not found"),
            Self::LibraryLoading(e) => write!(f, "library loading error: {e}"),
            #[cfg(target_os = "windows")]
            Self::WsaStartupFailed(e) => write!(f, "WSAStartup failed with error code {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::VirtualChannelOpenStaticChannelFailed(err) => {
                write!(f, "virtual channel open failed (last_error = {err})")
            }
            Self::VirtualChannelReadFailed(err) => {
                write!(f, "virtual channel read failed (last error = {err})")
            }
            Self::VirtualChannelWriteFailed(err) => {
                write!(f, "virtual channel write failed (last error = {err})")
            }
            #[cfg(target_os = "windows")]
            Self::VirtualChannelQueryFailed(err) => {
                write!(f, "virtual channel query failed (last error = {err})")
            }
            #[cfg(target_os = "windows")]
            Self::DuplicateHandleFailed(err) => {
                write!(f, "duplicate handle failed (last error = {err})")
            }
            #[cfg(target_os = "windows")]
            Self::CreateEventFailed(err) => {
                write!(f, "create event failed (last error = {err})")
            }
            Self::InvalidChannelName => {
                write!(f, "invalid channel name")
            }
        }
    }
}

impl From<libloading::Error> for Error {
    fn from(e: libloading::Error) -> Self {
        Self::LibraryLoading(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Clone, Copy, Debug)]
enum Instance {
    Citrix,
    Horizon,
    Xrdp,
    #[cfg(target_os = "windows")]
    Windows,
}

pub struct SymbolNames {
    open: &'static str,
    read: &'static str,
    write: &'static str,
    query: &'static str,
}

impl From<Instance> for SymbolNames {
    fn from(instance: Instance) -> Self {
        match instance {
            Instance::Citrix => Self {
                open: "WFVirtualChannelOpen",
                read: "WFVirtualChannelRead",
                write: "WFVirtualChannelWrite",
                query: "WFVirtualChannelQuery",
            },
            Instance::Horizon => Self {
                open: "VDP_VirtualChannelOpen",
                read: "VDP_VirtualChannelRead",
                write: "VDP_VirtualChannelWrite",
                query: "VDP_VirtualChannelQuery",
            },
            Instance::Xrdp => Self {
                open: "WTSVirtualChannelOpen",
                read: "WTSVirtualChannelRead",
                write: "WTSVirtualChannelWrite",
                query: "WTSVirtualChannelQuery",
            },
            #[cfg(target_os = "windows")]
            Instance::Windows => Self {
                open: "WTSVirtualChannelOpen",
                read: "WTSVirtualChannelRead",
                write: "WTSVirtualChannelWrite",
                query: "WTSVirtualChannelQuery",
            },
        }
    }
}

pub struct Implementation {
    instance: Instance,
    lib: libloading::Library,
}

impl Implementation {
    pub(crate) fn load() -> Result<Self, Error> {
        unsafe {
            let file = libloading::library_filename("wfapi64");
            common::debug!(
                "trying to load Citrix library from {}",
                file.to_string_lossy()
            );
            if let Ok(lib) = libloading::Library::new(file) {
                common::info!("Citrix library loaded");
                return Ok(Self {
                    instance: Instance::Citrix,
                    lib,
                });
            }

            let file = libloading::library_filename("vdp_rdpvcbridge");
            common::debug!(
                "trying to load Horizon library from {}",
                file.to_string_lossy()
            );
            if let Ok(lib) = libloading::Library::new(file) {
                common::info!("Horizon library loaded");
                return Ok(Self {
                    instance: Instance::Horizon,
                    lib,
                });
            }

            #[cfg(target_os = "windows")]
            {
                let file = libloading::library_filename("wtsapi32");
                common::debug!("trying to load WTS library from {}", file.to_string_lossy());
                if let Ok(lib) = libloading::Library::new(file) {
                    common::info!("WTS library loaded");
                    return Ok(Self {
                        instance: Instance::Windows,
                        lib,
                    });
                }
            }

            let file = libloading::library_filename("xrdpapi");
            common::debug!(
                "trying to load XRDP library from {}",
                file.to_string_lossy()
            );
            if let Ok(lib) = libloading::Library::new(file) {
                common::info!("XRDP library loaded");
                return Ok(Self {
                    instance: Instance::Xrdp,
                    lib,
                });
            }

            Err(Error::LibraryNotFound)
        }
    }
}

type VirtualChannelOpen = unsafe extern "system" fn(
    hserver: ws::Win32::Foundation::HANDLE,
    sessionid: os::raw::c_uint,
    pvirtualname: *mut os::raw::c_char,
) -> ws::Win32::Foundation::HANDLE;

type VirtualChannelRead = unsafe extern "system" fn(
    hchannelhandle: ws::Win32::Foundation::HANDLE,
    timeout: os::raw::c_ulong,
    buffer: *mut os::raw::c_uchar,
    buffersize: os::raw::c_ulong,
    pbytesread: *mut os::raw::c_ulong,
) -> os::raw::c_int;

type VirtualChannelWrite = unsafe extern "system" fn(
    hchannelhandle: ws::Win32::Foundation::HANDLE,
    buffer: *const os::raw::c_uchar,
    length: os::raw::c_ulong,
    pbyteswritten: *mut os::raw::c_ulong,
) -> os::raw::c_uint;

type VirtualChannelQuery = unsafe extern "system" fn(
    hchannelhandle: ws::Win32::Foundation::HANDLE,
    wtsvirtualclass: ws::Win32::System::RemoteDesktop::WTS_VIRTUAL_CLASS,
    ppbuffer: *mut *mut os::raw::c_void,
    pbytesreturned: *mut os::raw::c_ulong,
) -> ws::Win32::Foundation::BOOL;

pub enum Svc<'a> {
    High {
        svc: high::Svc<'a>,
    },
    #[cfg(target_os = "windows")]
    Low {
        svc: low::Svc<'a>,
    },
}

impl<'a> Svc<'a> {
    pub(crate) fn load(implem: &'a Implementation) -> Result<Self, Error> {
        let symbol_names = SymbolNames::from(implem.instance);
        match implem.instance {
            Instance::Citrix => {
                let svc = high::Svc::load(&implem.lib, &symbol_names)?;
                Ok(Self::High { svc })
            }
            Instance::Horizon => {
                let svc = high::Svc::load(&implem.lib, &symbol_names)?;
                Ok(Self::High { svc })
            }
            Instance::Xrdp => {
                #[cfg(feature = "log")]
                {
                    common::debug!("initiate XRDP logging");

                    let log_init = unsafe {
                        implem.lib.get::<fn(
                            os::raw::c_int,
                            *mut os::raw::c_void,
                        ) -> *mut os::raw::c_void>(
                            "log_config_init_for_console".as_bytes()
                        )?
                    };
                    let log_start = unsafe {
                        implem
                            .lib
                            .get::<fn(*mut os::raw::c_void)>("log_start_from_param".as_bytes())?
                    };

                    let lc = log_init(4, std::ptr::null_mut());

                    if !lc.is_null() {
                        log_start(lc);
                    }
                }

                let svc = high::Svc::load(&implem.lib, &symbol_names)?;
                Ok(Self::High { svc })
            }
            #[cfg(target_os = "windows")]
            Instance::Windows => {
                let svc = low::Svc::load(&implem.lib, &symbol_names)?;
                Ok(Self::Low { svc })
            }
        }
    }

    pub(crate) fn open(&'a self, name: &ffi::CStr) -> Result<Handle<'a>, Error> {
        let mut cname: [ffi::c_char; 8] = [0; 8];
        for (i, b) in name.to_bytes_with_nul().iter().enumerate() {
            cname[i] = i8::try_from(*b).map_err(|_| Error::InvalidChannelName)?;
        }

        match self {
            Self::High { svc } => Ok(Handle::from(svc.open(cname)?)),
            #[cfg(target_os = "windows")]
            Self::Low { svc } => Ok(Handle::from(svc.open(cname)?)),
        }
    }
}

pub enum Handle<'a> {
    High {
        handle: high::Handle<'a>,
    },
    #[cfg(target_os = "windows")]
    Low {
        handle: low::Handle,
    },
}

impl<'a> From<high::Handle<'a>> for Handle<'a> {
    fn from(handle: high::Handle<'a>) -> Self {
        Self::High { handle }
    }
}

#[cfg(target_os = "windows")]
impl From<low::Handle> for Handle<'_> {
    fn from(handle: low::Handle) -> Self {
        Self::Low { handle }
    }
}

impl Handler for Handle<'_> {
    fn read(&self, data: &mut [u8]) -> Result<usize, Error> {
        match self {
            Self::High { handle } => handle.read(data),
            #[cfg(target_os = "windows")]
            Self::Low { handle } => handle.read(data),
        }
    }

    fn write(&self, data: &[u8]) -> Result<usize, Error> {
        match self {
            Self::High { handle } => handle.write(data),
            #[cfg(target_os = "windows")]
            Self::Low { handle } => handle.write(data),
        }
    }
}

pub trait Handler {
    fn read(&self, data: &mut [u8]) -> Result<usize, Error>;
    fn write(&self, data: &[u8]) -> Result<usize, Error>;
}
