use crate::{api, channel, frontend, service};
use std::{net, thread};

pub fn tcp_frontend_handler(
    _server: &frontend::FrontendTcpServer,
    _scope: &thread::Scope,
    client: net::TcpStream,
    channel: &channel::Channel,
) -> Result<(), api::Error> {
    let client_rdp = channel.connect(&super::SERVICE)?;
    Ok(service::double_stream_copy(
        service::Kind::Frontend,
        &super::SERVICE,
        client_rdp,
        client,
        true,
    )?)
}
