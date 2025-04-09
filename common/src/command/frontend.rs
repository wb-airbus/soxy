use crate::service;
use std::{io, net, thread};

pub(crate) fn tcp_frontend_handler(
    _server: &service::TcpFrontendServer,
    _scope: &thread::Scope,
    client: net::TcpStream,
    channel: &service::Channel,
) -> Result<(), io::Error> {
    let client_rdp = channel.connect(&super::SERVICE)?;
    service::double_stream_copy(service::Kind::Frontend, &super::SERVICE, client_rdp, client)
}
