#[cfg(feature = "service-input")]
use crate::client;
use crate::{control, vc};
use std::{ffi, mem, ops::Deref, ptr, slice, sync};

mod headers;

pub struct Dvc {
    #[cfg(feature = "service-input")]
    client: Option<client::Client>,
}

unsafe impl Send for Dvc {}
unsafe impl Sync for Dvc {}

impl vc::VirtualChannel for Dvc {
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
    channel: *mut headers::IWTSVirtualChannel,
    write: unsafe extern "C" fn(
        channel: *mut headers::IWTSVirtualChannel,
        buffer_size: headers::ULONG,
        buffer: *const headers::BYTE,
        reserved: headers::LPVOID,
    ) -> headers::UINT,
    close: unsafe extern "C" fn(channel: *mut headers::IWTSVirtualChannel) -> headers::UINT,
}

unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl vc::Handle for Handle {
    fn write(&self, data: Vec<u8>) -> Result<(), vc::Error> {
        let len = headers::ULONG::try_from(data.len()).expect("data too large!");
        let ret = unsafe { (self.write)(self.channel, len, data.as_ptr(), ptr::null_mut()) };

        if ret != headers::CHANNEL_RC_OK {
            return Err(vc::Error::VirtualChannel(ret));
        }

        Ok(())
    }

    fn close(&mut self) -> Result<(), vc::Error> {
        let ret = unsafe { (self.close)(self.channel) };

        if ret != headers::CHANNEL_RC_OK {
            return Err(vc::Error::VirtualChannel(ret));
        }

        Ok(())
    }
}

extern "C" fn channel_on_data_received(
    _callback: *mut headers::IWTSVirtualChannelCallback,
    data: *mut headers::wStream,
) -> headers::UINT {
    common::trace!("CALLED channel_on_data_received");

    let Some(data) = (unsafe { data.as_mut() }) else {
        common::error!("invalid data");
        return 1;
    };

    let len = usize::try_from(data.length).expect("value too large!");

    if 0 < len {
        let buffer = unsafe { slice::from_raw_parts_mut(data.buffer, len) };

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

    headers::CHANNEL_RC_OK
}

extern "C" fn channel_on_open(
    _callback: *mut headers::IWTSVirtualChannelCallback,
) -> headers::UINT {
    common::debug!("CALLED channel_on_open");
    headers::CHANNEL_RC_OK
}

extern "C" fn channel_on_close(
    _callback: *mut headers::IWTSVirtualChannelCallback,
) -> headers::UINT {
    common::debug!("CALLED channel_on_close");

    crate::CONTROL
        .deref()
        .channel_connector()
        .send(control::FromVc::Closed)
        .expect("internal error: failed to send control message");

    headers::CHANNEL_RC_OK
}

struct Channel(headers::IWTSVirtualChannelCallback);

unsafe impl Sync for Channel {}
unsafe impl Send for Channel {}

static CHANNEL: sync::LazyLock<Channel> = sync::LazyLock::new(|| {
    Channel(headers::IWTSVirtualChannelCallback {
        OnDataReceived: Some(channel_on_data_received),
        OnOpen: Some(channel_on_open),
        OnClose: Some(channel_on_close),
        pInterface: ptr::null_mut(),
    })
});

extern "C" fn listener_on_new_channel_connection(
    _listener: *mut headers::IWTSListenerCallback,
    channel: *mut headers::IWTSVirtualChannel,
    _data: *mut headers::BYTE,
    accept: *mut headers::BOOL,
    callback: *mut *mut headers::IWTSVirtualChannelCallback,
) -> headers::UINT {
    common::debug!("CALLED listener_on_new_channel_connection");

    let Some(accept) = (unsafe { accept.as_mut() }) else {
        common::error!("invalid accept");
        return 1;
    };

    if crate::CONTROL.deref().is_opened() {
        common::warn!("replacing already opened channel");
    }

    let Some(callback) = (unsafe { callback.as_mut() }) else {
        common::error!("invalid callback");
        return 1;
    };

    *callback = ptr::from_ref(&CHANNEL.0).cast_mut();

    let Some(channel) = (unsafe { channel.as_mut() }) else {
        common::error!("invalid channel");
        return 1;
    };

    let Some(write) = channel.Write else {
        common::error!("invalid write");
        return 1;
    };

    let Some(close) = channel.Close else {
        common::error!("invalid close");
        return 1;
    };

    *accept = headers::TRUE;

    let handle = Handle {
        channel,
        write,
        close,
    };
    let handle = super::Handle::Freerdp(handle);
    let handle = vc::GenericHandle::Dynamic(handle);

    crate::CONTROL
        .deref()
        .channel_connector()
        .send(control::FromVc::Opened(handle))
        .expect("internal error: failed to send control message");

    headers::CHANNEL_RC_OK
}

struct Listener(headers::IWTSListenerCallback);

unsafe impl Sync for Listener {}
unsafe impl Send for Listener {}

static LISTENER: sync::LazyLock<Listener> = sync::LazyLock::new(|| {
    Listener(headers::IWTSListenerCallback {
        OnNewChannelConnection: Some(listener_on_new_channel_connection),
        pInterface: ptr::null_mut(),
    })
});

extern "C" fn plugin_initialize(
    _plugin: *mut headers::IWTSPlugin,
    channel_manager: *mut headers::IWTSVirtualChannelManager,
) -> headers::UINT {
    common::debug!("CALLED plugin_initialize {channel_manager:?}");

    let Some(channel_manager) = (unsafe { channel_manager.as_mut() }) else {
        common::error!("invalid channel_manager");
        return 1;
    };

    let Some(create_listener) = channel_manager.CreateListener else {
        common::error!("missing CreateListener function");
        return 1;
    };

    let Some(config) = crate::CONFIG.get() else {
        common::error!("no config loaded!");
        return 1;
    };

    common::info!("DVC(freerdp) name is {:?}", config.channel);

    let name = match common::virtual_channel_name(&config.channel) {
        Err(e) => {
            common::error!("{e}");
            return 1;
        }
        Ok(name) => name,
    };

    let flags = 0;

    common::debug!("create listener for channel {name:?}");

    let ret = unsafe {
        create_listener(
            ptr::from_mut(channel_manager),
            name.as_ptr().cast_mut(),
            flags,
            ptr::from_ref(&LISTENER.0).cast_mut(),
            ptr::null_mut(),
        )
    };
    if ret != 0 {
        common::error!("failed to create listener: 0x{ret:x?}");
        return ret;
    }

    headers::CHANNEL_RC_OK
}

extern "C" fn plugin_connected(_plugin: *mut headers::IWTSPlugin) -> headers::UINT {
    common::debug!("CALLED plugin_connected");
    headers::CHANNEL_RC_OK
}

extern "C" fn plugin_disconnected(
    _plugin: *mut headers::IWTSPlugin,
    disconnect_code: headers::DWORD,
) -> headers::UINT {
    common::debug!("CALLED plugin_disconnected {disconnect_code}");
    headers::CHANNEL_RC_OK
}

extern "C" fn plugin_terminated(_plugin: *mut headers::IWTSPlugin) -> headers::UINT {
    common::debug!("CALLED plugin_terminated");

    crate::CONTROL
        .deref()
        .channel_connector()
        .send(control::FromVc::Terminated)
        .expect("internal error: failed to send control message");

    headers::CHANNEL_RC_OK
}

extern "C" fn plugin_attached(_plugin: *mut headers::IWTSPlugin) -> headers::UINT {
    common::debug!("CALLED plugin_attached");
    headers::CHANNEL_RC_OK
}

extern "C" fn plugin_detached(_plugin: *mut headers::IWTSPlugin) -> headers::UINT {
    common::debug!("CALLED plugin_detached");
    headers::CHANNEL_RC_OK
}

struct Plugin(headers::IWTSPlugin);

unsafe impl Sync for Plugin {}
unsafe impl Send for Plugin {}

static PLUGIN: sync::LazyLock<Plugin> = sync::LazyLock::new(|| {
    Plugin(headers::IWTSPlugin {
        Initialize: Some(plugin_initialize),
        Connected: Some(plugin_connected),
        Disconnected: Some(plugin_disconnected),
        Terminated: Some(plugin_terminated),
        Attached: Some(plugin_attached),
        Detached: Some(plugin_detached),
        pInterface: ptr::null_mut(),
    })
});

#[unsafe(no_mangle)]
extern "C" fn DVCPluginEntry(entry_points: *mut headers::IDRDYNVC_ENTRY_POINTS) -> headers::UINT {
    crate::init();

    common::trace!("entry_points: {entry_points:?}");

    let ep = unsafe { *entry_points };

    common::trace!("ep.RegisterPlugin: {:?}", ep.RegisterPlugin);
    common::trace!("ep.GetPlugin: {:?}", ep.GetPlugin);
    common::trace!("ep.GetPluginData: {:?}", ep.GetPluginData);
    common::trace!("ep.GetRdpSettings: {:?}", ep.GetRdpSettings);
    common::trace!("ep.GetRdpContext: {:?}", ep.GetRdpContext);

    let Some(register_plugin) = ep.RegisterPlugin else {
        common::error!("missing RegisterPlugin function");
        return 1;
    };

    let mut name = Vec::from(env!("CARGO_CRATE_NAME").as_bytes());
    name.push(0);

    let name =
        ffi::CStr::from_bytes_with_nul(&name).expect("failed to convert crate name to plugin name");

    common::debug!("registering plugin {name:?}");

    let ret = unsafe {
        register_plugin(
            entry_points,
            name.as_ptr(),
            ptr::from_ref(&PLUGIN.0).cast_mut(),
        )
    };

    if ret != 0 {
        common::error!("failed to register plugin");
        return ret;
    }

    let epsize =
        u32::try_from(mem::size_of::<headers::IDRDYNVC_ENTRY_POINTS>()).expect("size_of too large");

    #[cfg(feature = "service-input")]
    let dvc = Dvc {
        client: client::Client::load_from_entrypoints(epsize, entry_points.cast()),
    };
    #[cfg(not(feature = "service-input"))]
    let dvc = Dvc {};

    let dvc = super::Dvc::Freerdp(dvc);
    let vc = vc::GenericChannel::Dynamic(dvc);

    crate::CONTROL
        .deref()
        .channel_connector()
        .send(control::FromVc::Loaded(vc))
        .expect("internal error: failed to send control message");

    headers::CHANNEL_RC_OK
}
