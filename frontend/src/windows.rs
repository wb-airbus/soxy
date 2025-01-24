use std::env;
use windows_sys as ws;

const ENTRY_NAME: &str = env!("CARGO_CRATE_NAME");

const CITRIX_MACHINE_MODULES_PATH: &str =
    "SOFTWARE\\WOW6432Node\\Citrix\\ICA Client\\Engine\\Configuration\\Advanced\\Modules";

const CITRIX_MODULES_ICA_PATH: &str = "ICA 3.0";

const CITRIX_ICA_VDEX_PATH: &str = "VirtualDriverEx";

const CITRIX_ENTRY_DRIVER_NAME: &str = "DriverName";
const CITRIX_ENTRY_DRIVER_NAME_WIN16: &str = "DriverNameWin16";
const CITRIX_ENTRY_DRIVER_NAME_WIN32: &str = "DriverNameWin32";

fn citrix_register() -> Result<(), String> {
    let hk = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let path = CITRIX_MACHINE_MODULES_PATH;

    let (modules, _disp) = hk
        .create_subkey(path)
        .map_err(|e| format!("failed to create citrix modules path: {e}"))?;

    let (ica, _disp) = modules
        .create_subkey(CITRIX_MODULES_ICA_PATH)
        .map_err(|e| format!("failed to create citrix modules virtual driver path: {e}"))?;

    let vdex: String = ica.get_value(CITRIX_ICA_VDEX_PATH).unwrap_or(String::new());
    let mut vdex: Vec<&str> = if vdex.trim().is_empty() {
        vec![]
    } else {
        vdex.split(',').map(str::trim).collect()
    };
    vdex.push(ENTRY_NAME);
    let vdex = vdex.join(",");
    ica.set_value(CITRIX_ICA_VDEX_PATH, &vdex)
        .map_err(|e| format!("failed to set name: {e}"))?;

    let (entry, _disp) = modules
        .create_subkey(ENTRY_NAME)
        .map_err(|e| format!("failed to create citrix modules entry path: {e}"))?;
    entry
        .set_value(CITRIX_ENTRY_DRIVER_NAME, &format!("{ENTRY_NAME}.dll"))
        .map_err(|e| format!("failed to set name: {e}"))?;
    entry
        .set_value(CITRIX_ENTRY_DRIVER_NAME_WIN16, &format!("{ENTRY_NAME}.dll"))
        .map_err(|e| format!("failed to set name: {e}"))?;
    entry
        .set_value(CITRIX_ENTRY_DRIVER_NAME_WIN32, &format!("{ENTRY_NAME}.dll"))
        .map_err(|e| format!("failed to set name: {e}"))?;

    Ok(())
}

fn citrix_unregister() {
    let hk = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let path = CITRIX_MACHINE_MODULES_PATH;

    if let Ok(modules) = hk.open_subkey_with_flags(path, winreg::enums::KEY_ALL_ACCESS) {
        if let Ok(ica) =
            modules.open_subkey_with_flags(CITRIX_MODULES_ICA_PATH, winreg::enums::KEY_ALL_ACCESS)
        {
            if let Ok(vdex) = ica.get_value::<String, _>(CITRIX_ICA_VDEX_PATH) {
                let vdex = vdex.trim();
                let vdex: Vec<&str> = if vdex.is_empty() {
                    vec![]
                } else {
                    vdex.split(',')
                        .map(str::trim)
                        .filter(|s| s != &ENTRY_NAME)
                        .collect()
                };
                let vdex = vdex.join(",");
                let _ = ica.set_value(CITRIX_ICA_VDEX_PATH, &vdex);
            }
        }

        let _ = modules.delete_subkey_all(ENTRY_NAME);
    }
}

const RDP_ADDINS_PATH: &str = "Software\\Microsoft\\Terminal Server Client\\Default\\AddIns";

fn rdp_register() -> Result<(), String> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);

    let (addins, _disp) = hkcu
        .create_subkey(RDP_ADDINS_PATH)
        .map_err(|e| format!("failed to create addins: {e}"))?;

    let (entry, _disp) = addins
        .create_subkey(ENTRY_NAME)
        .map_err(|e| format!("failed to create entry: {e}"))?;

    entry
        .set_value("View Enabled", &1u32)
        .map_err(|e| format!("failed to set view enabled: {e}"))?;

    let mut dll = env::current_dir().unwrap();
    dll.push(format!("{ENTRY_NAME}.dll"));
    entry
        .set_value("Name", &dll.as_os_str())
        .map_err(|e| format!("failed to set name: {e}"))?;

    Ok(())
}

fn rdp_unregister() {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    if let Ok(addins) = hkcu.open_subkey_with_flags(RDP_ADDINS_PATH, winreg::enums::KEY_ALL_ACCESS)
    {
        let _ = addins.delete_subkey_all(ENTRY_NAME);
    }
}

#[no_mangle]
#[allow(non_snake_case, unused_variables)]
extern "system" fn DllRegisterServer() -> ws::core::HRESULT {
    unsafe { ws::Win32::System::Console::AllocConsole() };

    if let Err(e) = common::init_logs() {
        eprintln!("failed to initialize log: {e}");
    }

    let mut is_ok = true;

    if let Err(e) = rdp_register() {
        common::error!("RDP register error: {e}");
        is_ok = false;
    } else {
        common::info!("RDP registered");
    }

    if let Err(e) = citrix_register() {
        common::error!("Citrix register error: {e}");
        is_ok = false;
    } else {
        common::info!("Citrix registered");
    }

    if !is_ok {
        return ws::Win32::System::Ole::SELFREG_E_CLASS;
    }

    ws::Win32::Foundation::S_OK
}

#[no_mangle]
#[allow(non_snake_case, unused_variables)]
extern "system" fn DllUnregisterServer() -> ws::core::HRESULT {
    unsafe { ws::Win32::System::Console::AllocConsole() };

    if let Err(e) = common::init_logs() {
        eprintln!("failed to initialize log: {e}");
    }

    rdp_unregister();

    common::info!("RDP unregistered");

    citrix_unregister();

    common::info!("Citrix unregistered");

    ws::Win32::Foundation::S_OK
}
