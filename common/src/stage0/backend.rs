use crate::{api, service};
use std::io::{self, Read};

pub struct Server {}

impl service::Backend for Server {
    fn accept(mut stream: service::RdpStream<'_>) -> Result<(), io::Error> {
        crate::debug!("starting");

        crate::warn!("unexpected {} connection", api::Service::Stage0);

        let mut buf = [0; api::CHUNK_LENGTH];
        let mut total = 0;

        loop {
            let read = stream.read(&mut buf)?;

            if read == 0 {
                break;
            }

            crate::trace!("{read} bytes read");

            total += read;
        }

        crate::debug!("total read {total} bytes");

        stream.disconnect()
    }
}
