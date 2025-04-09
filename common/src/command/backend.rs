use crate::service;
use std::{io, process, thread};

pub(crate) fn backend_handler(rdp_stream: service::RdpStream<'_>) -> Result<(), io::Error> {
    let client_id = rdp_stream.client_id();

    #[cfg(target_os = "windows")]
    let cmd = "cmd.exe";
    #[cfg(not(target_os = "windows"))]
    let cmd = "sh";

    #[cfg(target_os = "windows")]
    let args: [String; 0] = [];
    #[cfg(not(target_os = "windows"))]
    let args = ["-i"];

    crate::debug!("starting {cmd:?}");

    thread::scope(|scope| {
        let child = process::Command::new(cmd)
            .args(args)
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped())
            .spawn()?;

        let mut stdin = child
            .stdin
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no stdin"))?;
        let mut stdout = child
            .stdout
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no stdout"))?;
        let mut stderr = child
            .stderr
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no stderr"))?;

        let (mut rdp_stream_read, mut rdp_stream_write_out) = rdp_stream.split();
        let mut rdp_stream_write_err = rdp_stream_write_out.clone();

        thread::Builder::new()
            .name(format!(
                "{} {} {client_id:x} stdout",
                service::Kind::Backend,
                super::SERVICE,
            ))
            .spawn_scoped(scope, move || {
                if let Err(e) = service::stream_copy(&mut stdout, &mut rdp_stream_write_out) {
                    crate::debug!("error: {e}");
                } else {
                    crate::debug!("stopped");
                }
            })
            .unwrap();

        thread::Builder::new()
            .name(format!(
                "{} {} {client_id:x} stderr",
                service::Kind::Backend,
                super::SERVICE,
            ))
            .spawn_scoped(scope, move || {
                if let Err(e) = service::stream_copy(&mut stderr, &mut rdp_stream_write_err) {
                    crate::debug!("error: {e}");
                } else {
                    crate::debug!("stopped");
                }
            })
            .unwrap();

        if let Err(e) = service::stream_copy(&mut rdp_stream_read, &mut stdin) {
            crate::debug!("error: {e}");
        } else {
            crate::debug!("stopped");
        }
        rdp_stream_read.disconnect();

        Ok(())
    })
}
