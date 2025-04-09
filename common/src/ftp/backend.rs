use super::protocol;
use crate::service;
use std::{
    fs,
    io::{self, Write},
    path,
};

fn cmd_cwd(stream: &mut service::RdpStream<'_>, path: String) -> Result<(), io::Error> {
    crate::info!("change directory {path:?}");

    let path = path::PathBuf::from(path);
    if path.exists() {
        protocol::DataReply::CwdOk.send(stream)?;
    } else {
        protocol::DataReply::Ko.send(stream)?;
    }

    Ok(())
}

fn cmd_dele(stream: &mut service::RdpStream<'_>, path: String) -> Result<(), io::Error> {
    crate::info!("delete {path:?}");

    let path = path::PathBuf::from(path);
    if path.exists() {
        if let Err(e) = fs::remove_file(path) {
            crate::error!("failed to delete: {e}");
            protocol::DataReply::Ko.send(stream)?;
        } else {
            protocol::DataReply::DeleteOk.send(stream)?;
        }
    } else {
        protocol::DataReply::Ko.send(stream)?;
    }

    Ok(())
}

fn cmd_list(stream: &mut service::RdpStream<'_>, path: String) -> Result<(), io::Error> {
    crate::info!("list {path:?}");

    let path = path::PathBuf::from(path);
    if path.exists() {
        if let Ok(dir) = path.read_dir() {
            dir.into_iter().try_for_each(|entry| {
                if let Ok(entry) = entry {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_dir() {
                            write!(stream, "d")?;
                        } else if file_type.is_file() {
                            write!(stream, "-")?;
                        } else {
                            write!(stream, "l")?;
                        }
                        let _ = write!(stream, "rwxrwxrwx 1 ftp ftp ");
                        if let Ok(metadata) = entry.metadata() {
                            write!(stream, "{}", metadata.len())?;
                        } else {
                            write!(stream, "0")?;
                        }
                        write!(stream, " {}\r\n", entry.file_name().into_string().unwrap())?;
                    }
                }
                Ok::<(), io::Error>(())
            })?;
        }
    }

    Ok(())
}

fn cmd_nlst(stream: &mut service::RdpStream<'_>, path: String) -> Result<(), io::Error> {
    crate::info!("name list {path:?}");

    let path = path::PathBuf::from(path);
    if path.exists() {
        if let Ok(dir) = path.read_dir() {
            dir.into_iter().try_for_each(|entry| {
                if let Ok(entry) = entry {
                    write!(stream, "{}\r\n", entry.file_name().into_string().unwrap())?;
                }
                Ok::<(), io::Error>(())
            })?;
        }
    }
    Ok(())
}

fn cmd_retr(stream: &mut service::RdpStream<'_>, path: String) -> Result<(), io::Error> {
    crate::info!("downloading {path:?}");

    let path = path::PathBuf::from(path);
    if path.exists() && path.is_file() {
        let file = fs::File::options().read(true).write(false).open(path)?;
        let mut file = io::BufReader::new(file);

        if let Err(e) = service::stream_copy(&mut file, stream) {
            crate::debug!("error: {e}");
        } else {
            crate::debug!("stopped");
        }
    }

    Ok(())
}

fn cmd_size(stream: &mut service::RdpStream<'_>, path: String) -> Result<(), io::Error> {
    crate::info!("size {path:?}");

    let path = path::PathBuf::from(path);
    if path.exists() {
        if let Ok(metadata) = path.metadata() {
            let size = metadata.len();
            protocol::DataReply::SizeOk(size).send(stream)?;
        } else {
            protocol::DataReply::Ko.send(stream)?;
        }
    } else {
        protocol::DataReply::Ko.send(stream)?;
    }

    Ok(())
}

fn cmd_stor(stream: &mut service::RdpStream<'_>, path: String) -> Result<(), io::Error> {
    crate::info!("uploading {path:?}");

    let path = path::PathBuf::from(path);
    let file = fs::File::options()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    let mut file = io::BufWriter::new(file);

    if let Err(e) = service::stream_copy(stream, &mut file) {
        crate::debug!("error: {e}");
    } else {
        crate::debug!("stopped");
    }

    Ok(())
}

pub(crate) fn handler(mut stream: service::RdpStream<'_>) -> Result<(), io::Error> {
    crate::debug!("starting");

    let cmd = protocol::DataCommand::receive(&mut stream)?;

    match cmd {
        protocol::DataCommand::Cwd(path) => {
            #[cfg(target_os = "windows")]
            let path = "C:".to_string() + path.as_str();

            cmd_cwd(&mut stream, path)?;
        }
        protocol::DataCommand::Dele(path) => {
            #[cfg(target_os = "windows")]
            let path = "C:".to_string() + path.as_str();

            cmd_dele(&mut stream, path)?;
        }
        protocol::DataCommand::List(path) => {
            #[cfg(target_os = "windows")]
            let path = "C:".to_string() + path.as_str();

            cmd_list(&mut stream, path)?;
        }
        protocol::DataCommand::NLst(path) => {
            #[cfg(target_os = "windows")]
            let path = "C:".to_string() + path.as_str();

            cmd_nlst(&mut stream, path)?;
        }
        protocol::DataCommand::Retr(path) => {
            #[cfg(target_os = "windows")]
            let path = "C:".to_string() + path.as_str();

            cmd_retr(&mut stream, path)?;
        }
        protocol::DataCommand::Size(path) => {
            #[cfg(target_os = "windows")]
            let path = "C:".to_string() + path.as_str();

            cmd_size(&mut stream, path)?;
        }
        protocol::DataCommand::Stor(path) => {
            #[cfg(target_os = "windows")]
            let path = "C:".to_string() + path.as_str();

            cmd_stor(&mut stream, path)?;
        }
    }

    stream.disconnect()
}
