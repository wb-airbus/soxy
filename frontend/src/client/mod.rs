use common::input::{self, InputHandler};
use std::fmt;

mod citrix;
mod freerdp;
mod headers;
mod x11;

pub enum Error {
    Citrix(citrix::Error),
    Freerdp(freerdp::Error),
    X11(x11::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::Citrix(e) => write!(f, "Citrix error: {e}"),
            Self::Freerdp(e) => write!(f, "FreeRDP error: {e}"),
            Self::X11(e) => write!(f, "X11 error: {e}"),
        }
    }
}

impl From<citrix::Error> for Error {
    fn from(e: citrix::Error) -> Self {
        match e {
            citrix::Error::X11(e) => Self::X11(e),
            _ => Self::Citrix(e),
        }
    }
}

impl From<freerdp::Error> for Error {
    fn from(e: freerdp::Error) -> Self {
        match e {
            freerdp::Error::X11(e) => Self::X11(e),
            _ => Self::Freerdp(e),
        }
    }
}

impl From<x11::Error> for Error {
    fn from(e: x11::Error) -> Self {
        Self::X11(e)
    }
}

pub enum Client {
    Citrix(citrix::Client),
    Freerdp(freerdp::Client),
}

trait ClientImplementation: input::InputHandler + Sized {
    fn load_from_entrypoints(
        size: headers::DWORD,
        entrypoints: headers::LPVOID,
    ) -> Result<Self, Error>;
}

impl Client {
    pub fn load_from_entrypoints(
        size: headers::DWORD,
        entrypoints: headers::LPVOID,
    ) -> Option<Self> {
        match freerdp::Client::load_from_entrypoints(size, entrypoints) {
            Ok(client) => {
                common::info!("this is a FreeRDP client");
                Some(Self::Freerdp(client))
            }
            Err(e) => {
                common::debug!("this is not FreeRDP: {e}");
                match citrix::Client::load_from_entrypoints(size, entrypoints) {
                    Ok(client) => {
                        common::info!("this is a Citrix client");
                        Some(Self::Citrix(client))
                    }
                    Err(e) => {
                        common::debug!("this is not Citrix: {e}");
                        None
                    }
                }
            }
        }
    }

    pub fn set(&mut self, setting: input::InputSetting) -> Result<(), input::Error> {
        match self {
            Self::Citrix(client) => client.set(setting),
            Self::Freerdp(client) => client.set(setting),
        }
    }

    pub fn play(&self, action: input::InputAction) -> Result<(), input::Error> {
        match self {
            Self::Citrix(client) => client.play(action),
            Self::Freerdp(client) => client.play(action),
        }
    }

    pub fn reset(&mut self) {
        match self {
            Self::Citrix(client) => client.reset(),
            Self::Freerdp(client) => client.reset(),
        }
    }
}
