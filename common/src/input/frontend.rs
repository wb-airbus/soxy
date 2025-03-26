use crate::{api, input, service};
use std::{
    fs,
    io::{self, BufRead, Write},
    net, thread, time,
};

fn key_lookup(s: &str) -> Option<input::Key> {
    match s.to_uppercase().as_str() {
        "ALT" | "ALTL" | "ALT_L" | "ALT_LEFT" => Some(input::Key::AltLeft),
        "ALTR" | "ALT_R" | "ALT_RIGHT" => Some(input::Key::AltRight),
        "BACKSPACE" => Some(input::Key::Backspace),
        "CONTROL" | "CTRL" => Some(input::Key::Control),
        "DELETE" | "DEL" => Some(input::Key::Delete),
        "DOWN" => Some(input::Key::Down),
        "ESCAPE" | "ESC" => Some(input::Key::Escape),
        "F1" => Some(input::Key::F1),
        "F2" => Some(input::Key::F2),
        "F3" => Some(input::Key::F3),
        "F4" => Some(input::Key::F4),
        "F5" => Some(input::Key::F5),
        "F6" => Some(input::Key::F6),
        "F7" => Some(input::Key::F7),
        "F8" => Some(input::Key::F8),
        "F9" => Some(input::Key::F9),
        "F10" => Some(input::Key::F10),
        "F11" => Some(input::Key::F11),
        "HYPERL" | "HYPER_L" | "HYPER_LEFT" => Some(input::Key::HyperLeft),
        "HYPERR" | "HYPER_R" | "HYPER_RIGHT" => Some(input::Key::HyperRight),
        "LEFT" => Some(input::Key::Left),
        "METAL" | "META_L" | "META_LEFT" => Some(input::Key::MetaLeft),
        "METAR" | "META_R" | "META_RIGHT" => Some(input::Key::MetaRight),
        "RETURN" | "ENTER" => Some(input::Key::Return),
        "RIGHT" => Some(input::Key::Right),
        "SHIFT" => Some(input::Key::Shift),
        "SUPERL" | "SUPER_L" | "SUPER_LEFT" => Some(input::Key::SuperLeft),
        "SUPERR" | "SUPER_R" | "SUPER_RIGHT" => Some(input::Key::SuperRight),
        "TAB" => Some(input::Key::Tab),
        "UP" => Some(input::Key::Up),
        "WIN" | "WINDOWS" => Some(input::Key::Windows),
        _ => None,
    }
}

pub(crate) fn tcp_handler(
    _server: &service::TcpFrontendServer,
    _scope: &thread::Scope,
    stream: net::TcpStream,
    channel: &service::Channel,
) -> Result<(), api::Error> {
    let lstream = stream.try_clone()?;
    let mut client_read = io::BufReader::new(lstream);

    let mut client_write = io::BufWriter::new(stream);

    channel.reset_client()?;

    let mut line = String::new();

    loop {
        let _ = client_read.read_line(&mut line)?;

        let cline = if line.ends_with('\n') {
            line.strip_suffix('\n').unwrap()
        } else {
            &line
        };

        let cline = if cline.ends_with('\r') {
            cline.strip_suffix('\r').unwrap()
        } else {
            cline
        };

        let (command, mut args) = cline
            .split_once(' ')
            .map(|(command, args)| (command, args.to_string()))
            .unwrap_or((cline, String::new()));
        let command = command.to_uppercase();

        crate::debug!("{cline:?}");
        crate::trace!("COMMAND = {command:?}");
        crate::trace!("ARGS = {args:?}");

        match command.as_str() {
            "" => {}
            "EXIT" | "QUIT" => break,
            "PAUSE" | "SLEEP" => match args.parse() {
                Err(e) => {
                    writeln!(client_write, "failed parse delay: {e}")?;
                }
                Ok(delay) => {
                    let delay = time::Duration::from_millis(delay);
                    channel.send_input_action(input::InputAction::Pause(delay))?;
                }
            },
            "DELAY" => match args.parse() {
                Err(e) => {
                    writeln!(client_write, "failed parse delay: {e}")?;
                }
                Ok(delay) => {
                    let delay = time::Duration::from_millis(delay);

                    channel.send_input_setting(input::InputSetting::Keyboard(
                        input::KeyboardSetting::Delay(delay),
                    ))?;
                }
            },
            "WRITE" => {
                channel.send_input_action(input::InputAction::Keyboard(
                    input::KeyboardAction::Write(args),
                ))?;
            }
            "WRITELN" => {
                args.push('\n');
                channel.send_input_action(input::InputAction::Keyboard(
                    input::KeyboardAction::Write(args),
                ))?;
            }
            "KEYDOWN" => match key_lookup(&args) {
                None => {
                    writeln!(client_write, "unknown key")?;
                }
                Some(key) => channel.send_input_action(input::InputAction::Keyboard(
                    input::KeyboardAction::KeyDown(key),
                ))?,
            },
            "KEY" | "KEYPRESS" => match key_lookup(&args) {
                None => {
                    writeln!(client_write, "unknown key")?;
                }
                Some(key) => {
                    channel.send_input_action(input::InputAction::Keyboard(
                        input::KeyboardAction::KeyPress(key),
                    ))?;
                }
            },
            "KEYUP" => match key_lookup(&args) {
                None => {
                    writeln!(client_write, "unknown key")?;
                }
                Some(key) => channel.send_input_action(input::InputAction::Keyboard(
                    input::KeyboardAction::KeyUp(key),
                ))?,
            },
            "CAT" => match fs::File::options().read(true).open(args) {
                Err(e) => {
                    writeln!(client_write, "failed to open file for reading: {e}")?;
                }
                Ok(file) => {
                    let mut file = io::BufReader::new(file);
                    let mut buf = String::new();

                    loop {
                        let read = file.read_line(&mut buf)?;

                        if read == 0 {
                            break;
                        }

                        buf.truncate(read);

                        channel.send_input_action(input::InputAction::Keyboard(
                            input::KeyboardAction::Write(buf.clone()),
                        ))?;

                        buf.clear();
                    }
                }
            },
            _ => writeln!(client_write, "invalid command")?,
        }

        client_write.flush()?;
        line.clear();
    }

    let lstream = client_read.into_inner();
    let _ = lstream.shutdown(net::Shutdown::Both);

    Ok(())
}
