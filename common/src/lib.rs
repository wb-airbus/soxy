pub mod api;
pub mod log;
pub mod service;

pub mod clipboard;
pub mod command;
pub mod ftp;
pub mod socks5;

pub const VIRTUAL_CHANNEL_NAME: &str = "SOXY";

#[cfg(not(feature = "log"))]
pub const fn init_logs() -> Result<(), String> {
    Ok(())
}

#[cfg(feature = "log")]
pub fn init_logs() -> Result<(), ::log::SetLoggerError> {
    #[cfg(debug_assertions)]
    let level_filter = simplelog::LevelFilter::Debug;

    #[cfg(not(debug_assertions))]
    let level_filter = simplelog::LevelFilter::Info;

    let config = simplelog::ConfigBuilder::new()
        .set_level_padding(simplelog::LevelPadding::Right)
        .set_target_level(simplelog::LevelFilter::Off)
        .set_thread_level(simplelog::LevelFilter::Error)
        .set_thread_mode(simplelog::ThreadLogMode::Names)
        .set_time_format_rfc2822()
        .build();

    simplelog::TermLogger::init(
        level_filter,
        config,
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )
}
