use common::{channel, service};
use soxy as frontend;

const CHANNEL_SIZE: usize = 1;

#[allow(clippy::too_many_lines)]
fn main() {
    let (frontend_to_backend_send, frontend_to_backend_receive) =
        crossbeam_channel::bounded(CHANNEL_SIZE);
    let (backend_to_frontend_send, backend_to_frontend_receive) =
        crossbeam_channel::bounded(CHANNEL_SIZE);

    let backend_channel = channel::Channel::new(backend_to_frontend_send);
    let frontend_channel = channel::Channel::new(frontend_to_backend_send);

    if let Err(e) = frontend::start(frontend_channel, backend_to_frontend_receive) {
        common::error!("{e}");
        return;
    }

    common::log_package_infos!();

    if let Err(e) = backend_channel.run(service::Kind::Backend, &frontend_to_backend_receive) {
        common::error!("backend channel stopped: {e}");
    } else {
        common::debug!("backend channel stopped");
    }
}
