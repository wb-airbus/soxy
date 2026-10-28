use crate::{api, channel, frontend, service};
use std::{
    fs,
    io::{self, BufRead, Read, Write},
    net, thread,
};

// https://patorjk.com/software/taag/#p=display&h=0&v=0&f=Ogre&t=stage0%0A
const LOGO: &str = r"
      _                         ___
 ___ | |_   __ _   __ _   ___  / _ \
/ __|| __| / _` | / _` | / _ \| | | |
\__ \| |_ | (_| || (_| ||  __/| |_| |
|___/ \__| \__,_| \__, | \___| \___/
                  |___/";

const HELP: &str = r#"
Available commands:
- "cat FILE" or "push FILE" or "put FILE" or "send FILE" or "upload FILE" to uplaod the content of FILE;
- "exit" or "quit" to exit this intrerface.
"#;

const PROMPT: &str = "stage0> ";

pub fn tcp_handler(
    _server: &frontend::FrontendTcpServer,
    _scope: &thread::Scope,
    stream: net::TcpStream,
    channel: &channel::Channel,
) -> Result<(), api::Error> {
    let lstream = stream.try_clone()?;
    let mut client_read = io::BufReader::new(lstream);

    let mut client_write = io::BufWriter::new(stream);

    client_write.write_fmt(format_args!("{}\n{}\n{}\n", service::LOGO, LOGO, HELP))?;
    client_write.flush()?;

    let mut rdp = channel.connect(&super::SERVICE)?;

    let mut line = String::new();

    client_write.write_all(PROMPT.as_bytes())?;
    client_write.flush()?;

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
                    let mut buf = [0; api::PDU_DATA_MAX_SIZE];

                    let mut total = 0;

                    loop {
                        let read = file.read(&mut buf)?;

                        if read == 0 {
                            break;
                        }

                        crate::trace!("{read} bytes read");

                        rdp.write_all(&buf[..read])?;

                        total += read;
                    }

                    writeln!(client_write, "file sent ({total} bytes)")?;
                }
            }
        }
        _ => writeln!(client_write, "invalid command")?,
    }

    client_write.flush()?;

    let lstream = client_read.into_inner();
    let _ = lstream.shutdown(net::Shutdown::Both);

    Ok(())
}
