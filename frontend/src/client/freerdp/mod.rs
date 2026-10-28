use super::x11;
use common::input;
use std::{fmt, mem, thread, time};

mod headers;

pub enum Error {
    NullPointer(String),
    NotFreerdp(String),
    Loading(libloading::Error),
    MissingFunction(&'static str),
    X11(x11::Error),
    SendExFailed(headers::DWORD),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::NullPointer(msg) => write!(f, "null pointer: {msg}"),
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

const fn looks_like_fn_ptr(p: usize) -> bool {
    // Cheap sanity check to avoid misclassifying unrelated entrypoint tables
    // (e.g. mstsc's static virtual channel tables) as FreeRDP DVC entrypoints.
    //
    // The main goal is to avoid calling through a bogus function pointer and
    // crashing the host process.
    if p < 0x10000 {
        return false;
    }
    if cfg!(target_pointer_width = "64") {
        // Typical user-mode canonical range.
        (p as u64) < 0x0000_8000_0000_0000u64
    } else {
        p < 0x8000_0000
    }
}

const KEYBOARD_DELAY_DEFAULT_MS: u64 = 12;

pub struct Client {
    x11: x11::Client,
    rdp_input: headers::LPVOID,
    keyboard_init: headers::freerdp_keyboard_init,
    keyboard_get_rdp_scancode_from_x11_keycode:
        headers::freerdp_keyboard_get_rdp_scancode_from_x11_keycode,
    send_keyboard_event_ex: headers::freerdp_input_send_keyboard_event_ex,
    keyboard_delay: time::Duration,
}

impl Client {
    #[allow(clippy::too_many_lines)]
    fn load(size: headers::DWORD, entrypoints: headers::LPVOID) -> Result<Self, Error> {
        let rdp_context = {
            if size as usize == mem::size_of::<headers::CHANNEL_ENTRY_POINTS_FREERDP>() {
                let pep: headers::PCHANNEL_ENTRY_POINTS_FREERDP = entrypoints.cast();
                let ep = unsafe { pep.as_ref() }.ok_or_else(|| Error::NullPointer("ep".into()))?;

                if ep.MagicNumber == headers::FREERDP_CHANNEL_MAGIC_NUMBER {
                    Ok(ep.rdpContext)
                } else {
                    Err(Error::NotFreerdp(format!(
                        "bad magic number 0x{:x}",
                        ep.MagicNumber
                    )))
                }
            } else if size as usize == mem::size_of::<headers::IDRDYNVC_ENTRY_POINTS>() {
                // try to detect misdetected linux horizon client
                let first: *mut u32 = entrypoints.cast();
                let second = unsafe { first.add(1) };
                let first =
                    unsafe { first.as_ref() }.ok_or_else(|| Error::NullPointer("ep".into()))?;
                let second =
                    unsafe { second.as_ref() }.ok_or_else(|| Error::NullPointer("ep".into()))?;

                if *first == 0x28 && *second == 0x1 {
                    return Err(Error::NotFreerdp("it looks like an Horizon client".into()));
                }

                let pep: *mut headers::IDRDYNVC_ENTRY_POINTS = entrypoints.cast();
                let ep = unsafe { pep.as_ref() }.ok_or_else(|| Error::NullPointer("ep".into()))?;

                // Defensive: IDRDYNVC_ENTRY_POINTS does not have a magic number. Validate that the
                // first few function pointers look plausible before calling into them.
                //
                // Without this, other clients (notably mstsc.exe) can be misdetected when their
                // entrypoint table size happens to match, leading to an access violation.
                let ptrs: [Option<usize>; 5] = [
                    ep.RegisterPlugin.map(|f| f as usize),
                    ep.GetPlugin.map(|f| f as usize),
                    ep.GetPluginData.map(|f| f as usize),
                    ep.GetRdpSettings.map(|f| f as usize),
                    ep.GetRdpContext.map(|f| f as usize),
                ];
                if ptrs.iter().any(Option::is_none) {
                    return Err(Error::NotFreerdp(
                        "IDRDYNVC entrypoints missing required function pointers".into(),
                    ));
                }
                if ptrs.iter().flatten().any(|p| !looks_like_fn_ptr(*p)) {
                    return Err(Error::NotFreerdp(
                        "IDRDYNVC entrypoints function pointers look invalid".into(),
                    ));
                }

                let get_rdp_context = ep
                    .GetRdpContext
                    .ok_or(Error::NullPointer("GetRdpContext".into()))?;

                Ok(unsafe { get_rdp_context(entrypoints.cast()) })
            } else {
                Err(Error::NotFreerdp("no valid structure found".into()))
            }
        }?;

        let rdp_context =
            unsafe { rdp_context.as_ref() }.ok_or(Error::NullPointer("rdp_context".into()))?;

        let rdp_input = rdp_context.input;

        let x11 = x11::Client::load()?;

        let lfreerdp = unsafe {
            libloading::Library::new(libloading::library_filename("freerdp3")).or(
                libloading::Library::new(libloading::library_filename("freerdp2")),
            )?
        };

        let keyboard_init = unsafe {
            lfreerdp
                .get::<headers::freerdp_keyboard_init>(b"freerdp_keyboard_init")?
                .into_raw()
                .as_raw_ptr()
        };
        let keyboard_init = unsafe {
            mem::transmute::<headers::LPVOID, headers::freerdp_keyboard_init>(keyboard_init)
        };

        let keyboard_get_rdp_scancode_from_x11_keycode = unsafe {
            lfreerdp
                .get::<headers::freerdp_keyboard_get_rdp_scancode_from_x11_keycode>(
                    b"freerdp_keyboard_get_rdp_scancode_from_x11_keycode",
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
                    b"freerdp_input_send_keyboard_event_ex",
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
