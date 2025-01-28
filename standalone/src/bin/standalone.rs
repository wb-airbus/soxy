use common::{api, service};

const CHANNEL_SIZE: usize = 256;

#[allow(clippy::too_many_lines)]
fn main() {
    let (frontend_to_backend_send, frontend_to_backend_receive) =
        crossbeam_channel::bounded(CHANNEL_SIZE);
    let (backend_to_frontend_send, backend_to_frontend_receive) =
        crossbeam_channel::bounded(CHANNEL_SIZE);

    let backend_channel = service::Channel::new(backend_to_frontend_send);
    let frontend_channel = service::Channel::new(frontend_to_backend_send);

    if let Err(e) = soxy::init(frontend_channel, backend_to_frontend_receive) {
        common::error!("error: {e}");
        return;
    }

    if let Err(e) = backend_channel.start(api::ServiceKind::Backend, &frontend_to_backend_receive) {
        common::error!("error: {e}");
    } else {
        common::debug!("terminated");
    }
}
