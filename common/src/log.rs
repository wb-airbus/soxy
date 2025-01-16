#[cfg(not(feature = "log"))]
mod inner {
    #[macro_export]
    macro_rules! trace {
        ($($arg:tt)+) => {};
    }

    #[macro_export]
    macro_rules! debug {
        ($($arg:tt)+) => {};
    }

    #[macro_export]
    macro_rules! info {
        ($($arg:tt)+) => {};
    }

    #[macro_export]
    macro_rules! warn {
        ($($arg:tt)+) => {};
    }

    #[macro_export]
    macro_rules! error {
        ($($arg:tt)+) => {};
    }
}

#[cfg(feature = "log")]
mod inner {
    #[macro_export]
    macro_rules! trace {
        ($($arg:tt)+) => { log::trace!($($arg)+) }
    }

    #[macro_export]
    macro_rules! debug {
        ($($arg:tt)+) => { log::debug!($($arg)+) }
    }

    #[macro_export]
    macro_rules! info {
        ($($arg:tt)+) => { log::info!($($arg)+) }
    }

    #[macro_export]
    macro_rules! warn {
        ($($arg:tt)+) => { log::warn!($($arg)+) }
    }

    #[macro_export]
    macro_rules! error {
        ($($arg:tt)+) => { log::error!($($arg)+) }
    }
}
