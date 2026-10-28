#[cfg(feature = "frontend")]
use crate::frontend as sfrontend;
use crate::service;
use std::{fmt, time};

#[cfg(feature = "frontend")]
mod frontend;

pub static SERVICE: service::Service = service::Service {
    internal: false,
    name: "input",
    #[cfg(feature = "frontend")]
    frontend: Some(sfrontend::Frontend {
        tcp: Some(sfrontend::FrontendTcp {
            default_port: 1081,
            handler: frontend::tcp_handler,
        }),
    }),
    #[cfg(feature = "backend")]
    backend: None,
};

pub enum Error {
    Keyboard(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::Keyboard(msg) => write!(f, "keyboard error: {msg}"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    AltLeft,
    AltRight,
    Backspace,
    Control,
    Delete,
    Down,
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    HyperLeft,
    HyperRight,
    Left,
    Level3Shift,
    Level5Shift,
    MetaLeft,
    MetaRight,
    Return,
    Right,
    Shift,
    SuperLeft,
    SuperRight,
    Tab,
    Up,
    Windows,
    Printable(char),
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::AltLeft => write!(f, "AltLeft"),
            Self::AltRight => write!(f, "AltRight"),
            Self::Backspace => write!(f, "Backspace"),
            Self::Control => write!(f, "Control"),
            Self::Delete => write!(f, "Delete"),
            Self::Down => write!(f, "Down"),
            Self::Escape => write!(f, "Escape"),
            Self::F1 => write!(f, "F1"),
            Self::F2 => write!(f, "F2"),
            Self::F3 => write!(f, "F3"),
            Self::F4 => write!(f, "F4"),
            Self::F5 => write!(f, "F5"),
            Self::F6 => write!(f, "F6"),
            Self::F7 => write!(f, "F7"),
            Self::F8 => write!(f, "F8"),
            Self::F9 => write!(f, "F9"),
            Self::F10 => write!(f, "F10"),
            Self::F11 => write!(f, "F11"),
            Self::HyperLeft => write!(f, "HyperLeft"),
            Self::HyperRight => write!(f, "HyperRight"),
            Self::Left => write!(f, "Left"),
            Self::Level3Shift => write!(f, "Level3Shift"),
            Self::Level5Shift => write!(f, "Level5Shift"),
            Self::MetaLeft => write!(f, "MetaLeft"),
            Self::MetaRight => write!(f, "MetaRight"),
            Self::Return => write!(f, "Return"),
            Self::Right => write!(f, "Right"),
            Self::Shift => write!(f, "Shift"),
            Self::SuperLeft => write!(f, "SuperLeft"),
            Self::SuperRight => write!(f, "SuperRight"),
            Self::Tab => write!(f, "Tab"),
            Self::Up => write!(f, "Up"),
            Self::Windows => write!(f, "Windows"),
            Self::Printable(c) => write!(f, "Printable({c:?})"),
        }
    }
}

pub enum KeyboardAction {
    KeyDown(Key),
    KeyPress(Key),
    KeyUp(Key),
    Write(String),
}

pub enum InputAction {
    Pause(time::Duration),
    Keyboard(KeyboardAction),
}

pub enum KeyboardSetting {
    Delay(time::Duration),
}

pub enum InputSetting {
    Keyboard(KeyboardSetting),
}

pub trait InputHandler {
    fn set(&mut self, setting: InputSetting) -> Result<(), Error>;
    fn play(&self, action: InputAction) -> Result<(), Error>;
    fn reset(&mut self);
}
