use std::ffi;
#[cfg(feature = "log")]
use std::{env, fs};

pub mod api;
pub mod log;
pub mod service;

pub mod clipboard;
pub mod command;
pub mod ftp;
pub mod socks5;
pub mod stage0;

pub const VIRTUAL_CHANNEL_NAME: &ffi::CStr = c"SOXY";

#[cfg(not(feature = "log"))]
pub const fn init_logs(_with_file: bool) {}

#[cfg(feature = "log")]
pub fn init_logs(with_file: bool) {
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

    let mut loggers: Vec<Box<dyn simplelog::SharedLogger>> = vec![simplelog::TermLogger::new(
        level_filter,
        config.clone(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )];

    if with_file {
        let mut path = env::temp_dir();
        path.push("soxy.log");

        if let Ok(file) = fs::File::options()
            .create(true)
            .append(false)
            .truncate(true)
            .write(true)
            .open(path)
        {
            loggers.push(simplelog::WriteLogger::new(level_filter, config, file));
        }
    }

    let _ = simplelog::CombinedLogger::init(loggers);
}
