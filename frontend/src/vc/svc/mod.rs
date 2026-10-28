#[cfg(feature = "service-input")]
use crate::client;
use crate::vc;

mod citrix;
mod rdp;
mod semaphore;

const MAX_CHUNKS_IN_FLIGHT: usize = 64;

pub enum Svc {
    Citrix(citrix::Svc),
    Rdp(rdp::Svc),
}

impl vc::VirtualChannel for Svc {
    fn open(&mut self) -> Result<(), vc::Error> {
        match self {
            Self::Citrix(svc) => svc.open(),
            Self::Rdp(svc) => svc.open(),
        }
    }

    #[cfg(feature = "service-input")]
    fn client(&self) -> Option<&client::Client> {
        match self {
            Self::Citrix(svc) => svc.client(),
            Self::Rdp(svc) => svc.client(),
        }
    }

    #[cfg(feature = "service-input")]
    fn client_mut(&mut self) -> Option<&mut client::Client> {
        match self {
            Self::Citrix(svc) => svc.client_mut(),
            Self::Rdp(svc) => svc.client_mut(),
        }
    }

    fn terminate(&mut self) -> Result<(), vc::Error> {
        match self {
            Self::Citrix(svc) => svc.terminate(),
            Self::Rdp(svc) => svc.terminate(),
        }
    }
}

pub enum Handle {
    Citrix(citrix::Handle),
    Rdp(rdp::Handle),
}

impl vc::Handle for Handle {
    fn write(&self, data: Vec<u8>) -> Result<(), vc::Error> {
        match self {
            Self::Citrix(svc) => svc.write(data),
            Self::Rdp(svc) => svc.write(data),
        }
    }

    fn close(&mut self) -> Result<(), vc::Error> {
        match self {
            Self::Citrix(svc) => svc.close(),
            Self::Rdp(svc) => svc.close(),
        }
    }
}
