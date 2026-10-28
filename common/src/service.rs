#[cfg(feature = "frontend")]
use crate::frontend;
use crate::{api, rdp};

#[cfg(feature = "service-clipboard")]
use crate::clipboard;
#[cfg(feature = "service-command")]
use crate::command;
#[cfg(feature = "service-forward")]
use crate::forward;
#[cfg(feature = "service-ftp")]
use crate::ftp;
#[cfg(feature = "service-input")]
use crate::input;
#[cfg(feature = "service-socks5")]
use crate::socks5;
#[cfg(feature = "service-stage0")]
use crate::stage0;

use std::{
    fmt,
    io::{self, Write},
    net::{self, TcpStream},
    thread,
};

pub(crate) fn stream_copy<R, W>(from: &mut R, to: &mut W, flush: bool) -> Result<(), io::Error>
where
    R: io::Read,
    W: io::Write,
{
    let mut buf = vec![0u8; 10 * api::Chunk::max_payload_length()];

    loop {
        let read = from.read(&mut buf)?;
        if read == 0 {
            to.flush()?;
            return Ok(());
        }
        to.write_all(&buf[..read])?;
        if flush {
            to.flush()?;
        }
        thread::yield_now();
    }
}

pub(crate) fn double_stream_copy(
    #[cfg(feature = "log")] service_kind: Kind,
    #[cfg(not(feature = "log"))] _service_kind: Kind,
    #[cfg(feature = "log")] service: &Service,
    #[cfg(not(feature = "log"))] _service: &Service,
    rdp_stream: rdp::RdpStream<'_>,
    tcp_stream: TcpStream,
    flush: bool,
) -> Result<(), io::Error> {
    #[cfg(feature = "log")]
    let client_id = rdp_stream.client_id();

    let (mut rdp_stream_read, mut rdp_stream_write) = rdp_stream.split();

    let tcp_stream2 = tcp_stream.try_clone()?;

    thread::scope(move |scope| {
        let thread = thread::Builder::new();
        #[cfg(feature = "log")]
        let thread = thread.name(format!(
            "{service_kind} {service} {client_id:x} stream copy"
        ));
        thread
            .spawn_scoped(scope, move || {
                let mut tcp_stream2 = io::BufWriter::new(tcp_stream2);
                if let Err(e) = stream_copy(&mut rdp_stream_read, &mut tcp_stream2, flush) {
                    crate::debug!("error: {e}");
                } else {
                    crate::debug!("stopped");
                }
                let _ = rdp_stream_read;
                let _ = tcp_stream2.flush();
                if let Ok(tcp_stream2) = tcp_stream2.into_inner() {
                    let _ = tcp_stream2.shutdown(net::Shutdown::Both);
                }
            })
            .unwrap();

        let mut tcp_stream = io::BufReader::new(tcp_stream);
        if let Err(e) = stream_copy(&mut tcp_stream, &mut rdp_stream_write, flush) {
            crate::debug!("error: {e}");
        } else {
            crate::debug!("stopped");
        }
        let _ = rdp_stream_write.flush();
        let _ = rdp_stream_write;
        let tcp_stream = tcp_stream.into_inner();
        let _ = tcp_stream.shutdown(net::Shutdown::Both);

        Ok(())
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    #[cfg(feature = "backend")]
    Backend,
    #[cfg(feature = "frontend")]
    Frontend,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            #[cfg(feature = "backend")]
            Self::Backend => write!(f, "backend"),
            #[cfg(feature = "frontend")]
            Self::Frontend => write!(f, "frontend"),
        }
    }
}

#[cfg(feature = "backend")]
type BackendHandler = fn(stream: rdp::RdpStream<'_>) -> Result<(), io::Error>;

#[cfg(feature = "backend")]
pub(crate) struct Backend {
    pub(crate) handler: BackendHandler,
}

pub struct Service {
    pub(crate) internal: bool,
    pub(crate) name: &'static str,
    #[cfg(feature = "frontend")]
    pub(crate) frontend: Option<frontend::Frontend>,
    #[cfg(feature = "backend")]
    pub(crate) backend: Option<Backend>,
}

impl Service {
    pub const fn internal(&self) -> bool {
        self.internal
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }

    #[cfg(feature = "frontend")]
    pub const fn frontend(&self) -> Option<&frontend::Frontend> {
        self.frontend.as_ref()
    }
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(feature = "backend")]
pub(crate) fn lookup_bytes(bytes: &[u8]) -> Result<&'static Service, String> {
    let name = String::from_utf8_lossy(bytes).to_string();
    lookup(&name).ok_or(name)
}

pub fn lookup(name: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|s| s.name == name).copied()
}

// https://patorjk.com/software/taag/#p=display&h=0&v=0&f=Ogre&t=soxy%0A
#[cfg(feature = "frontend")]
pub(crate) const LOGO: &str = r"
 ___   ___  __  __ _   _
/ __| / _ \ \ \/ /| | | |
\__ \| (_) | >  < | |_| |
|___/ \___/ /_/\_\ \__, |
                   |___/";

pub static SERVICES: &[&Service] = &[
    #[cfg(feature = "service-clipboard")]
    &clipboard::SERVICE,
    #[cfg(feature = "service-command")]
    &command::SERVICE,
    #[cfg(feature = "service-forward")]
    &forward::SERVICE,
    #[cfg(feature = "service-ftp")]
    &ftp::SERVICE,
    #[cfg(feature = "service-input")]
    &input::SERVICE,
    #[cfg(feature = "service-socks5")]
    &socks5::SERVICE,
    #[cfg(feature = "service-stage0")]
    &stage0::SERVICE,
];
