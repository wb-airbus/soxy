use super::protocol;
use crate::{rdp, service};
use std::{io, net};

const SERVICE_KIND: service::Kind = service::Kind::Backend;

pub fn handler(mut stream: rdp::RdpStream<'_>) -> Result<(), io::Error> {
    crate::debug!("starting");

    let command = protocol::Command::receive(&mut stream)?;

    match command {
        protocol::Command::Connect(dest) => {
            crate::info!("connecting to {dest:#?}");

            match net::TcpStream::connect(&dest) {
                Err(e) => {
                    crate::warn!("failed to connect to {dest:#?}: {e}");
                    protocol::Response::Error(e.to_string()).send(&mut stream)
                }
                Ok(server) => {
                    crate::debug!("connected to {dest:#?}");

                    protocol::Response::Connected.send(&mut stream)?;

                    crate::debug!("starting stream copy");

                    service::double_stream_copy(SERVICE_KIND, &super::SERVICE, stream, server, true)
                }
            }
        }
    }
}
