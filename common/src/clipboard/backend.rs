use super::protocol;
use crate::service;
use std::io;

pub struct Server {}

impl service::Backend for Server {
    fn accept(mut stream: service::RdpStream<'_>) -> Result<(), io::Error> {
        crate::debug!("starting");

        loop {
            let cmd = protocol::Command::receive(&mut stream)?;

            match cmd {
                protocol::Command::Read => {
                    crate::debug!("read");

                    #[cfg(not(target_os = "windows"))]
                    {
                        protocol::Response::Failed.send(&mut stream)?;
                    }

                    #[cfg(target_os = "windows")]
                    {
                        match clipboard_win::get_clipboard_string() {
                            Err(e) => {
                                crate::error!("failed to get clipboard: {e}");
                                protocol::Response::Failed.send(&mut stream)?;
                            }
                            Ok(s) => protocol::Response::Clipboard(s).send(&mut stream)?,
                        }
                    }
                }

                protocol::Command::Write(value) => {
                    crate::debug!("write {value:?}");

                    #[cfg(not(target_os = "windows"))]
                    {
                        protocol::Response::Failed.send(&mut stream)?;
                    }

                    #[cfg(target_os = "windows")]
                    {
                        match clipboard_win::set_clipboard_string(&value) {
                            Err(e) => {
                                crate::error!("failed to set clipboard: {e}");
                                protocol::Response::Failed.send(&mut stream)?;
                            }
                            Ok(()) => protocol::Response::WriteDone.send(&mut stream)?,
                        }
                    }
                }
            }
        }
    }
}
