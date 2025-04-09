use super::protocol;
use crate::service;
use copyrs::Clipboard;
use std::{borrow, io};

pub(crate) fn handler(mut stream: service::RdpStream<'_>) -> Result<(), io::Error> {
    crate::debug!("starting");

    loop {
        let cmd = protocol::Command::receive(&mut stream)?;

        match cmd {
            protocol::Command::Read => {
                crate::debug!("read");

                match copyrs::clipboard() {
                    Err(e) => {
                        crate::error!("failed to get clipboard: {e}");
                        protocol::Response::Failed.send(&mut stream)?;
                    }
                    Ok(clipboard) => match clipboard.get_content() {
                        Err(e) => {
                            crate::error!("failed to get clipboard content: {e}");
                            protocol::Response::Failed.send(&mut stream)?;
                        }
                        Ok(content) => match content.kind {
                            copyrs::ClipboardContentKind::Image => {
                                crate::error!("clipboard contrent is an image, not text");
                                protocol::Response::Failed.send(&mut stream)?;
                            }
                            copyrs::ClipboardContentKind::Text => {
                                protocol::Response::Text(content.data).send(&mut stream)?;
                            }
                        },
                    },
                }
            }

            protocol::Command::WriteText(value) => {
                crate::debug!("write_text {value:?}");

                match copyrs::clipboard() {
                    Err(e) => {
                        crate::error!("failed to get clipboard: {e}");
                        protocol::Response::Failed.send(&mut stream)?;
                    }
                    Ok(mut clipboard) => {
                        let value = borrow::Cow::Borrowed(value.as_slice());

                        match clipboard.set_content(value, copyrs::ClipboardContentKind::Text) {
                            Err(e) => {
                                crate::error!("failed to set clipboard: {e}");
                                protocol::Response::Failed.send(&mut stream)?;
                            }
                            Ok(()) => {
                                protocol::Response::WriteDone.send(&mut stream)?;
                            }
                        }
                    }
                }
            }
        }
    }
}
