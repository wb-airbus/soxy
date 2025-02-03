use crate::{api, service};
use std::{
    fs,
    io::{self, BufRead, Read, Write},
    net, thread,
};

const SERVICE: api::Service = api::Service::Stage0;
const SERVICE_KIND: api::ServiceKind = api::ServiceKind::Frontend;

pub struct Server {
    server: net::TcpListener,
}

impl Server {
    fn accept(stream: net::TcpStream) -> Client {
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
        thread::scope(|scope| loop {
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
            "CAT" | "PUSH" | "PUT" | "SEND" | "UPLOAD" => {
                match fs::File::options().read(true).open(args) {
                    Err(e) => {
                        writeln!(client_write, "failed to open file for reading: {e}")?;
                    }
                    Ok(mut file) => {
                        let mut buf = [0; api::CHUNK_LENGTH];

                        loop {
                            let read = file.read(&mut buf)?;

                            if read == 0 {
                                break;
                            }

                            crate::debug!("{read} bytes read");

                            rdp.write_all(&buf[0..read])?;
                        }

                        writeln!(client_write, "file sent")?;
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
}
