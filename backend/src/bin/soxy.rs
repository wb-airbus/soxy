use std::env;

fn main() {
    let mut args = env::args();

    let channel_name = args.nth(1);
    let channel_name = channel_name
        .as_ref()
        .map_or(common::VIRTUAL_CHANNEL_DEFAULT_NAME, String::as_str);

    #[cfg(debug_assertions)]
    soxy::main(channel_name, common::Level::Debug);
    #[cfg(not(debug_assertions))]
    soxy::main(channel_name, common::Level::Info);
}
