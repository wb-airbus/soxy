use super::protocol;
use crate::{api, channel, frontend, service};
use std::{
    io::{self, BufRead, Write},
    net, thread,
};

// https://patorjk.com/software/taag/#p=display&h=0&v=0&f=Ogre&t=clipboard%0A
const LOGO: &str = r"
       _  _         _                              _
  ___ | |(_) _ __  | |__    ___    __ _  _ __   __| |
 / __|| || || '_ \ | '_ \  / _ \  / _` || '__| / _` |
| (__ | || || |_) || |_) || (_) || (_| || |   | (_| |
 \___||_||_|| .__/ |_.__/  \___/  \__,_||_|    \__,_|
            |_|";

const HELP: &str = r#"
Available commands:
- "read" or "get" to get remote clipboard content;
- "write XXX" or "put XXX" to set remote clipboard content to XXX;
- "exit" or "quit" to exit this intrerface.
"#;

const PROMPT: &str = "clipboard> ";

pub fn tcp_handler<'a>(
    _server: &frontend::FrontendTcpServer,
    _scope: &'a thread::Scope<'a, '_>,
    stream: net::TcpStream,
    channel: &'a channel::Channel,
) -> Result<(), api::Error> {
    let lstream = stream.try_clone()?;
    let mut client_read = io::BufReader::new(lstream);

    let mut client_write = io::BufWriter::new(stream);

    client_write.write_fmt(format_args!("{}\n{}\n{}\n", service::LOGO, LOGO, HELP))?;
    client_write.flush()?;

    let mut rdp = channel.connect(&super::SERVICE)?;

    let mut line = String::new();

    loop {
        client_write.write_all(PROMPT.as_bytes())?;
        client_write.flush()?;

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
            "" => (),
            "READ" | "GET" => {
                protocol::Command::Read.send(&mut rdp)?;
                match protocol::Response::receive(&mut rdp)? {
                    protocol::Response::Text(value) => {
                        writeln!(client_write, "ok {value:?}")?;
                    }
                    protocol::Response::Failed => {
                        writeln!(client_write, "KO")?;
                    }
                    protocol::Response::WriteDone => unreachable!(),
                }
            }
            "WRITE" | "PUT" => {
                protocol::Command::WriteText(args).send(&mut rdp)?;
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
