use super::protocol;
use crate::service;
use std::{
    io::{self, BufRead, Write},
    net, thread,
};

pub(crate) fn tcp_handler<'a>(
    _server: &service::TcpFrontendServer,
    _scope: &'a thread::Scope<'a, '_>,
    stream: net::TcpStream,
    channel: &'a service::Channel,
) -> Result<(), io::Error> {
    let lstream = stream.try_clone()?;
    let mut client_read = io::BufReader::new(lstream);

    let mut client_write = io::BufWriter::new(stream);

    let mut rdp = channel.connect(&super::SERVICE)?;

    let mut line = String::new();

    loop {
        let _ = client_read.read_line(&mut line)?;

        let cline = line
            .strip_suffix("\n")
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "interrupted"))?;

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
            "READ" | "GET" => {
                protocol::Command::Read.send(&mut rdp)?;
                match protocol::Response::receive(&mut rdp)? {
                    protocol::Response::Text(value) => {
                        let value = String::from_utf8_lossy(&value);
                        writeln!(client_write, "ok {value:?}")?;
                    }
                    protocol::Response::Failed => {
                        writeln!(client_write, "KO")?;
                    }
                    protocol::Response::WriteDone => unreachable!(),
                }
            }
            "WRITE" | "PUT" => {
                protocol::Command::WriteText(args.into_bytes()).send(&mut rdp)?;
                match protocol::Response::receive(&mut rdp)? {
                    protocol::Response::WriteDone => {
                        writeln!(client_write, "ok")?;
                    }
                    protocol::Response::Failed => {
                        writeln!(client_write, "KO")?;
                    }
                    protocol::Response::Text(_) => unreachable!(),
                }
            }
            "EXIT" | "QUIT" => {
                let _ = rdp.disconnect();
                let lstream = client_read.into_inner();
                let _ = lstream.shutdown(net::Shutdown::Both);
                return Ok(());
            }
            _ => writeln!(client_write, "invalid command")?,
        }
        client_write.flush()?;

        line.clear();
    }
}
