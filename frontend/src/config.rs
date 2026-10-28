use common::service;
use std::{
    env, fmt, fs,
    io::{self, Read, Write},
    string,
};

pub enum Error {
    Deserialization(toml::de::Error),
    Io(io::Error),
    Serialization(toml::ser::Error),
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Self::Deserialization(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(e: toml::ser::Error) -> Self {
        Self::Serialization(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Deserialization(e) => write!(f, "deserialization error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Serialization(e) => write!(f, "serialization error: {e}"),
        }
    }
}

fn default_channel() -> String {
    common::VIRTUAL_CHANNEL_DEFAULT_NAME.into()
}

fn default_log_level() -> String {
    #[cfg(debug_assertions)]
    {
        "DEBUG".into()
    }
    #[cfg(not(debug_assertions))]
    {
        "INFO".into()
    }
}

fn default_log_file() -> Option<String> {
    let mut path = env::temp_dir();
    path.push(format!("{}.log", env!("CARGO_CRATE_NAME")));
    path.to_str().map(string::ToString::to_string)
}

const fn default_true() -> bool {
    true
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Log {
    #[serde(default = "default_log_level")]
    level: String,
    #[serde(default = "default_log_file")]
    file: Option<String>,
}

impl Default for Log {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: None,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Service {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Forward {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub destination: String,
}

fn default_services() -> Vec<Service> {
    service::SERVICES
        .iter()
        .filter(|s| !s.internal())
        .map(|s| Service {
            name: s.name().to_string(),
            enabled: true,
            ip: None,
            port: None,
        })
        .collect()
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Config {
    #[serde(default = "default_channel")]
    pub channel: String,
    pub ip: String,
    #[serde(default)]
    pub log: Log,
    #[serde(default = "default_services")]
    pub services: Vec<Service>,
    #[serde(default)]
    pub forward: Option<Vec<Forward>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            channel: default_channel(),
            ip: "127.0.0.1".into(),
            log: Log::default(),
            services: default_services(),
            forward: None,
        }
    }
}

impl Config {
    pub fn read() -> Result<Option<Self>, Error> {
        let mut path = dirs::config_dir()
            .ok_or_else(|| Error::Io(io::Error::other("missing configuration directory")))?;

        path.push(format!("{}.toml", env!("CARGO_CRATE_NAME")));

        common::debug!("try to read configuration file at {:?}", path.display());

        if !path.exists() {
            return Ok(None);
        }

        common::debug!("reading configuration file");

        let mut file = fs::File::options().read(true).write(false).open(path)?;

        let mut data = String::new();
        file.read_to_string(&mut data)?;

        Ok(Some(Self::parse(&data)?))
    }

    pub fn save(&self) -> Result<(), Error> {
        let mut path = dirs::config_dir()
            .ok_or_else(|| Error::Io(io::Error::other("missing configuration directory")))?;

        path.push(format!("{}.toml", env!("CARGO_CRATE_NAME")));

        common::debug!("try to write configuration file at {:?}", path.display());

        let mut file = fs::File::options()
            .read(false)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        Ok(write!(file, "{}", self.to_string(true)?)?)
    }

    pub fn log_level(&self) -> common::Level {
        common::Level::try_from(self.log.level.as_str()).unwrap_or(common::Level::Info)
    }

    pub const fn log_file(&self) -> Option<&String> {
        self.log.file.as_ref()
    }

    fn parse(config: &str) -> Result<Self, Error> {
        Ok(toml::from_str(config)?)
    }

    fn to_string(&self, pretty: bool) -> Result<String, toml::ser::Error> {
        if pretty {
            toml::to_string_pretty(self)
        } else {
            toml::to_string(self)
        }
    }
}
