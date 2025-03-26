use crate::{api, service};
use std::{
    fs,
    io::{self, BufRead, Read, Write},
    net, thread,
};

pub(crate) fn tcp_handler(
    _server: &service::TcpFrontendServer,
    _scope: &thread::Scope,
    stream: net::TcpStream,
    channel: &service::Channel,
) -> Result<(), api::Error> {
    let lstream = stream.try_clone()?;
    let mut client_read = io::BufReader::new(lstream);

    let mut client_write = io::BufWriter::new(stream);

    let mut rdp = channel.connect(&super::SERVICE)?;

    let mut line = String::new();

    let _ = client_read.read_line(&mut line)?;

    let cline = line
        .strip_suffix("\n")
        .ok_or(io::Error::new(io::ErrorKind::BrokenPipe, "interrupted"))?;

    let cline = if cline.ends_with('\r') {
        cline.strip_suffix('\r').unwrap()
    } else {
        cline
    };

    let (command, args) = cline
        .split_once(' ')
        .map(|(command, args)| (command, args.to_string()))
        .unwrap_or((cline, String::new()));
    let command = command.to_uppercase();

    crate::debug!("{cline:?}");
    crate::trace!("COMMAND = {command:?}");
    crate::trace!("ARGS = {args:?}");

    match command.as_str() {
        "EXIT" | "QUIT" => (),
        "CAT" | "PUSH" | "PUT" | "SEND" | "UPLOAD" => {
            match fs::File::options().read(true).open(args) {
                Err(e) => {
                    writeln!(client_write, "failed to open file for reading: {e}")?;
                }
                Ok(mut file) => {
                    let mut buf = [0; api::CHUNK_LENGTH];

                    let mut total = 0;

                    loop {
                        let read = file.read(&mut buf)?;

                        if read == 0 {
                            break;
                        }

                        crate::trace!("{read} bytes read");

                        rdp.write_all(&buf[0..read])?;

                        total += read;
                    }

                    writeln!(client_write, "file sent ({total} bytes)")?;
                }
            }
        }
        _ => writeln!(client_write, "invalid command")?,
    }

    client_write.flush()?;

    let _ = rdp.disconnect();
    let lstream = client_read.into_inner();
    let _ = lstream.shutdown(net::Shutdown::Both);

    Ok(())
}
