use super::protocol;
use crate::{api, service};
use std::{
    io::{self, Write},
    net, path, thread,
};

const SERVICE: api::Service = api::Service::Ftp;
const SERVICE_KIND: api::ServiceKind = api::ServiceKind::Frontend;

pub struct Server {
    server: net::TcpListener,
    server_ip: net::IpAddr,
}

impl Server {
    const fn accept(&self, stream: net::TcpStream) -> Client {
        Client::new(stream, self.server_ip)
    }
}

impl service::Frontend for Server {
    fn bind(tcp: net::SocketAddr) -> Result<Self, io::Error> {
        let server = net::TcpListener::bind(tcp)?;
        crate::info!("accepting {SERVICE} clients on {}", server.local_addr()?);
        let server_ip = server.local_addr()?.ip();
        Ok(Self { server, server_ip })
    }

    fn start(&mut self, channel: &service::Channel) -> Result<(), io::Error> {
        thread::scope(|scope| {
            loop {
                let (client, client_addr) = self.server.accept()?;

                crate::debug!("new client {client_addr}");

                let client = self.accept(client);

                thread::Builder::new()
                    .name(format!("{SERVICE_KIND} {SERVICE} {client_addr}",))
                    .spawn_scoped(scope, move || {
                        if let Err(e) = client.start(scope, channel) {
                            crate::error!("error: {e}");
                        }
                    })?;
            }
        })
    }
}

#[derive(Debug)]
enum Command {
    Cdup,
    Cwd(String),
    Dele(String),
    Epsv,
    Feat,
    List,
    Nlst,
    Opts,
    Pass,
    Pasv,
    Pwd,
    Quit,
    Retr(String),
    Stor(String),
    Size(String),
    Type,
    User,
}

impl Command {
    fn read<R>(r: &mut R) -> Result<Option<Self>, io::Error>
    where
        R: io::BufRead,
    {
        let mut line = String::new();
        let read = r.read_line(&mut line)?;
        if read == 0 {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "disconnected"));
        }

        let line = match line.strip_suffix("\r\n") {
            None => return Ok(None),
            Some(line) => line,
        };

        crate::debug!("{line:?}");

        let (command, args) = line
            .split_once(' ')
            .map(|(command, args)| (command, args.to_string()))
            .unwrap_or((line, String::new()));
        let command = command.to_uppercase();

        let command = match command.as_str() {
            "CDUP" => Self::Cdup,
            "CWD" => Self::Cwd(args),
            "DELE" => Self::Dele(args),
            "EPSV" => Self::Epsv,
            "FEAT" => Self::Feat,
            "LIST" => Self::List,
            "NLST" => Self::Nlst,
            "OPTS" => Self::Opts,
            "PASS" => Self::Pass,
            "PASV" => Self::Pasv,
            "PWD" => Self::Pwd,
            "QUIT" => Self::Quit,
            "RETR" => Self::Retr(args),
            "SIZE" => Self::Size(args),
            "STOR" => Self::Stor(args),
            "TYPE" => Self::Type,
            "USER" => Self::User,
            _ => return Ok(None),
        };

        Ok(Some(command))
    }
}

struct Client {
    stream: net::TcpStream,
    server_ip: net::IpAddr,
}

impl Client {
    const fn new(stream: net::TcpStream, server_ip: net::IpAddr) -> Self {
        Self { stream, server_ip }
    }

    fn cmd_cdup(current_path: &mut path::PathBuf) -> Vec<String> {
        if current_path.pop() {
            vec!["250 Directory successfully changed".into()]
        } else {
            vec!["550 Failed to change directory".into()]
        }
    }

    fn cmd_dele(
        to_data: &crossbeam_channel::Sender<protocol::DataCommand>,
        from_data: &crossbeam_channel::Receiver<protocol::DataReply>,
        current_path: &path::Path,
        path: String,
    ) -> Result<Vec<String>, api::Error> {
        let mut fpath = current_path.to_path_buf();
        fpath.push(path);
        to_data.send(protocol::DataCommand::Dele(fpath.to_string_lossy().into()))?;
        let reply = from_data.recv()?;
        let res = if reply.is_ok() {
            vec!["200 Command okay".into()]
        } else {
            vec!["550 Delete failed".into()]
        };
        Ok(res)
    }

    fn cmd_list(
        to_data: &crossbeam_channel::Sender<protocol::DataCommand>,
        current_path: &path::Path,
    ) -> Result<Vec<String>, api::Error> {
        to_data.send(protocol::DataCommand::List(
            current_path.to_string_lossy().into(),
        ))?;
        Ok(vec!["150 Here comes the directory listing".into()])
    }

    fn cmd_nlst(
        to_data: &crossbeam_channel::Sender<protocol::DataCommand>,
        current_path: &path::Path,
    ) -> Result<Vec<String>, api::Error> {
        to_data.send(protocol::DataCommand::NLst(
            current_path.to_string_lossy().into(),
        ))?;
        Ok(vec!["150 Here comes the directory listing".into()])
    }

    fn cmd_retr(
        to_data: &crossbeam_channel::Sender<protocol::DataCommand>,
        current_path: &path::Path,
        path: String,
    ) -> Result<Vec<String>, api::Error> {
        let mut fpath = current_path.to_path_buf();
        fpath.push(path);
        to_data.send(protocol::DataCommand::Retr(fpath.to_string_lossy().into()))?;
        Ok(vec![
            "125 Data connection already open; transfer starting".into(),
        ])
    }

    fn cmd_size(
        to_data: &crossbeam_channel::Sender<protocol::DataCommand>,
        from_data: &crossbeam_channel::Receiver<protocol::DataReply>,
        current_path: &path::Path,
        path: String,
    ) -> Result<Vec<String>, api::Error> {
        let mut fpath = current_path.to_path_buf();
        fpath.push(path);
        to_data.send(protocol::DataCommand::Size(fpath.to_string_lossy().into()))?;
        let reply = from_data.recv()?;

        let res = if let protocol::DataReply::SizeOk(size) = reply {
            vec![format!("213 {size}")]
        } else {
            vec!["540 Invalid path".into()]
        };

        Ok(res)
    }

    fn cmd_stor(
        to_data: &crossbeam_channel::Sender<protocol::DataCommand>,
        current_path: &path::Path,
        path: String,
    ) -> Result<Vec<String>, api::Error> {
        let mut fpath = current_path.to_path_buf();
        fpath.push(path);
        to_data.send(protocol::DataCommand::Stor(fpath.to_string_lossy().into()))?;
        Ok(vec![
            "125 Data connection already open; transfer starting".into(),
        ])
    }

    fn control_loop<R>(
        client: &mut R,
        to_control: &crossbeam_channel::Sender<Vec<String>>,
        to_data: &crossbeam_channel::Sender<protocol::DataCommand>,
        from_data: &crossbeam_channel::Receiver<protocol::DataReply>,
        server_ip: net::IpAddr,
        data_port: u16,
    ) -> Result<(), api::Error>
    where
        R: io::BufRead,
    {
        let mut current_path = path::PathBuf::from("/");

        to_control.send(vec!["220 Welcome".into()])?;

        loop {
            let reply = match Command::read(client)? {
                None => vec!["500 Syntax error".into()],
                Some(command) => {
                    crate::trace!("{command:?}");
                    match command {
                        Command::Cdup => Self::cmd_cdup(&mut current_path),
                        Command::Cwd(path) => {
                            let new_path = if path.starts_with('/') {
                                path::PathBuf::from(path)
                            } else {
                                let mut res = current_path.clone();
                                res.push(path);
                                res
                            };
                            to_data.send(protocol::DataCommand::Cwd(
                                new_path.to_string_lossy().into(),
                            ))?;
                            let reply = from_data.recv()?;
                            if reply.is_ok() {
                                current_path = new_path;
                                vec!["250 Directory successfully changed".into()]
                            } else {
                                vec!["550 Failed to change directory".into()]
                            }
                        }
                        Command::Dele(path) => {
                            Self::cmd_dele(to_data, from_data, &current_path, path)?
                        }
                        Command::Epsv => vec![format!(
                            "229 Entering Extended Passive Mode (|||{data_port}|)"
                        )],
                        Command::Feat => vec![
                            "211-Features:".into(),
                            " EPRT".into(),
                            " EPSV".into(),
                            " PASV".into(),
                            " REST STREAM".into(),
                            " SIZE".into(),
                            " TVFS".into(),
                            " UTF8".into(),
                            "211 End".into(),
                        ],
                        Command::List => Self::cmd_list(to_data, &current_path)?,
                        Command::Nlst => Self::cmd_nlst(to_data, &current_path)?,
                        Command::Opts | Command::Type => vec!["200 Command okay".into()],
                        Command::Pass => vec!["220 Login successful".into()],
                        Command::Pasv => match &server_ip {
                            net::IpAddr::V4(ip) => {
                                let ip = ip.to_bits().to_be_bytes();
                                let port = data_port.to_be_bytes();
                                vec![format!(
                                    "227 Entering Passive Mode ({},{},{},{},{},{})",
                                    ip[0], ip[1], ip[2], ip[3], port[0], port[1]
                                )]
                            }
                            net::IpAddr::V6(_) => {
                                vec!["425 Can't open data connection".into()]
                            }
                        },
                        Command::Pwd => vec![format!(
                            "257 {:?} is the current directory",
                            current_path.as_os_str().to_string_lossy()
                        )],
                        Command::Quit => return Ok(()),
                        Command::Retr(path) => Self::cmd_retr(to_data, &current_path, path)?,
                        Command::Size(path) => {
                            Self::cmd_size(to_data, from_data, &current_path, path)?
                        }
                        Command::Stor(path) => Self::cmd_stor(to_data, &current_path, path)?,
                        Command::User => vec!["331 Provide password".into()],
                    }
                }
            };

            to_control.send(reply)?;
        }
    }

    fn data_transfer(
        client: net::TcpStream,
        mut rdp: service::RdpStream,
        cmd: &protocol::DataCommand,
    ) -> Result<bool, io::Error> {
        let mut status = false;

        if cmd.is_upload() {
            let _ = client.shutdown(net::Shutdown::Write);

            let mut client = io::BufReader::new(client);
            if let Err(e) = service::stream_copy(&mut client, &mut rdp) {
                crate::debug!("error: {e}");
            } else {
                crate::debug!("stopped");
                status = true;
            }
            let client = client.into_inner();
            let _ = client.shutdown(net::Shutdown::Both);
        } else {
            let _ = client.shutdown(net::Shutdown::Read);

            let mut client = io::BufWriter::new(client);
            if let Err(e) = service::stream_copy(&mut rdp, &mut client) {
                crate::debug!("error: {e}");
            } else {
                crate::debug!("stopped");
                status = true;
            }
            let _ = client.flush();
            if let Ok(client) = client.into_inner() {
                let _ = client.shutdown(net::Shutdown::Both);
            }
        }

        rdp.disconnect()?;

        Ok(status)
    }

    fn data_loop<'a>(
        data_server: &net::TcpListener,
        from_control: &crossbeam_channel::Receiver<protocol::DataCommand>,
        to_control: &crossbeam_channel::Sender<protocol::DataReply>,
        to_client: &crossbeam_channel::Sender<Vec<String>>,
        channel: &'a service::Channel,
        scope: &'a thread::Scope<'a, '_>,
    ) -> Result<(), api::Error> {
        loop {
            let cmd = from_control.recv()?;

            let mut rdp = channel.connect(SERVICE)?;

            cmd.send(&mut rdp)?;

            crate::trace!("{cmd}");

            if cmd.is_ftp_control() {
                to_control.send(protocol::DataReply::receive(&mut rdp)?)?;

                let _ = rdp.disconnect();
            } else {
                let (client, client_addr) = data_server.accept()?;

                crate::debug!("connection from {client_addr}");

                let to_client = to_client.clone();
                thread::Builder::new()
                    .name(format!("{SERVICE_KIND} {SERVICE} data {client_addr}"))
                    .spawn_scoped(scope, move || {
                        match Self::data_transfer(client, rdp, &cmd) {
                            Err(e) => {
                                crate::debug!("error {e}");
                            }
                            Ok(status) => {
                                if status {
                                    let _ =
                                        to_client.send(vec!["226 Closing data connection".into()]);
                                } else {
                                    let _ = to_client.send(vec![
                                        "426 Connection closed; transfer aborted".into(),
                                    ]);
                                }
                            }
                        }
                    })
                    .unwrap();
            }
        }
    }

    fn to_control<W>(
        from: &crossbeam_channel::Receiver<Vec<String>>,
        client: &mut W,
    ) -> Result<(), api::Error>
    where
        W: io::Write,
    {
        loop {
            let msgs = from.recv()?;
            for msg in msgs {
                write!(client, "{msg}\r\n")?;
                client.flush()?;
            }
        }
    }

    fn start<'a>(
        self,
        scope: &'a thread::Scope<'a, '_>,
        channel: &'a service::Channel,
    ) -> Result<(), io::Error> {
        let data_server = net::TcpListener::bind((self.server_ip, 0))?;
        let data_port = data_server.local_addr().unwrap().port();

        let (control_to_data_send, control_to_data_receive) = crossbeam_channel::bounded(1);
        let (data_to_control_send, data_to_control_receive) = crossbeam_channel::bounded(1);
        let (to_control_send, to_control_receive) = crossbeam_channel::bounded(1);

        let lstream = self.stream.try_clone().unwrap();
        thread::Builder::new()
            .name(format!(
                "{SERVICE_KIND} {SERVICE} control {}",
                self.stream
                    .peer_addr()
                    .map_or_else(|_| "<unknown>".into(), |a| a.to_string())
            ))
            .spawn_scoped(scope, move || {
                let mut lstream = io::BufWriter::new(lstream);
                if let Err(e) = Self::to_control(&to_control_receive, &mut lstream) {
                    crate::debug!("error: {e}");
                } else {
                    crate::debug!("stopped");
                }
                let _ = lstream.flush();
                if let Ok(lstream) = lstream.into_inner() {
                    let _ = lstream.shutdown(net::Shutdown::Both);
                }
            })
            .unwrap();

        let lto_control_send = to_control_send.clone();
        thread::Builder::new()
            .name(format!(
                "{SERVICE_KIND} {SERVICE} data {}",
                self.stream
                    .peer_addr()
                    .map_or_else(|_| "<unknown>".into(), |a| a.to_string())
            ))
            .spawn_scoped(scope, move || {
                if let Err(e) = Self::data_loop(
                    &data_server,
                    &control_to_data_receive,
                    &data_to_control_send,
                    &lto_control_send,
                    channel,
                    scope,
                ) {
                    crate::debug!("error: {e}");
                } else {
                    crate::debug!("stopped");
                }
            })
            .unwrap();

        let mut stream = io::BufReader::new(self.stream);
        if let Err(e) = Self::control_loop(
            &mut stream,
            &to_control_send,
            &control_to_data_send,
            &data_to_control_receive,
            self.server_ip,
            data_port,
        ) {
            crate::debug!("error: {e}");
        } else {
            crate::debug!("stopped");
        }
        let stream = stream.into_inner();
        let _ = stream.shutdown(net::Shutdown::Both);

        Ok(())
    }
}
