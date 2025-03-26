use super::x11;
use common::input;
use std::{ffi, fmt, mem, ptr, thread, time};

mod headers;

pub(crate) enum Error {
    Loading(libloading::Error),
    MissingFunction(&'static str),
    X11(x11::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::Loading(e) => write!(f, "loading error: {e}"),
            Self::MissingFunction(fun) => write!(f, "missing function {fun:?}"),
            Self::X11(e) => write!(f, "X11 error: {e}"),
        }
    }
}

impl From<libloading::Error> for Error {
    fn from(e: libloading::Error) -> Self {
        Self::Loading(e)
    }
}

impl From<x11::Error> for Error {
    fn from(e: x11::Error) -> Self {
        Self::X11(e)
    }
}

const KEYBOARD_DELAY_DEFAULT_MS: u64 = 20;

pub(crate) struct Client {
    x11: x11::Client,
    window: *mut ffi::c_void,
    modifiers_state: x11::KeycodeAndModifier,
    ncsx_key_down: headers::NCSXKeyDown,
    ncsx_key_up: headers::NCSXKeyDown,
    keyboard_delay: time::Duration,
}

impl Client {
    fn load() -> Result<Self, Error> {
        let x11 = x11::Client::load()?;

        #[cfg(not(target_os = "windows"))]
        let (ncsx_key_down, ncsx_key_up, get_ica_window) = {
            let lwfica = libloading::os::unix::Library::this();

            let ncsx_key_down = unsafe {
                lwfica
                    .get::<headers::NCSXKeyDown>("NCSXKeyDown".as_bytes())?
                    .into_raw()
            };

            let ncsx_key_up = unsafe {
                lwfica
                    .get::<headers::NCSXKeyUp>("NCSXKeyUp".as_bytes())?
                    .into_raw()
            };

            let get_ica_window = unsafe {
                lwfica
                    .get::<headers::GetICADisplayWindow>("GetICADisplayWindow".as_bytes())?
                    .into_raw()
            };

            (ncsx_key_down, ncsx_key_up, get_ica_window)
        };

        #[cfg(target_os = "windows")]
        let (ncsx_key_down, ncsx_key_up, get_ica_window) = {
            let lwfica = libloading::os::windows::Library::this()?;

            let ncsx_key_down = unsafe {
                lwfica
                    .get::<headers::NCSXKeyDown>("NCSXKeyDown".as_bytes())?
                    .as_raw_ptr()
            };

            let ncsx_key_up = unsafe {
                lwfica
                    .get::<headers::NCSXKeyUp>("NCSXKeyUp".as_bytes())?
                    .as_raw_ptr()
            };

            let get_ica_window = unsafe {
                lwfica
                    .get::<headers::GetICADisplayWindow>("GetICADisplayWindow".as_bytes())?
                    .as_raw_ptr()
            };

            (ncsx_key_down, ncsx_key_up, get_ica_window)
        };

        let ncsx_key_down =
            unsafe { mem::transmute::<headers::LPVOID, headers::NCSXKeyDown>(ncsx_key_down) };

        let ncsx_key_up =
            unsafe { mem::transmute::<headers::LPVOID, headers::NCSXKeyUp>(ncsx_key_up) };

        let get_ica_window = unsafe {
            mem::transmute::<headers::LPVOID, headers::GetICADisplayWindow>(get_ica_window)
        };

        let get_ica_window =
            get_ica_window.ok_or_else(|| Error::MissingFunction("get_ica_window"))?;

        let window = unsafe { get_ica_window() };

        let modifiers_state = x11::KeycodeAndModifier::default();

        let keyboard_delay = time::Duration::from_millis(KEYBOARD_DELAY_DEFAULT_MS);

        Ok(Self {
            x11,
            window,
            modifiers_state,
            ncsx_key_down,
            ncsx_key_up,
            keyboard_delay,
        })
    }

    fn send_key(&self, key: input::Key, down: bool, release: bool) -> Result<(), Error> {
        let mut km = self.x11.lookup_keycode_and_modifiers(key)?;

        km.set_modifiers(self.modifiers_state);

        if down {
            let ncsx_key_down = self
                .ncsx_key_down
                .as_ref()
                .ok_or(Error::MissingFunction("ncsx_key_down"))?;

            let event = self.x11.key_press_event(self.window, km);

            let mut event =
                unsafe { mem::transmute::<x11::headers::XKeyEvent, headers::XKeyEvent>(event) };
            unsafe { ncsx_key_down(ptr::from_mut(&mut event)) };

            thread::sleep(self.keyboard_delay);
        }

        if release {
            let ncsx_key_up = self
                .ncsx_key_up
                .as_ref()
                .ok_or(Error::MissingFunction("ncsx_key_up"))?;

            let event = self.x11.key_release_event(self.window, km);
            let mut event =
                unsafe { mem::transmute::<x11::headers::XKeyEvent, headers::XKeyEvent>(event) };
            unsafe { ncsx_key_up(ptr::from_mut(&mut event)) };

            thread::sleep(self.keyboard_delay);
        }

        Ok(())
    }

    fn send_text(&self, s: &str) -> Result<(), Error> {
        for c in s.chars() {
            if c == '\n' {
                self.send_key(input::Key::Return, true, true)?;
            } else {
                self.send_key(input::Key::Printable(c), true, true)?;
            }

            // TODO: is there a better way to detect such a key?
            if c == '~' {
                self.send_key(input::Key::Printable(' '), true, true)?;
            }
        }

        Ok(())
    }
}

impl input::InputHandler for Client {
    fn set(&mut self, setting: input::InputSetting) -> Result<(), input::Error> {
        match setting {
            input::InputSetting::Keyboard(setting) => match setting {
                input::KeyboardSetting::Delay(delay) => {
                    common::debug!("set keyboard delay to {delay:?}");
                    self.keyboard_delay = delay;
                    Ok(())
                }
            },
        }
    }

    fn play(&self, action: input::InputAction) -> Result<(), input::Error> {
        match action {
            input::InputAction::Pause(delay) => {
                thread::sleep(delay);
                Ok(())
            }
            input::InputAction::Keyboard(action) => {
                let res = match action {
                    input::KeyboardAction::KeyDown(k) => self.send_key(k, true, false),
                    input::KeyboardAction::KeyPress(k) => self.send_key(k, true, true),
                    input::KeyboardAction::KeyUp(k) => self.send_key(k, false, true),
                    input::KeyboardAction::Write(t) => self.send_text(&t),
                };
                res.map_err(|e| input::Error::Keyboard(e.to_string()))
            }
        }
    }

    fn reset(&mut self) {
        self.modifiers_state = x11::KeycodeAndModifier::default();
        self.keyboard_delay = time::Duration::from_millis(KEYBOARD_DELAY_DEFAULT_MS);
        self.x11.reset();
    }
}

impl super::ClientImplementation for Client {
    fn load_from_entrypoints(
        _size: headers::DWORD,
        _entrypoints: headers::LPVOID,
    ) -> Result<Self, super::Error> {
        Self::load().map_err(super::Error::Citrix)
    }
}
