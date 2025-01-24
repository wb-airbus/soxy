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

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    rdp_bindings
        .write_to_file(out_path.join("rdp_headers.rs"))
        .expect("could not write RDP bindings");

    citrix_bindings
        .write_to_file(out_path.join("citrix_headers.rs"))
        .expect("could not write Citrix bindings");
}
