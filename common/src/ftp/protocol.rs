use crate::{api, util};
use std::io;

const ID_MODE_CONTROL: u8 = 0x00;
const ID_MODE_DATA: u8 = 0x01;

pub enum BackendMode {
    Control,
    Data,
}

impl BackendMode {
    #[cfg(feature = "frontend")]
    pub fn send<W>(&self, stream: &mut W) -> Result<(), api::Error>
    where
        W: io::Write,
    {
        let code = match self {
            Self::Control => ID_MODE_CONTROL,
            Self::Data => ID_MODE_DATA,
        };

        let buf = [code; 1];
        stream.write_all(&buf)?;
        stream.flush()?;

        Ok(())
    }

    #[cfg(feature = "backend")]
    pub fn receive<R>(stream: &mut R) -> Result<Self, api::Error>
    where
        R: io::Read,
    {
        let mut buf = [0u8; 1];
        stream.read_exact(&mut buf)?;

        match buf[0] {
            ID_MODE_CONTROL => Ok(Self::Control),
            ID_MODE_DATA => Ok(Self::Data),
            _ => {
                #[cfg(not(feature = "log"))]
                {
                    Err(api::Error::Io(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "",
                    )))
                }

                #[cfg(feature = "log")]
                {
                    Err(api::Error::Io(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid command",
                    )))
                }
            }
        }
    }
}

const ID_CTRL_CMD_CDUP: u8 = 0x00;
const ID_CTRL_CMD_CWD: u8 = 0x01;
const ID_CTRL_CMD_DELE: u8 = 0x02;
const ID_CTRL_CMD_EPSV: u8 = 0x03;
const ID_CTRL_CMD_FEAT: u8 = 0x04;
const ID_CTRL_CMD_LIST: u8 = 0x05;
const ID_CTRL_CMD_NLST: u8 = 0x06;
const ID_CTRL_CMD_OPTS: u8 = 0x07;
const ID_CTRL_CMD_PASS: u8 = 0x08;
const ID_CTRL_CMD_PASV: u8 = 0x09;
const ID_CTRL_CMD_PWD: u8 = 0x0a;
const ID_CTRL_CMD_QUIT: u8 = 0x0b;
const ID_CTRL_CMD_RETR: u8 = 0x0c;
const ID_CTRL_CMD_STOR: u8 = 0x0d;
const ID_CTRL_CMD_SIZE: u8 = 0x0e;
const ID_CTRL_CMD_TYPE: u8 = 0x0f;
const ID_CTRL_CMD_USER: u8 = 0x10;

#[derive(Debug)]
pub enum ControlCommand {
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

impl ControlCommand {
    #[cfg(feature = "frontend")]
    pub fn send<W>(&self, stream: &mut W) -> Result<(), api::Error>
    where
        W: io::Write,
    {
        let code = match self {
            Self::Cdup => ID_CTRL_CMD_CDUP,
            Self::Cwd(_) => ID_CTRL_CMD_CWD,
            Self::Dele(_) => ID_CTRL_CMD_DELE,
            Self::Epsv => ID_CTRL_CMD_EPSV,
            Self::Feat => ID_CTRL_CMD_FEAT,
            Self::List => ID_CTRL_CMD_LIST,
            Self::Nlst => ID_CTRL_CMD_NLST,
            Self::Opts => ID_CTRL_CMD_OPTS,
            Self::Pass => ID_CTRL_CMD_PASS,
            Self::Pasv => ID_CTRL_CMD_PASV,
            Self::Pwd => ID_CTRL_CMD_PWD,
            Self::Quit => ID_CTRL_CMD_QUIT,
            Self::Retr(_) => ID_CTRL_CMD_RETR,
            Self::Stor(_) => ID_CTRL_CMD_STOR,
            Self::Size(_) => ID_CTRL_CMD_SIZE,
            Self::Type => ID_CTRL_CMD_TYPE,
            Self::User => ID_CTRL_CMD_USER,
        };

        let buf = [code; 1];
        stream.write_all(&buf)?;

        match self {
            Self::Dele(s) | Self::Cwd(s) | Self::Retr(s) | Self::Stor(s) | Self::Size(s) => {
                util::serialize_string(stream, s)?;
            }
            Self::Cdup
            | Self::Epsv
            | Self::Feat
            | Self::List
            | Self::Nlst
            | Self::Opts
            | Self::Pass
            | Self::Pasv
            | Self::Pwd
            | Self::Quit
            | Self::Type
            | Self::User => (),
        }

        stream.flush()?;

        Ok(())
    }

    #[cfg(feature = "backend")]
    pub fn receive<R>(stream: &mut R) -> Result<Self, api::Error>
    where
        R: io::Read,
    {
        let mut buf = [0u8; 1];
        stream.read_exact(&mut buf)?;

        let res = match buf[0] {
            ID_CTRL_CMD_CDUP => Self::Cdup,
            ID_CTRL_CMD_CWD => Self::Cwd(util::deserialize_string(stream)?),
            ID_CTRL_CMD_DELE => Self::Dele(util::deserialize_string(stream)?),
            ID_CTRL_CMD_EPSV => Self::Epsv,
            ID_CTRL_CMD_FEAT => Self::Feat,
            ID_CTRL_CMD_LIST => Self::List,
            ID_CTRL_CMD_NLST => Self::Nlst,
            ID_CTRL_CMD_OPTS => Self::Opts,
            ID_CTRL_CMD_PASS => Self::Pass,
            ID_CTRL_CMD_PASV => Self::Pasv,
            ID_CTRL_CMD_PWD => Self::Pwd,
            ID_CTRL_CMD_QUIT => Self::Quit,
            ID_CTRL_CMD_RETR => Self::Retr(util::deserialize_string(stream)?),
            ID_CTRL_CMD_STOR => Self::Stor(util::deserialize_string(stream)?),
            ID_CTRL_CMD_SIZE => Self::Size(util::deserialize_string(stream)?),
            ID_CTRL_CMD_TYPE => Self::Type,
            ID_CTRL_CMD_USER => Self::User,
            v => unimplemented!("unsupported ftp data command {v}"),
        };

        Ok(res)
    }
}

const ID_CTRL_RESP_OK: u8 = 0x00;
const ID_CTRL_RESP_ERROR: u8 = 0x01;
const ID_CTRL_RESP_DATA: u8 = 0x02;
const ID_CTRL_RESP_QUIT: u8 = 0x03;
const ID_CTRL_RESP_FEAT: u8 = 0x04;
const ID_CTRL_RESP_PASV: u8 = 0x05;
const ID_CTRL_RESP_EPSV: u8 = 0x06;

#[derive(Debug)]
pub enum ControlResponse {
    Ok(u16, Option<String>),
    Error(u16),
    Data(DataCommand),
    Quit,
    Feat,
    Pasv,
    Epsv,
}

impl ControlResponse {
    #[cfg(feature = "backend")]
    pub fn send<W>(&self, stream: &mut W) -> Result<(), api::Error>
    where
        W: io::Write,
    {
        let code = match self {
            Self::Ok(_, _) => ID_CTRL_RESP_OK,
            Self::Error(_) => ID_CTRL_RESP_ERROR,
            Self::Data(_) => ID_CTRL_RESP_DATA,
            Self::Quit => ID_CTRL_RESP_QUIT,
            Self::Feat => ID_CTRL_RESP_FEAT,
            Self::Pasv => ID_CTRL_RESP_PASV,
            Self::Epsv => ID_CTRL_RESP_EPSV,
        };

        let buf = [code; 1];
        stream.write_all(&buf)?;

        match self {
            Self::Ok(c, msg) => {
                stream.write_all(&c.to_le_bytes())?;
                match msg {
                    None => util::serialize_string(stream, "")?,
                    Some(msg) => util::serialize_string(stream, msg)?,
                }
            }
            Self::Error(c) => {
                stream.write_all(&c.to_le_bytes())?;
            }
            Self::Data(cmd) => {
                cmd.send(stream)?;
            }
            Self::Quit | Self::Feat | Self::Pasv | Self::Epsv => (),
        }

        stream.flush()?;

        Ok(())
    }

    #[cfg(feature = "frontend")]
    pub fn receive<R>(stream: &mut R) -> Result<Self, api::Error>
    where
        R: io::Read,
    {
        let mut buf = [0u8; 1];
        stream.read_exact(&mut buf)?;

        let res = match buf[0] {
            ID_CTRL_RESP_OK => {
                let mut c = [0u8; 2];
                stream.read_exact(&mut c)?;
                let c = u16::from_le_bytes(c);
                let msg = util::deserialize_string(stream)?;
                if msg.is_empty() {
                    Self::Ok(c, None)
                } else {
                    Self::Ok(c, Some(msg))
                }
            }
            ID_CTRL_RESP_ERROR => {
                let mut c = [0u8; 2];
                stream.read_exact(&mut c)?;
                Self::Error(u16::from_le_bytes(c))
            }
            ID_CTRL_RESP_DATA => {
                let cmd = DataCommand::receive(stream)?;
                Self::Data(cmd)
            }
            ID_CTRL_RESP_QUIT => Self::Quit,
            ID_CTRL_RESP_FEAT => Self::Feat,
            ID_CTRL_RESP_PASV => Self::Pasv,
            ID_CTRL_RESP_EPSV => Self::Epsv,
            v => unimplemented!("unsupported ftp data response {v}"),
        };

        Ok(res)
    }
}

const ID_DATA_CMD_LIST: u8 = 0x00;
const ID_DATA_CMD_NLST: u8 = 0x01;
const ID_DATA_CMD_RETR: u8 = 0x02;
const ID_DATA_CMD_STOR: u8 = 0x03;

#[derive(Debug)]
pub enum DataCommand {
    List(String),
    Nlst(String),
    Retr(String),
    Stor(String),
}

impl DataCommand {
    pub fn send<W>(&self, stream: &mut W) -> Result<(), api::Error>
    where
        W: io::Write,
    {
        let code = match self {
            Self::List(_) => ID_DATA_CMD_LIST,
            Self::Nlst(_) => ID_DATA_CMD_NLST,
            Self::Retr(_) => ID_DATA_CMD_RETR,
            Self::Stor(_) => ID_DATA_CMD_STOR,
        };

        let buf = [code; 1];
        stream.write_all(&buf)?;

        match self {
            Self::List(p) | Self::Nlst(p) | Self::Retr(p) | Self::Stor(p) => {
                util::serialize_string(stream, p)?;
            }
        }

        stream.flush()?;

        Ok(())
    }

    pub fn receive<R>(stream: &mut R) -> Result<Self, api::Error>
    where
        R: io::Read,
    {
        let mut buf = [0u8; 1];
        stream.read_exact(&mut buf)?;

        let path = util::deserialize_string(stream)?;

        let res = match buf[0] {
            ID_DATA_CMD_LIST => Self::List(path),
            ID_DATA_CMD_NLST => Self::Nlst(path),
            ID_DATA_CMD_RETR => Self::Retr(path),
            ID_DATA_CMD_STOR => Self::Stor(path),
            v => unimplemented!("unsupported backend mode {v}"),
        };

        Ok(res)
    }
}
