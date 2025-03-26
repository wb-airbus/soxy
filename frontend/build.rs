use std::env;
use std::path::PathBuf;

fn main() {
    let rdp_bindings = bindgen::Builder::default()
        .header("src/svc/rdp/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate RDP bindings");

    let citrix_bindings = bindgen::Builder::default()
        .header("src/svc/citrix/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate Citrix bindings");

    let x11_bindings = bindgen::Builder::default()
        .header("src/client/x11/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate X11 bindings");

    let citrix_client_bindings = bindgen::Builder::default()
        .header("src/client/citrix/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate Citrix client bindings");

    let freerdp_bindings = bindgen::Builder::default()
        .header("src/client/freerdp/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate FreeRDP bindings");

    let client_bindings = bindgen::Builder::default()
        .header("src/client/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate client bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    rdp_bindings
        .write_to_file(out_path.join("rdp_headers.rs"))
        .expect("could not write RDP bindings");

    citrix_bindings
        .write_to_file(out_path.join("citrix_headers.rs"))
        .expect("could not write Citrix bindings");

    x11_bindings
        .write_to_file(out_path.join("x11_headers.rs"))
        .expect("could not write X11 bindings");

    citrix_client_bindings
        .write_to_file(out_path.join("citrix_client_headers.rs"))
        .expect("could not write Citrix client bindings");

    freerdp_bindings
        .write_to_file(out_path.join("freerdp_headers.rs"))
        .expect("could not write FreeRDP bindings");

    client_bindings
        .write_to_file(out_path.join("client_headers.rs"))
        .expect("could not write client bindings");
}
