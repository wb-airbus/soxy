use std::env;
use std::path::PathBuf;

fn main() {
    let svc_rdp_headers = bindgen::Builder::default()
        .header("src/vc/svc/rdp/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate SVC RDP headers");

    let svc_citrix_headers = bindgen::Builder::default()
        .header("src/vc/svc/citrix/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate SVC Citrix headers");

    let dvc_freerdp_headers = bindgen::Builder::default()
        .header("src/vc/dvc/freerdp/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate DVC Freerdp headers");

    let client_x11_headers = bindgen::Builder::default()
        .header("src/client/x11/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate X11 headers");

    let client_citrix_headers = bindgen::Builder::default()
        .header("src/client/citrix/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate Citrix client headers");

    let client_freerdp_headers = bindgen::Builder::default()
        .header("src/client/freerdp/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate FreeRDP headers");

    let client_headers = bindgen::Builder::default()
        .header("src/client/headers.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .default_visibility(bindgen::FieldVisibilityKind::PublicCrate)
        .derive_debug(false)
        .derive_default(true)
        .generate()
        .expect("unable to generate client headers");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    svc_rdp_headers
        .write_to_file(out_path.join("svc_rdp_headers.rs"))
        .expect("could not write SVC RDP headers");

    svc_citrix_headers
        .write_to_file(out_path.join("svc_citrix_headers.rs"))
        .expect("could not write SVC Citrix headers");

    dvc_freerdp_headers
        .write_to_file(out_path.join("dvc_freerdp_headers.rs"))
        .expect("could not write DVC RDP headers");

    client_x11_headers
        .write_to_file(out_path.join("client_x11_headers.rs"))
        .expect("could not write X11 headers");

    client_citrix_headers
        .write_to_file(out_path.join("client_citrix_headers.rs"))
        .expect("could not write Citrix client headers");

    client_freerdp_headers
        .write_to_file(out_path.join("client_freerdp_headers.rs"))
        .expect("could not write FreeRDP headers");

    client_headers
        .write_to_file(out_path.join("client_headers.rs"))
        .expect("could not write client headers");
}
