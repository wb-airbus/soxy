use super::x11;
use common::input;
use std::{fmt, mem, thread, time};

mod headers;

pub(crate) enum Error {
    NotFreerdp(String),
    Loading(libloading::Error),
    MissingFunction(&'static str),
    X11(x11::Error),
    SendExFailed(headers::DWORD),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::NotFreerdp(msg) => write!(f, "client is not FreeRDP: {msg}"),
            Self::Loading(e) => write!(f, "loading error: {e}"),
            Self::MissingFunction(fun) => write!(f, "missing function {fun:?}"),
            Self::X11(e) => write!(f, "X11 error: {e}"),
            Self::SendExFailed(sc) => write!(f, "failed to send 0x{sc:x}"),
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

const KEYBOARD_DELAY_DEFAULT_MS: u64 = 12;

pub(crate) struct Client {
    x11: x11::Client,
    rdp_input: headers::LPVOID,
    keyboard_init: headers::freerdp_keyboard_init,
    keyboard_get_rdp_scancode_from_x11_keycode:
        headers::freerdp_keyboard_get_rdp_scancode_from_x11_keycode,
    send_keyboard_event_ex: headers::freerdp_input_send_keyboard_event_ex,
    keyboard_delay: time::Duration,
}

impl Client {
    fn load(size: headers::DWORD, entrypoints: headers::LPVOID) -> Result<Self, Error> {
        let size_of_freerdp_struct = mem::size_of::<headers::CHANNEL_ENTRY_POINTS_FREERDP>();
        if size as usize != size_of_freerdp_struct {
            return Err(Error::NotFreerdp(format!(
                "size of the struct is not 0x{size_of_freerdp_struct:x}"
            )))?;
        }

        let pep: headers::PCHANNEL_ENTRY_POINTS_FREERDP = entrypoints.cast();
        let ep = unsafe { *pep };

        if ep.MagicNumber != headers::FREERDP_CHANNEL_MAGIC_NUMBER {
            return Err(Error::NotFreerdp(format!(
                "bad magic number 0x{:x}",
                ep.MagicNumber
            )))?;
        }

        let x11 = x11::Client::load()?;

        let rdp_input = unsafe { (*ep.rdpContext).input };

        let lfreerdp =
            unsafe { libloading::Library::new(libloading::library_filename("freerdp3"))? };

        let keyboard_init = unsafe {
            lfreerdp
                .get::<headers::freerdp_keyboard_init>("freerdp_keyboard_init".as_bytes())?
                .into_raw()
                .as_raw_ptr()
        };
        let keyboard_init = unsafe {
            mem::transmute::<headers::LPVOID, headers::freerdp_keyboard_init>(keyboard_init)
        };

        let keyboard_get_rdp_scancode_from_x11_keycode = unsafe {
            lfreerdp
                .get::<headers::freerdp_keyboard_get_rdp_scancode_from_x11_keycode>(
                    "freerdp_keyboard_get_rdp_scancode_from_x11_keycode".as_bytes(),
                )?
                .into_raw()
                .as_raw_ptr()
        };
        let keyboard_get_rdp_scancode_from_x11_keycode = unsafe {
            mem::transmute::<
                headers::LPVOID,
                headers::freerdp_keyboard_get_rdp_scancode_from_x11_keycode,
            >(keyboard_get_rdp_scancode_from_x11_keycode)
        };

        let send_keyboard_event_ex = unsafe {
            lfreerdp
                .get::<headers::freerdp_input_send_keyboard_event_ex>(
                    "freerdp_input_send_keyboard_event_ex".as_bytes(),
                )?
                .into_raw()
                .as_raw_ptr()
        };
        let send_keyboard_event_ex = unsafe {
            mem::transmute::<headers::LPVOID, headers::freerdp_input_send_keyboard_event_ex>(
                send_keyboard_event_ex,
            )
        };

        let keyboard_delay = time::Duration::from_millis(KEYBOARD_DELAY_DEFAULT_MS);

        Ok(Self {
            x11,
            rdp_input,
            keyboard_init,
            keyboard_get_rdp_scancode_from_x11_keycode,
            send_keyboard_event_ex,
            keyboard_delay,
        })
    }

    fn send_keycode(&self, press: bool, keycode: u8) -> Result<(), Error> {
        let kgrsfxk = self
            .keyboard_get_rdp_scancode_from_x11_keycode
            .as_ref()
            .ok_or(Error::MissingFunction(
                "keyboard_get_rdp_scancode_from_x11_keycode",
            ))?;

        let sc = unsafe { kgrsfxk(u32::from(keycode)) };

        let send_ex = self
            .send_keyboard_event_ex
            .as_ref()
            .ok_or(Error::MissingFunction("send_keyboard_event_ex"))?;

        let ret = unsafe { send_ex(self.rdp_input, i32::from(press), 0, sc) };
        if ret == headers::FALSE {
            return Err(Error::SendExFailed(sc));
        }

        Ok(())
    }

    fn send_key(&self, key: input::Key, down: bool, release: bool) -> Result<(), Error> {
        let km = self.x11.lookup_keycode_and_modifiers(key)?;

        for press in [true, false] {
            if km.contains_shift() {
                //common::debug!("  SHIFT");
                let mkey = input::Key::Shift;
                let mkm = self.x11.lookup_keycode_and_modifiers(mkey)?;
                self.send_keycode(press, mkm.keycode)?;
            }

            if km.contains_control() {
                //common::debug!("  CONTROL");
                let mkey = input::Key::Control;
                let mkm = self.x11.lookup_keycode_and_modifiers(mkey)?;
                self.send_keycode(press, mkm.keycode)?;
            }

            if km.contains_mod1() {
                //common::debug!("  MOD1 => Level3Shift");
                let mkey = input::Key::Level3Shift;
                let mkm = self.x11.lookup_keycode_and_modifiers(mkey)?;
                self.send_keycode(press, mkm.keycode)?;
            }

            if km.contains_mod2() {
                common::debug!("  MOD2 => ???");
            }

            if km.contains_mod3() {
                common::debug!("  MOD3 => ???");
            }

            if km.contains_mod4() {
                common::debug!("  MOD4 => ???");
            }

            if km.contains_mod5() {
                //common::debug!("  MOD5 => Level3Shift");
                let mkey = input::Key::Level3Shift;
                let mkm = self.x11.lookup_keycode_and_modifiers(mkey)?;
                self.send_keycode(press, mkm.keycode)?;
            }

            if (press && down) || (!press && release) {
                self.send_keycode(press, km.keycode)?;
            }
        }

        thread::sleep(self.keyboard_delay);

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
        if let Some(keyboard_init) = self.keyboard_init.as_ref() {
            let _layout = unsafe { keyboard_init(0) };
        }
        self.keyboard_delay = time::Duration::from_millis(KEYBOARD_DELAY_DEFAULT_MS);
        self.x11.reset();
    }
}

impl super::ClientImplementation for Client {
    fn load_from_entrypoints(
        size: headers::DWORD,
        entrypoints: headers::LPVOID,
    ) -> Result<Self, super::Error> {
        Self::load(size, entrypoints).map_err(super::Error::Freerdp)
    }
}
