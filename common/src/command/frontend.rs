use crate::{api, service};
use std::{net, thread};

pub(crate) fn tcp_frontend_handler(
    _server: &service::TcpFrontendServer,
    _scope: &thread::Scope,
    client: net::TcpStream,
    channel: &service::Channel,
) -> Result<(), api::Error> {
    let client_rdp = channel.connect(&super::SERVICE)?;
    Ok(service::double_stream_copy(
        service::Kind::Frontend,
        &super::SERVICE,
        client_rdp,
        client,
    )?)
}
