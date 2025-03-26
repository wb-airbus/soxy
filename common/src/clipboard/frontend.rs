use super::protocol;
use crate::{api, service};
use std::{
    io::{self, BufRead, Write},
    net, thread,
};

const SERVICE: api::Service = api::Service::Clipboard;
const SERVICE_KIND: api::ServiceKind = api::ServiceKind::Frontend;

pub struct Server {
    server: net::TcpListener,
}

impl Server {
    const fn accept(stream: net::TcpStream) -> Client {
        Client { stream }
    }
}

impl service::Frontend for Server {
    fn bind(tcp: net::SocketAddr) -> Result<Self, io::Error> {
        let server = net::TcpListener::bind(tcp)?;
        crate::info!("accepting {SERVICE} clients on {}", server.local_addr()?);
        Ok(Self { server })
    }

    fn start(&mut self, channel: &service::Channel) -> Result<(), io::Error> {
        thread::scope(|scope| {
            loop {
                let (client, client_addr) = self.server.accept()?;

                crate::debug!("new client {client_addr}");

                let client = Self::accept(client);

                thread::Builder::new()
                    .name(format!("{SERVICE_KIND} {SERVICE} {client_addr}"))
                    .spawn_scoped(scope, move || {
                        if let Err(e) = client.start(channel) {
                            crate::debug!("error: {e}");
                        }
                    })?;
            }
        })
    }
}

struct Client {
    stream: net::TcpStream,
}

impl Client {
    fn start(self, channel: &service::Channel) -> Result<(), io::Error> {
        let lstream = self.stream.try_clone()?;
        let mut client_read = io::BufReader::new(lstream);

        let mut client_write = io::BufWriter::new(self.stream);

        let mut rdp = channel.connect(SERVICE)?;

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
                        protocol::Response::Clipboard(value) => {
                            writeln!(client_write, "ok {value:?}")?;
                        }
                        protocol::Response::Failed => {
                            writeln!(client_write, "KO")?;
                        }
                        protocol::Response::WriteDone => unreachable!(),
                    }
                }
                "WRITE" | "PUT" => {
                    protocol::Command::Write(args).send(&mut rdp)?;
                    match protocol::Response::receive(&mut rdp)? {
                        protocol::Response::WriteDone => {
                            writeln!(client_write, "ok")?;
                        }
                        protocol::Response::Failed => {
                            writeln!(client_write, "KO")?;
                        }
                        protocol::Response::Clipboard(_) => unreachable!(),
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
}
