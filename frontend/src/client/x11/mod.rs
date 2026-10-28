use common::input;
use std::{
    collections, ffi, fmt, mem,
    ops::{BitAnd, BitOrAssign},
    ptr, slice, sync,
};

pub mod headers;

pub enum Error {
    Loading(libloading::Error),
    MissingFunction(&'static str),
    OpenDisplayFailed,
    GetMapFailed,
    MapIsNull,
    KeySymMapIsNull,
    TypesIsNull,
    SymsIsNull,
    KeycodeNotFoundForKey(input::Key),
    InvalidCharacter(char),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::Loading(e) => write!(f, "loading error: {e}"),
            Self::MissingFunction(fun) => write!(f, "missing function {fun:?}"),
            Self::OpenDisplayFailed => write!(f, "failed to open display"),
            Self::GetMapFailed => write!(f, "failed to get keymap"),
            Self::MapIsNull => write!(f, "map is null"),
            Self::KeySymMapIsNull => write!(f, "key_sym_map is null"),
            Self::TypesIsNull => write!(f, "types is null"),
            Self::SymsIsNull => write!(f, "syms is null"),
            Self::KeycodeNotFoundForKey(k) => {
                write!(f, "keycode for key {k} not found")
            }
            Self::InvalidCharacter(c) => write!(f, "invalid character {c:#?}"),
        }
    }
}

impl From<libloading::Error> for Error {
    fn from(e: libloading::Error) -> Self {
        Self::Loading(e)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct KeycodeAndModifier {
    pub keycode: u8,
    pub modifiers: u8,
}

impl KeycodeAndModifier {
    pub fn contains_shift(self) -> bool {
        self.modifiers.bitand(headers::ShiftMask) != 0
    }

    pub fn contains_control(self) -> bool {
        self.modifiers.bitand(headers::ControlMask) != 0
    }

    pub fn contains_mod1(self) -> bool {
        self.modifiers.bitand(headers::Mod1Mask) != 0
    }

    pub fn contains_mod2(self) -> bool {
        self.modifiers.bitand(headers::Mod2Mask) != 0
    }

    pub fn contains_mod3(self) -> bool {
        self.modifiers.bitand(headers::Mod3Mask) != 0
    }

    pub fn contains_mod4(self) -> bool {
        self.modifiers.bitand(headers::Mod4Mask) != 0
    }

    pub fn contains_mod5(self) -> bool {
        self.modifiers.bitand(headers::Mod5Mask) != 0
    }

    pub fn set_modifiers(&mut self, other: Self) {
        self.modifiers.bitor_assign(other.modifiers);
    }
}

pub struct Client {
    display: *mut ffi::c_void,
    xkb_get_map: headers::XkbGetMap,
    mapping_cache: sync::RwLock<collections::HashMap<input::Key, KeycodeAndModifier>>,
}

impl Client {
    pub fn load() -> Result<Self, Error> {
        let lx11 = unsafe { libloading::Library::new("libX11.so.6")? };

        let x_open_display = unsafe {
            lx11.get::<headers::XOpenDisplay>(b"XOpenDisplay")?
                .into_raw()
                .as_raw_ptr()
        };
        let x_open_display: headers::XOpenDisplay =
            unsafe { mem::transmute::<*mut ffi::c_void, headers::XOpenDisplay>(x_open_display) };

        let x_open_display = x_open_display
            .as_ref()
            .ok_or(Error::MissingFunction("XOpenDisplay"))?;

        let display = unsafe { x_open_display(ptr::null_mut()) };

        if display.is_null() {
            return Err(Error::OpenDisplayFailed);
        }

        let xkb_get_map = unsafe {
            lx11.get::<headers::XkbGetMap>(b"XkbGetMap")?
                .into_raw()
                .as_raw_ptr()
        };
        let xkb_get_map =
            unsafe { mem::transmute::<*mut ffi::c_void, headers::XkbGetMap>(xkb_get_map) };

        Ok(Self {
            display,
            xkb_get_map,
            mapping_cache: sync::RwLock::new(collections::HashMap::new()),
        })
    }

    pub fn reset(&self) {
        self.mapping_cache.write().unwrap().clear();
    }

    pub fn lookup_keycode_and_modifiers(
        &self,
        key: input::Key,
    ) -> Result<KeycodeAndModifier, Error> {
        let mut cache = self.mapping_cache.write().unwrap();

        match cache.entry(key) {
            collections::hash_map::Entry::Occupied(e) => Ok(*e.get()),
            collections::hash_map::Entry::Vacant(e) => {
                let xkb_get_map = self
                    .xkb_get_map
                    .as_ref()
                    .ok_or(Error::MissingFunction("XkbGetMap"))?;

                let keymap = unsafe {
                    xkb_get_map(
                        self.display,
                        headers::XkbAllMapComponentsMask,
                        headers::XkbUseCoreKbd,
                    )
                };
                let keymap = unsafe { keymap.as_ref().ok_or(Error::GetMapFailed)? };

                let map = unsafe { keymap.map.as_ref().ok_or(Error::MapIsNull)? };
                let key_sym_maps =
                    unsafe { map.key_sym_map.as_ref().ok_or(Error::KeySymMapIsNull)? };
                let key_sym_maps = unsafe { slice::from_raw_parts(key_sym_maps, 256) };
                let types = unsafe { map.types.as_ref().ok_or(Error::TypesIsNull)? };
                let types = unsafe { slice::from_raw_parts(types, usize::from(map.size_types)) };

                let km = key_to_keycode_and_modifiers(keymap, map, key_sym_maps, types, key)?;

                e.insert(km);

                Ok(km)
            }
        }
    }

    pub fn key_press_event(
        &self,
        window: *mut ffi::c_void,
        keycode_modifier: KeycodeAndModifier,
    ) -> headers::XKeyPressedEvent {
        headers::XKeyPressedEvent {
            type_: headers::KeyPress,
            display: self.display,
            window,
            root: window,
            time: headers::CurrentTime,
            state: u32::from(keycode_modifier.modifiers),
            keycode: u32::from(keycode_modifier.keycode),
            same_screen: headers::TRUE,
            ..Default::default()
        }
    }

    pub fn key_release_event(
        &self,
        window: *mut ffi::c_void,
        keycode_modifier: KeycodeAndModifier,
    ) -> headers::XKeyReleasedEvent {
        headers::XKeyReleasedEvent {
            type_: headers::KeyRelease,
            display: self.display,
            window,
            root: window,
            time: headers::CurrentTime,
            state: u32::from(keycode_modifier.modifiers),
            keycode: u32::from(keycode_modifier.keycode),
            same_screen: headers::TRUE,
            ..Default::default()
        }
    }
}

fn key_to_keycode_and_modifiers(
    keymap: &headers::XkbDescRec,
    map: &headers::XkbClientMapRec,
    key_sym_maps: &[headers::XkbSymMapRec],
    types: &[headers::XkbKeyTypeRec],
    key: input::Key,
) -> Result<KeycodeAndModifier, Error> {
    let mut matches = vec![];

    for keycode in keymap.min_key_code..=keymap.max_key_code {
        let syms = unsafe { map.syms.as_ref().ok_or(Error::SymsIsNull)? };
        let key_sym_map = key_sym_maps[usize::from(keycode)];

        let num_syms = key_sym_map.width * (key_sym_map.group_info.bitand(0x0f));
        let syms = unsafe { slice::from_raw_parts(syms, usize::from(map.size_syms)) };
        let syms = &syms[key_sym_map.offset as usize..];

        let groups_width = key_sym_map.width;

        let mut cur_level = 0;
        let mut cur_group = 0u8;
        let mut cur_sym = 0;

        let keysym = key_to_keysym(key)?;

        loop {
            if cur_sym >= num_syms {
                break;
            }

            if syms[cur_sym as usize] == keysym && cur_group == 0 {
                let key_type =
                    types[usize::from(key_sym_map.kt_index[usize::from(cur_group.bitand(0x3))])];

                let mut found = false;

                if let Some(maps) = unsafe { key_type.map.as_ref() } {
                    let maps =
                        unsafe { slice::from_raw_parts(maps, usize::from(key_type.map_count)) };

                    for map in maps {
                        if map.active != 0 && map.level == cur_level {
                            found = true;
                            matches.push(KeycodeAndModifier {
                                keycode,
                                modifiers: map.mods.mask,
                            });
                        }
                    }
                }

                if !found {
                    matches.push(KeycodeAndModifier {
                        keycode,
                        modifiers: 0,
                    });
                }
            }

            cur_level += 1;
            if cur_level >= groups_width {
                cur_level = 0;
                cur_group += 1;
            }

            cur_sym += 1;
        }
    }

    let mut best = matches.pop().ok_or(Error::KeycodeNotFoundForKey(key))?;

    for km in matches {
        if km.modifiers < best.modifiers
            || (km.modifiers == best.modifiers && km.keycode < best.keycode)
        {
            best = km;
        }
    }

    Ok(best)
}

fn key_to_keysym(key: input::Key) -> Result<ffi::c_ulong, Error> {
    match key {
        input::Key::AltLeft => Ok(0xffe9),
        input::Key::AltRight => Ok(0xffea),
        input::Key::Backspace => Ok(0xff08),
        input::Key::Control => Ok(0xffe3),
        input::Key::Delete => Ok(0xffff),
        input::Key::Down => Ok(0xff54),
        input::Key::Escape => Ok(0xff1b),
        input::Key::F1 => Ok(0xffbe),
        input::Key::F2 => Ok(0xffbf),
        input::Key::F3 => Ok(0xffc0),
        input::Key::F4 => Ok(0xffc1),
        input::Key::F5 => Ok(0xffc2),
        input::Key::F6 => Ok(0xffc3),
        input::Key::F7 => Ok(0xffc4),
        input::Key::F8 => Ok(0xffc5),
        input::Key::F9 => Ok(0xffc6),
        input::Key::F10 => Ok(0xffc7),
        input::Key::F11 => Ok(0xffc8),
        input::Key::HyperLeft => Ok(0xffed),
        input::Key::HyperRight => Ok(0xffee),
        input::Key::Left => Ok(0xff51),
        input::Key::Level3Shift => Ok(0xfe03),
        input::Key::Level5Shift => Ok(0xfe11),
        input::Key::MetaLeft => Ok(0xffe7),
        input::Key::MetaRight => Ok(0xffe8),
        input::Key::Return => Ok(0xff0d),
        input::Key::Right => Ok(0xff53),
        input::Key::Shift => Ok(0xffe1),
        input::Key::SuperLeft | input::Key::Windows => Ok(0xffeb),
        input::Key::SuperRight => Ok(0xffec),
        input::Key::Tab => Ok(0xff09),
        input::Key::Up => Ok(0xff52),
        input::Key::Printable(c) => {
            if c < ' ' {
                return Err(Error::InvalidCharacter(c));
            }
            Ok(ffi::c_ulong::from(c as u8))
        }
    }
}
