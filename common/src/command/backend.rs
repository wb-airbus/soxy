use crate::{rdp, service};
use std::{io, process, thread};

pub fn backend_handler(rdp_stream: rdp::RdpStream<'_>) -> Result<(), io::Error> {
    #[cfg(feature = "log")]
    let client_id = rdp_stream.client_id();
    #[cfg(target_os = "windows")]
    let cmd = "cmd.exe";
    #[cfg(target_os = "windows")]
    let args: [String; 0] = [];

    #[cfg(not(target_os = "windows"))]
    let cmd = "sh";
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

        let thread = thread::Builder::new();
        #[cfg(feature = "log")]
        let thread = thread.name(format!(
            "{} {} {client_id:x} stdout",
            service::Kind::Backend,
            super::SERVICE,
        ));
        thread
            .spawn_scoped(scope, move || {
                if let Err(e) = service::stream_copy(&mut stdout, &mut rdp_stream_write_out, true) {
                    crate::debug!("error: {e}");
                } else {
                    crate::debug!("stopped");
                }
            })
            .unwrap();

        let thread = thread::Builder::new();
        #[cfg(feature = "log")]
        let thread = thread.name(format!(
            "{} {} {client_id:x} stderr",
            service::Kind::Backend,
            super::SERVICE,
        ));
        thread
            .spawn_scoped(scope, move || {
                if let Err(e) = service::stream_copy(&mut stderr, &mut rdp_stream_write_err, true) {
                    crate::debug!("error: {e}");
                } else {
                    crate::debug!("stopped");
                }
            })
            .unwrap();

        if let Err(e) = service::stream_copy(&mut rdp_stream_read, &mut stdin, true) {
            crate::debug!("error: {e}");
        } else {
            crate::debug!("stopped");
        }

        Ok(())
    })
}
