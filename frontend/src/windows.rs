use std::{env, io};
use windows_sys as ws;

const ENTRY_NAME: &str = common::VIRTUAL_CHANNEL_NAME;

const ADDINS_PATH: &str = "Software\\Microsoft\\Terminal Server Client\\Default\\AddIns";

fn register() -> Result<(), String> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);

    let (addins, _disp) = hkcu
        .create_subkey(ADDINS_PATH)
        .map_err(|e| format!("failed to create addins: {e}"))?;

    let (entry, _disp) = addins
        .create_subkey(ENTRY_NAME)
        .map_err(|e| format!("failed to create entry: {e}"))?;

    entry
        .set_value("View Enabled", &1u32)
        .map_err(|e| format!("failed to set view enabled: {e}"))?;

    let mut dll = env::current_dir().unwrap();
    dll.push(format!("{}.dll", env!("CARGO_CRATE_NAME")));
    entry
        .set_value("Name", &dll.as_os_str())
        .map_err(|e| format!("failed to set name: {e}"))?;

    Ok(())
}

#[no_mangle]
#[allow(non_snake_case, unused_variables)]
unsafe extern "system" fn DllRegisterServer() -> ws::core::HRESULT {
    match register() {
        Ok(()) => ws::Win32::Foundation::S_OK,
        Err(e) => {
            common::error!("{e}");
            ws::Win32::System::Ole::SELFREG_E_CLASS
        }
    }
}

fn unregister() -> Result<(), io::Error> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let addins = hkcu.open_subkey(ADDINS_PATH)?;
    let _ = addins.delete_subkey_all(ENTRY_NAME);
    Ok(())
}

#[no_mangle]
#[allow(non_snake_case, unused_variables)]
unsafe extern "system" fn DllUnregisterServer() -> ws::core::HRESULT {
    match unregister() {
        Ok(()) => ws::Win32::Foundation::S_OK,
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => ws::Win32::Foundation::S_OK,
            _ => {
                common::error!("{e}");
                ws::Win32::System::Ole::SELFREG_E_CLASS
            }
        },
    }
}
