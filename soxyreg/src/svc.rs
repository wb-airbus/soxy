use std::io;

const ENTRY_NAME: &str = "soxy";

const CITRIX_MACHINE_MODULES_PATH: &str =
    "Software\\Citrix\\ICA Client\\Engine\\Configuration\\Advanced\\Modules";

const CITRIX_MODULES_ICA_PATH: &str = "ICA 3.0";

const CITRIX_ICA_VDEX_PATH: &str = "VirtualDriverEx";

const CITRIX_ENTRY_DRIVER_NAME: &str = "DriverName";
const CITRIX_ENTRY_DRIVER_NAME_WIN16: &str = "DriverNameWin16";
const CITRIX_ENTRY_DRIVER_NAME_WIN32: &str = "DriverNameWin32";

fn citrix_register(dll_file_name: &str) -> Result<(), String> {
    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);

    let path = CITRIX_MACHINE_MODULES_PATH;

    let (modules, _disp) = hklm
        .create_subkey(path)
        .map_err(|e| format!("failed to create citrix modules path: {e}"))?;

    let (ica, _disp) = modules
        .create_subkey(CITRIX_MODULES_ICA_PATH)
        .map_err(|e| format!("failed to create citrix modules virtual driver path: {e}"))?;

    let vdex: String = ica.get_value(CITRIX_ICA_VDEX_PATH).unwrap_or(String::new());
    let mut vdex: Vec<&str> = if vdex.trim().is_empty() {
        vec![]
    } else {
        vdex.split(',')
            .map(str::trim)
            .filter(|e| e != &ENTRY_NAME)
            .collect()
    };
    vdex.push(ENTRY_NAME);
    let vdex = vdex.join(",");
    ica.set_value(CITRIX_ICA_VDEX_PATH, &vdex)
        .map_err(|e| format!("failed to set name: {e}"))?;

    let (entry, _disp) = modules
        .create_subkey(ENTRY_NAME)
        .map_err(|e| format!("failed to create citrix modules entry path: {e}"))?;
    entry
        .set_value(CITRIX_ENTRY_DRIVER_NAME, &dll_file_name)
        .map_err(|e| format!("failed to set name: {e}"))?;
    entry
        .set_value(CITRIX_ENTRY_DRIVER_NAME_WIN16, &dll_file_name)
        .map_err(|e| format!("failed to set name: {e}"))?;
    entry
        .set_value(CITRIX_ENTRY_DRIVER_NAME_WIN32, &dll_file_name)
        .map_err(|e| format!("failed to set name: {e}"))?;

    Ok(())
}

fn citrix_unregister() -> Result<(), String> {
    let mut res = Ok(());

    let hklm = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE);
    let path = CITRIX_MACHINE_MODULES_PATH;

    if let Ok(modules) = hklm.open_subkey_with_flags(path, winreg::enums::KEY_ALL_ACCESS) {
        if let Ok(ica) =
            modules.open_subkey_with_flags(CITRIX_MODULES_ICA_PATH, winreg::enums::KEY_ALL_ACCESS)
            && let Ok(vdex) = ica.get_value::<String, _>(CITRIX_ICA_VDEX_PATH)
        {
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
            if let Err(e) = ica.set_value(CITRIX_ICA_VDEX_PATH, &vdex)
                && e.kind() != io::ErrorKind::NotFound
            {
                res = Err(format!(
                    "failed to alter {path}\\{CITRIX_MODULES_ICA_PATH}\\{CITRIX_ICA_VDEX_PATH}: {e}"
                ));
            }
        }

        if let Err(e) = modules.delete_subkey_all(ENTRY_NAME)
            && e.kind() != io::ErrorKind::NotFound
        {
            res = Err(format!("failed delete {path}\\{ENTRY_NAME}: {e}"));
        }
    }

    res
}

const RDP_ADDINS_PATH: &str = "Software\\Microsoft\\Terminal Server Client\\Default\\AddIns";

fn rdp_register(dll_path: &str) -> Result<(), String> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);

    let (addins, _disp) = hkcu
        .create_subkey(RDP_ADDINS_PATH)
        .map_err(|e| format!("failed to create addins: {e}"))?;

    let (entry, _disp) = addins
        .create_subkey(ENTRY_NAME)
        .map_err(|e| format!("failed to create entry: {e}"))?;

    entry
        .set_value("Name", &dll_path)
        .map_err(|e| format!("failed to set name: {e}"))?;

    Ok(())
}

fn rdp_unregister() -> Result<(), String> {
    let mut res = Ok(());

    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);

    if let Ok(addins) = hkcu.open_subkey_with_flags(RDP_ADDINS_PATH, winreg::enums::KEY_ALL_ACCESS)
        && let Err(e) = addins.delete_subkey_all(ENTRY_NAME)
        && e.kind() != io::ErrorKind::NotFound
    {
        res = Err(format!(
            "failed delete {RDP_ADDINS_PATH}\\{ENTRY_NAME}: {e}"
        ));
    }

    res
}

pub fn register(dll_path: &str, dll_file_name: &str) {
    if let Err(e) = rdp_register(dll_path) {
        eprintln!("RDP register error: {e}");
    } else {
        println!("RDP registered");
    }

    if let Err(e) = citrix_register(dll_file_name) {
        eprintln!("Citrix register error: {e}");
    } else {
        println!(
            "Do not forget to put {dll_file_name} in C:\\Program Files (x86)\\Citrix\\ICA Client\\ !!!!!!"
        );
        println!("Citrix registered");
    }
}

pub fn unregister() {
    if let Err(e) = rdp_unregister() {
        eprintln!("RDP unregister error: {e}");
    } else {
        println!("RDP unregistered");
    }

    if let Err(e) = citrix_unregister() {
        eprintln!("Citrix unregister error: {e}");
    } else {
        println!("Citrix unregistered");
    }
}
