use std::io;

const PLUGIN_NAME: &str = "soxy";

const WTS_HKCU_ADDINS_PATH: &str = "Software\\Microsoft\\Terminal Server Client\\Default\\AddIns";

fn guid_to_clsid(guid: u128) -> String {
    let bytes = guid.to_be_bytes();
    format!(
        "{{{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

fn wts_register(dll_path: &str) -> Result<(), String> {
    println!("using entry name = {PLUGIN_NAME}");

    let hkcuser = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);

    let (addins, _disp) = hkcuser
        .create_subkey(WTS_HKCU_ADDINS_PATH)
        .map_err(|e| format!("failed to create HKCU addins: {e}"))?;

    let (entry, _disp) = addins
        .create_subkey(PLUGIN_NAME)
        .map_err(|e| format!("failed to create entry: {e}"))?;

    let clsid = guid_to_clsid(soxyreg::PLUGIN_GUID);

    println!("using CLSID = {clsid}");

    entry
        .set_value("Name", &clsid)
        .map_err(|e| format!("failed to set name: {e}"))?;

    let hkcr = winreg::RegKey::predef(winreg::enums::HKEY_CLASSES_ROOT);

    let (inproc, _disp) = hkcr
        .create_subkey(format!("CLSID\\{clsid}\\InprocServer32"))
        .map_err(|e| format!("failed to create InprocServer32: {e}"))?;

    inproc
        .set_value("", &dll_path)
        .map_err(|e| format!("failed to InprocServer32 (Default): {e}"))?;

    inproc
        .set_value("ThreadingModel", &"Free")
        .map_err(|e| format!("failed to InprocServer32 ThreadingModel: {e}"))?;

    Ok(())
}

fn wts_unregister() -> Result<(), String> {
    let mut res = Ok(());

    println!("using entry name = {PLUGIN_NAME}");

    let hkcuser = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);

    if let Ok(addins) =
        hkcuser.open_subkey_with_flags(WTS_HKCU_ADDINS_PATH, winreg::enums::KEY_ALL_ACCESS)
        && let Err(e) = addins.delete_subkey_all(PLUGIN_NAME)
        && e.kind() != io::ErrorKind::NotFound
    {
        res = Err(format!("failed to delete HKCU {PLUGIN_NAME}: {e}"));
    }

    let clsid = guid_to_clsid(soxyreg::PLUGIN_GUID);

    println!("using CLSID = {clsid}");

    let hkcr = winreg::RegKey::predef(winreg::enums::HKEY_CLASSES_ROOT);

    if let Ok(entry) = hkcr.open_subkey_with_flags("CLSID", winreg::enums::KEY_ALL_ACCESS)
        && let Err(e) = entry.delete_subkey_all(clsid)
        && e.kind() != io::ErrorKind::NotFound
    {
        res = Err(format!("failed to delete HKCR CLSID: {e}"));
    }

    res
}

pub fn register(dll_path: &str) {
    if let Err(e) = wts_register(dll_path) {
        eprintln!("WTS register error: {e}");
    } else {
        println!("WTS registered");
    }
}

pub fn unregister() {
    if let Err(e) = wts_unregister() {
        eprintln!("WTS unregister error: {e}");
    } else {
        println!("WTS unregistered");
    }
}
