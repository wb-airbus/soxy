use std::{fmt, io};

const ID_CWD: u8 = 0x0;
const ID_DELE: u8 = 0x1;
const ID_LIST: u8 = 0x2;
const ID_NLST: u8 = 0x3;
const ID_RETR: u8 = 0x4;
const ID_SIZE: u8 = 0x5;
const ID_STOR: u8 = 0x6;

#[derive(Debug)]
pub(crate) enum DataCommand {
    Cwd(String),
    Dele(String),
    List(String),
    NLst(String),
    Retr(String),
    Size(String),
    Stor(String),
}

impl DataCommand {
    pub(crate) const fn is_ftp_control(&self) -> bool {
        match self {
            Self::Dele(_) | Self::Cwd(_) | Self::Size(_) => true,
            Self::List(_) | Self::NLst(_) | Self::Retr(_) | Self::Stor(_) => false,
        }
    }

    pub(crate) const fn is_upload(&self) -> bool {
        matches!(self, Self::Stor(_))
    }

    pub(crate) fn send<W>(&self, stream: &mut W) -> Result<(), io::Error>
    where
        W: io::Write,
    {
        let (code, value) = match self {
            Self::Cwd(s) => (ID_CWD, s),
            Self::Dele(s) => (ID_DELE, s),
            Self::List(s) => (ID_LIST, s),
            Self::NLst(s) => (ID_NLST, s),
            Self::Retr(s) => (ID_RETR, s),
            Self::Size(s) => (ID_SIZE, s),
            Self::Stor(s) => (ID_STOR, s),
        };

        let buf = [code; 1];
        stream.write_all(&buf)?;
        let len = value.len() as u64;
        stream.write_all(&len.to_le_bytes())?;
        stream.write_all(value.as_bytes())?;
        stream.flush()
    }

    pub(crate) fn receive<R>(stream: &mut R) -> Result<Self, io::Error>
    where
        R: io::Read,
    {
        let mut buf = [0u8; 1];
        stream.read_exact(&mut buf)?;
        let code = buf[0];

        let mut buf = [0u8; 8];
        stream.read_exact(&mut buf)?;
        let len = u64::from_le_bytes(buf);

        let len = usize::try_from(len)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf)?;
        let value = String::from_utf8_lossy(&buf).to_string();

        match code {
            ID_CWD => Ok(Self::Cwd(value)),
            ID_DELE => Ok(Self::Dele(value)),
            ID_LIST => Ok(Self::List(value)),
            ID_NLST => Ok(Self::NLst(value)),
            ID_RETR => Ok(Self::Retr(value)),
            ID_SIZE => Ok(Self::Size(value)),
            ID_STOR => Ok(Self::Stor(value)),
            v => unimplemented!("unsupported ftp data command {v}"),
        }
    }
}

impl fmt::Display for DataCommand {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Cwd(path) => write!(f, "change directory {path:?}"),
            Self::Dele(path) => write!(f, "delete {path:?}"),
            Self::List(path) => write!(f, "list {path:?}"),
            Self::NLst(path) => write!(f, "nlst {path:?}"),
            Self::Retr(path) => write!(f, "download {path:?}"),
            Self::Size(path) => write!(f, "size {path:?}"),
            Self::Stor(path) => write!(f, "upload {path:?}"),
        }
    }
}

const ID_DATA_TRANSFER_OK: u8 = 0x0;
const ID_CWD_OK: u8 = 0x1;
const ID_SIZE_OK: u8 = 0x2;
const ID_DELETE_OK: u8 = 0x3;
const ID_KO: u8 = 0x4;

#[derive(Debug)]
pub(crate) enum DataReply {
    DataTransferOk,
    CwdOk,
    SizeOk(u64),
    DeleteOk,
    Ko,
}

impl DataReply {
    pub(crate) const fn is_ok(&self) -> bool {
        match self {
            Self::DataTransferOk | Self::CwdOk | Self::SizeOk(_) | Self::DeleteOk => true,
            Self::Ko => false,
        }
    }

    pub(crate) fn send<W>(&self, stream: &mut W) -> Result<(), io::Error>
    where
        W: io::Write,
    {
        let (code, value) = match self {
            Self::DataTransferOk => (ID_DATA_TRANSFER_OK, None),
            Self::CwdOk => (ID_CWD_OK, None),
            Self::SizeOk(size) => (ID_SIZE_OK, Some(size)),
            Self::DeleteOk => (ID_DELETE_OK, None),
            Self::Ko => (ID_KO, None),
        };

        let mut buf = [0u8; 1];
        buf[0] = code;
        stream.write_all(&buf)?;
        if let Some(value) = value {
            stream.write_all(&value.to_le_bytes())?;
        }
        stream.flush()
    }

    pub(crate) fn receive<R>(stream: &mut R) -> Result<Self, io::Error>
    where
        R: io::Read,
    {
        let mut buf = [0u8; 1];
        stream.read_exact(&mut buf)?;
        let code = buf[0];

        match code {
            ID_DATA_TRANSFER_OK => Ok(Self::DataTransferOk),
            ID_CWD_OK => Ok(Self::CwdOk),
            ID_SIZE_OK => {
                let mut buf = [0u8; 8];
                stream.read_exact(&mut buf)?;
                let size = u64::from_le_bytes(buf);
                Ok(Self::SizeOk(size))
            }
            ID_DELETE_OK => Ok(Self::DeleteOk),
            ID_KO => Ok(Self::Ko),
            v => unimplemented!("unsupported ftp data reply {v}"),
        }
    }
}

impl fmt::Display for DataReply {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::DataTransferOk => write!(f, "data transfer ok"),
            Self::CwdOk => write!(f, "change directory ok"),
            Self::SizeOk(size) => write!(f, "size ok ({size})"),
            Self::DeleteOk => write!(f, "delete ok"),
            Self::Ko => write!(f, "KO"),
        }
    }
}
