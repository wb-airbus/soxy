use super::protocol;
use crate::{api, channel, frontend, service};
use std::{
    io::{self, Write},
    net, thread,
};

fn parse_command<R>(r: &mut R) -> Result<Option<protocol::ControlCommand>, io::Error>
where
    R: io::BufRead,
{
    let mut line = String::new();
    let read = r.read_line(&mut line)?;
    if read == 0 {
        return Err(io::Error::new(io::ErrorKind::BrokenPipe, "disconnected"));
    }

    let Some(line) = line.strip_suffix("\r\n") else {
        return Ok(None);
    };

    crate::debug!("{line:?}");

    let (command, args) = line
        .split_once(' ')
        .map(|(command, args)| (command, args.to_string()))
        .unwrap_or((line, String::new()));
    let command = command.to_uppercase();

    let command = match command.as_str() {
        "CDUP" => protocol::ControlCommand::Cdup,
        "CWD" => protocol::ControlCommand::Cwd(args),
        "DELE" => protocol::ControlCommand::Dele(args),
        "EPSV" => protocol::ControlCommand::Epsv,
        "FEAT" => protocol::ControlCommand::Feat,
        "LIST" => protocol::ControlCommand::List,
        "NLST" => protocol::ControlCommand::Nlst,
        "OPTS" => protocol::ControlCommand::Opts,
        "PASS" => protocol::ControlCommand::Pass,
        "PASV" => protocol::ControlCommand::Pasv,
        "PWD" => protocol::ControlCommand::Pwd,
        "QUIT" => protocol::ControlCommand::Quit,
        "RETR" => protocol::ControlCommand::Retr(args),
        "SIZE" => protocol::ControlCommand::Size(args),
        "STOR" => protocol::ControlCommand::Stor(args),
        "TYPE" => protocol::ControlCommand::Type,
        "USER" => protocol::ControlCommand::User,
        cmd => {
            crate::warn!("command {cmd:?} not implemented");
            return Ok(None);
        }
    };

    Ok(Some(command))
}

fn data_command(
    channel: &channel::Channel,
    client: &mut net::TcpStream,
    data_frontend: &net::TcpListener,
    cmd: &protocol::DataCommand,
) -> Result<(), api::Error> {
    let mut backend = channel.connect(&super::SERVICE)?;
    protocol::BackendMode::Data.send(&mut backend)?;

    cmd.send(&mut backend)?;

    let is_upload = match cmd {
        protocol::DataCommand::List(_) | protocol::DataCommand::Nlst(_) => {
            client.write_all("150 Here comes the directory listing\r\n".as_bytes())?;
            false
        }
        protocol::DataCommand::Retr(_) => {
            client
                .write_all("125 Data connection already open; transfer starting\r\n".as_bytes())?;
            false
        }
        protocol::DataCommand::Stor(_) => {
            client
                .write_all("125 Data connection already open; transfer starting\r\n".as_bytes())?;
            true
        }
    };

    client.flush()?;

    crate::debug!("accepting data on {}", data_frontend.local_addr()?);
    let (data_client, _) = data_frontend.accept()?;
    crate::debug!("data accepted");

    let res = if is_upload {
        let mut data_client_read = io::BufReader::new(data_client);
        let res = service::stream_copy(&mut data_client_read, &mut backend, false);
        let _ = backend.flush();
        res
    } else {
        let mut data_client_write = io::BufWriter::new(data_client);
        let res = service::stream_copy(&mut backend, &mut data_client_write, false);
        let _ = data_client_write.flush();
        res
    };

    match res {
        Err(e) => {
            crate::warn!("data command {cmd:?} failed: {e}");
            client.write_all(b"426 Connection closed; transfer aborted\r\n")?;
        }
        Ok(()) => {
            client.write_all(b"226 Closing data connection\r\n")?;
        }
    }

    client.flush()?;

    Ok(())
}

pub fn tcp_handler<'a>(
    server: &frontend::FrontendTcpServer,
    _scope: &'a thread::Scope<'a, '_>,
    mut client: net::TcpStream,
    channel: &'a channel::Channel,
) -> Result<(), api::Error> {
    let data_frontend = net::TcpListener::bind((server.ip, 0))?;
    let data_frontend_bind_port = data_frontend.local_addr().unwrap().port();

    let client_read = client.try_clone()?;
    let mut client_read = io::BufReader::new(client_read);

    let mut backend = channel.connect(&super::SERVICE)?;
    protocol::BackendMode::Control.send(&mut backend)?;

    client.write_all(b"220 Welcome\r\n")?;
    client.flush()?;

    loop {
        match parse_command(&mut client_read)? {
            None => client.write_all(b"502 Command not implemented\r\n")?,
            Some(command) => {
                command.send(&mut backend)?;
                let resp = protocol::ControlResponse::receive(&mut backend)?;
                crate::trace!("response {resp:?}");
                match resp {
                    protocol::ControlResponse::Ok(c, msg) => {
                        if let Some(msg) = msg {
                            client.write_all(format!("{c} {msg}\r\n").as_bytes())?;
                        } else {
                            client.write_all(format!("{c} ControlCommand okay\r\n").as_bytes())?;
                        }
                    }
                    protocol::ControlResponse::Error(c) => {
                        client.write_all(format!("{c} Error\r\n").as_bytes())?;
                    }
                    protocol::ControlResponse::Data(cmd) => {
                        data_command(channel, &mut client, &data_frontend, &cmd)?;
                    }
                    protocol::ControlResponse::Quit => {
                        return Ok(());
                    }
                    protocol::ControlResponse::Feat => {
                        client.write_all(b"211-Features:\r\n")?;
                        client.write_all(b" EPRT\r\n")?;
                        client.write_all(b" EPSV\r\n")?;
                        client.write_all(b" PASV\r\n")?;
                        client.write_all(b" REST STREAM\r\n")?;
                        client.write_all(b" SIZE\r\n")?;
                        client.write_all(b" TVFS\r\n")?;
                        client.write_all(b" UTF8\r\n")?;
                        client.write_all(b"211 End\r\n")?;
                    }
                    protocol::ControlResponse::Pasv => match server.ip {
                        net::IpAddr::V4(ip) => {
                            let ip = ip.to_bits().to_be_bytes();
                            let port = data_frontend_bind_port.to_be_bytes();
                            client.write_all(
                                format!(
                                    "227 Entering Passive Mode ({},{},{},{},{},{})\r\n",
                                    ip[0], ip[1], ip[2], ip[3], port[0], port[1]
                                )
                                .as_bytes(),
                            )?;
                        }
                        net::IpAddr::V6(_) => {
                            client.write_all(b"425 Can't open data connection\r\n")?;
                        }
                    },
                    protocol::ControlResponse::Epsv => {
                        client.write_all(format!("229 Entering Extended Passive Mode (|||{data_frontend_bind_port}|)\r\n").as_bytes())?;
                    }
                }
            }
        }
        client.flush()?;
    }
}
