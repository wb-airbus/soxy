use std::fmt;

#[cfg(feature = "service-input")]
use crate::client;
use crate::vc;

mod freerdp;
#[cfg(target_os = "windows")]
#[allow(clippy::borrow_as_ptr)]
#[allow(clippy::inline_always)]
#[allow(clippy::ptr_as_ptr)]
#[allow(clippy::ref_as_ptr)]
#[allow(clippy::wildcard_imports)]
mod wts;

pub enum Dvc {
    #[cfg(target_os = "windows")]
    Wts(wts::Dvc),
    Freerdp(freerdp::Dvc),
}

impl vc::VirtualChannel for Dvc {
    fn open(&mut self) -> Result<(), vc::Error> {
        match self {
            #[cfg(target_os = "windows")]
            Self::Wts(dvc) => dvc.open(),
            Self::Freerdp(dvc) => dvc.open(),
        }
    }

    #[cfg(feature = "service-input")]
    fn client(&self) -> Option<&client::Client> {
        match self {
            #[cfg(target_os = "windows")]
            Self::Wts(dvc) => dvc.client(),
            Self::Freerdp(dvc) => dvc.client(),
        }
    }

    #[cfg(feature = "service-input")]
    fn client_mut(&mut self) -> Option<&mut client::Client> {
        match self {
            #[cfg(target_os = "windows")]
            Self::Wts(dvc) => dvc.client_mut(),
            Self::Freerdp(dvc) => dvc.client_mut(),
        }
    }

    fn terminate(&mut self) -> Result<(), vc::Error> {
        match self {
            #[cfg(target_os = "windows")]
            Self::Wts(dvc) => dvc.terminate(),
            Self::Freerdp(dvc) => dvc.terminate(),
        }
    }
}

pub enum Handle {
    #[cfg(target_os = "windows")]
    Wts(wts::Handle),
    Freerdp(freerdp::Handle),
}

impl vc::Handle for Handle {
    fn write(&self, data: Vec<u8>) -> Result<(), vc::Error> {
        match self {
            #[cfg(target_os = "windows")]
            Self::Wts(dvc) => dvc.write(data),
            Self::Freerdp(dvc) => dvc.write(data),
        }
    }

    fn close(&mut self) -> Result<(), vc::Error> {
        match self {
            #[cfg(target_os = "windows")]
            Self::Wts(dvc) => dvc.close(),
            Self::Freerdp(dvc) => dvc.close(),
        }
    }
}

enum PduChannel {
    Short(u8),
    Medium(u16),
    Large(u32),
}

impl fmt::Display for PduChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Short(chan) => {
                write!(f, "0x{chan:x}")
            }
            Self::Medium(chan) => {
                write!(f, "0x{chan:x}")
            }
            Self::Large(chan) => {
                write!(f, "0x{chan:x}")
            }
        }
    }
}

enum Pdu {
    Data { channel_id: PduChannel },
}

impl Pdu {
    fn parse(data: &[u8]) -> Result<(Self, usize), String> {
        let header = data[0];

        let cbid = header & 0x03;
        //let sp = (header & 0x0C) >> 2;
        let cmd = (header & 0xF0) >> 4;

        let mut read = 1;

        let pdu = match cmd {
            0x03 => {
                let channel_id = match cbid {
                    0 => {
                        let res = PduChannel::Short(data[read]);
                        read += 1;
                        res
                    }
                    1 => {
                        let channel = [data[read], data[read + 1]];
                        let res = PduChannel::Medium(u16::from_le_bytes(channel));
                        read += 2;
                        res
                    }
                    2 => {
                        let channel = [data[read], data[read + 1], data[read + 2], data[read + 3]];
                        let res = PduChannel::Large(u32::from_le_bytes(channel));
                        read += 4;
                        res
                    }
                    _ => {
                        return Err(format!("PDU Data cbid is invalid: 0x{cbid:x}"));
                    }
                };
                Self::Data { channel_id }
            }
            _ => return Err(format!("unknown PDU command 0x{cmd:x} / 0x{header:x}")),
        };

        Ok((pdu, read))
    }
}

impl fmt::Display for Pdu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Data { channel_id } => {
                write!(f, "Data channel_id = {channel_id}")
            }
        }
    }
}
