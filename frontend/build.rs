use std::env;
use std::path::PathBuf;

fn main() {
    let rdp_bindings = bindgen::Builder::default()
        .header("rdp_api.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate rdp bindings");

    #[cfg(feature = "citrix")]
    let citrix_bindings = bindgen::Builder::default()
        .header("citrix_api.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate citrix bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    rdp_bindings
        .write_to_file(out_path.join("rdp_api.rs"))
        .expect("could not write rdp bindings");

    #[cfg(feature = "citrix")]
    citrix_bindings
        .write_to_file(out_path.join("citrix_api.rs"))
        .expect("could not write citrix bindings");
}
