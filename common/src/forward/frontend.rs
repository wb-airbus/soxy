use super::protocol;
use crate::{api, channel, frontend, service};
use std::{io, net, thread};

const SERVICE_KIND: service::Kind = service::Kind::Frontend;

pub fn tcp_handler<'a>(
    server: &frontend::FrontendTcpServer,
    _scope: &'a thread::Scope<'a, '_>,
    stream: net::TcpStream,
    channel: &'a channel::Channel,
) -> Result<(), api::Error> {
    let mut rdp = channel.connect(&super::SERVICE)?;

    let dest = server.custom_data().ok_or(api::Error::Io(io::Error::new(
        io::ErrorKind::InvalidData,
        "missing destination",
    )))?;

    protocol::Command::Connect(dest.clone()).send(&mut rdp)?;

    match protocol::Response::receive(&mut rdp)? {
        protocol::Response::Error(msg) => {
            crate::warn!("port forwarding error: {msg}");
            let _ = stream.shutdown(net::Shutdown::Both);
        }
        protocol::Response::Connected => {
            service::double_stream_copy(SERVICE_KIND, &super::SERVICE, rdp, stream, true)?;
        }
    }

    Ok(())
}
