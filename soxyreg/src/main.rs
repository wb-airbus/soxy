use clap::Parser;
use std::{env, path};

mod dvc;
mod svc;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(value_enum)]
    mode: Mode,
    #[command(subcommand)]
    operation: Operation,
}

#[derive(clap::ValueEnum, Clone)]
enum Mode {
    Dvc,
    Svc,
}

#[derive(clap::Subcommand)]
enum Operation {
    Register { dll: String },
    Unregister,
}

fn main() {
    let args = Args::parse();

    match (args.operation, args.mode) {
        (Operation::Register { dll }, Mode::Dvc) => {
            let path = path::Path::new(&dll);

            if !path.exists() {
                eprintln!("invalid DLL path");
                return;
            }

            if !path.is_file() {
                eprintln!("given path is not a file");
                return;
            }

            let path = if path.is_absolute() {
                path::PathBuf::from(path)
            } else {
                let curdir = env::current_dir().expect("invalid current dir");
                curdir.join(path)
            };

            let path = path.as_os_str().to_str().unwrap();

            println!("DLL path = {path}");

            dvc::register(path);
        }
        (Operation::Unregister, Mode::Dvc) => dvc::unregister(),
        (Operation::Register { dll }, Mode::Svc) => {
            let path = path::Path::new(&dll);

            if !path.exists() {
                eprintln!("invalid DLL path");
                return;
            }

            if !path.is_file() {
                eprintln!("given path is not a file");
                return;
            }

            let path = if path.is_absolute() {
                path::PathBuf::from(path)
            } else {
                let curdir = env::current_dir().expect("invalid current dir");
                curdir.join(path)
            };

            let file_name = path.file_name().unwrap().to_str().unwrap();

            let path = path.as_os_str().to_str().unwrap();

            println!("DLL path = {path}");
            println!(
                "Do not forget to put {file_name} in C:\\Program Files (x86)\\Citrix\\ICA Client\\ !!!!!!"
            );

            svc::register(path, file_name);
        }
        (Operation::Unregister, Mode::Svc) => svc::unregister(),
    }
}
