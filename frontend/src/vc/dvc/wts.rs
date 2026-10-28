#[cfg(feature = "service-input")]
use crate::client;
use crate::{control, vc};
use std::{ffi, mem, ops::Deref, ptr, result, slice};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::RemoteDesktop::*;
use windows::core::*;

pub struct Dvc {
    #[cfg(feature = "service-input")]
    client: Option<client::Client>,
}

unsafe impl Sync for Dvc {}
unsafe impl Send for Dvc {}

impl vc::VirtualChannel for Dvc {
    fn open(&mut self) -> result::Result<(), vc::Error> {
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

    fn terminate(&mut self) -> result::Result<(), vc::Error> {
        Ok(())
    }
}

pub struct Handle {
    channel: IWTSVirtualChannel,
}

unsafe impl Sync for Handle {}
unsafe impl Send for Handle {}

impl vc::Handle for Handle {
    #[allow(clippy::cast_sign_loss)]
    fn write(&self, data: Vec<u8>) -> result::Result<(), vc::Error> {
        unsafe { self.channel.Write(&data, None) }
            .map_err(|e| vc::Error::VirtualChannel(e.code().0 as u32))
    }

    #[allow(clippy::cast_sign_loss)]
    fn close(&mut self) -> result::Result<(), vc::Error> {
        unsafe { self.channel.Close() }.map_err(|e| vc::Error::VirtualChannel(e.code().0 as u32))
    }
}

#[implement(IWTSPlugin, IWTSListenerCallback, IWTSVirtualChannelCallback)]
struct Plugin();

impl IWTSVirtualChannelCallback_Impl for Plugin_Impl {
    fn OnDataReceived(&self, len: u32, data: *const u8) -> Result<()> {
        common::trace!("CALLED OnDataReceived {len}");

        let len = usize::try_from(len).expect("value too large!");

        if 0 < len {
            let buffer = unsafe { slice::from_raw_parts(data, len) };

            match super::Pdu::parse(buffer) {
                Err(e) => {
                    common::warn!("failed to parse PDU: {e}; assuming this is raw data");
                    let data = Vec::from(buffer);

                    crate::CONTROL
                        .deref()
                        .channel_connector()
                        .send(control::FromVc::Data(data))
                        .expect("internal error: failed to send control message");
                }
                Ok((_pdu, skip)) => {
                    let data = Vec::from(&buffer[skip..]);

                    crate::CONTROL
                        .deref()
                        .channel_connector()
                        .send(control::FromVc::Data(data))
                        .expect("internal error: failed to send control message");
                }
            }
        }

        Ok(())
    }

    fn OnClose(&self) -> Result<()> {
        common::debug!("CALLED OnClose");

        crate::CONTROL
            .deref()
            .channel_connector()
            .send(control::FromVc::Closed)
            .map_err(|e| Error::new(E_FAIL, e.to_string()))
    }
}

impl IWTSListenerCallback_Impl for Plugin_Impl {
    fn OnNewChannelConnection(
        &self,
        channel: Ref<IWTSVirtualChannel>,
        _data: &BSTR,
        accept: *mut BOOL,
        callback: OutRef<IWTSVirtualChannelCallback>,
    ) -> Result<()> {
        common::debug!("CALLED OnNewChannelConnection");

        if crate::CONTROL.deref().is_opened() {
            if let Some(accept) = unsafe { accept.as_mut() } {
                common::warn!("refused new virtual channel connection since one is already opened");
                *accept = FALSE;
            }
            return Ok(());
        }

        let channel = channel.unwrap().clone();

        let oself: IWTSVirtualChannelCallback = self
            .as_interface::<IWTSVirtualChannelCallback>()
            .cast()
            .inspect_err(|e| common::error!("failed to cast virtual channel callback: {e}"))?;

        callback
            .write(Some(oself))
            .inspect_err(|e| common::error!("failed to write virtual channel callback: {e}"))?;

        if let Some(accept) = unsafe { accept.as_mut() } {
            *accept = TRUE;
        }

        let handle = Handle { channel };

        let handle = super::Handle::Wts(handle);
        let handle = vc::GenericHandle::Dynamic(handle);

        crate::CONTROL
            .deref()
            .channel_connector()
            .send(control::FromVc::Opened(handle))
            .map_err(|e| Error::new(E_FAIL, e.to_string()))
    }
}

impl IWTSPlugin_Impl for Plugin_Impl {
    fn Initialize(&self, channel_manager: Ref<IWTSVirtualChannelManager>) -> Result<()> {
        let config = crate::bootstrap()
            .inspect_err(|e| eprintln!("{e}"))
            .map_err(|e| Error::new(E_FAIL, e.to_string()))?;

        common::debug!("CALLED Initialize");

        common::info!("DVC(WTS) name is {:?}", config.channel);

        let mut name = match common::virtual_channel_name(&config.channel) {
            Err(e) => {
                common::error!("{e}");
                return Err(Error::new(E_INVALIDARG, &e));
            }
            Ok(name) => name,
        };

        let flags = 0;

        common::debug!("create listener for channel {name:?}");

        match channel_manager.as_ref() {
            None => {
                common::error!("channel manager is null");
                return Err(Error::new(E_INVALIDARG, "channel manager is null"));
            }
            Some(cm) => {
                let _ = unsafe {
                    cm.CreateListener(
                        PSTR(name.as_mut_ptr() as *mut u8),
                        flags,
                        self.as_interface_ref(),
                    )?
                };
            }
        }

        #[cfg(feature = "service-input")]
        let dvc = Dvc {
            client: client::Client::load_from_entrypoints(0, ptr::null_mut()),
        };
        #[cfg(not(feature = "service-input"))]
        let dvc = Dvc {};

        let dvc = super::Dvc::Wts(dvc);
        let vc = vc::GenericChannel::Dynamic(dvc);

        crate::init();

        crate::CONTROL
            .deref()
            .channel_connector()
            .send(control::FromVc::Loaded(vc))
            .expect("internal error: failed to send control message");

        Ok(())
    }

    fn Connected(&self) -> Result<()> {
        common::debug!("CALLED Connected");
        Ok(())
    }

    fn Disconnected(&self, disconnect_code: u32) -> Result<()> {
        common::debug!("CALLED Disconnected {disconnect_code}");
        Ok(())
    }

    fn Terminated(&self) -> Result<()> {
        common::debug!("CALLED Terminated");

        crate::CONTROL
            .deref()
            .channel_connector()
            .send(control::FromVc::Terminated)
            .expect("internal error: failed to send control message");

        Ok(())
    }
}

#[implement(IClassFactory)]
struct PluginFactory();

impl IClassFactory_Impl for PluginFactory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<'_, IUnknown>,
        iid: *const GUID,
        ppobject: *mut *mut ffi::c_void,
    ) -> Result<()> {
        let iid = unsafe { *iid };
        let ppobject = unsafe { &mut *ppobject };

        if outer.is_some() {
            return Err(Error::from(CLASS_E_NOAGGREGATION));
        }

        *ppobject = ptr::null_mut();

        match iid {
            IWTSPlugin::IID => {
                let plugin: IWTSPlugin = Plugin().into();
                *ppobject = unsafe { mem::transmute::<IWTSPlugin, *mut ffi::c_void>(plugin) };
            }
            _ => return Err(Error::from(E_NOINTERFACE)),
        }

        Ok(())
    }

    fn LockServer(&self, _lock: BOOL) -> Result<()> {
        Ok(())
    }
}

#[unsafe(no_mangle)]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    let _ = crate::bootstrap().inspect_err(|e| eprintln!("{e}"));

    common::debug!("CALLED DllCanUnloadNow");

    S_OK
}

#[unsafe(no_mangle)]
extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    _riid: *const GUID,
    ppv: *mut *mut ffi::c_void,
) -> HRESULT {
    let _ = crate::bootstrap().inspect_err(|e| eprintln!("{e}"));

    common::debug!("CALLED DllGetClassObject");

    let factory = PluginFactory();

    unsafe { ppv.as_mut() }.map_or(E_FAIL, |ppv| {
        if unsafe { *rclsid } == GUID::from_u128(soxyreg::PLUGIN_GUID) {
            *ppv = unsafe { mem::transmute::<IClassFactory, *mut ffi::c_void>(factory.into()) };
            S_OK
        } else {
            *ppv = ptr::null_mut();
            CLASS_E_CLASSNOTAVAILABLE
        }
    })
}
