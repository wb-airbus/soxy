use std::{
    io::{self, Write},
    net,
};

pub mod backend;
pub mod frontend;
mod protocol;

pub(crate) const VERSION: u8 = 0x05;

pub(crate) fn encode_addr(addr: &net::SocketAddr) -> Result<Vec<u8>, io::Error> {
    let mut data = Vec::with_capacity(192);

    match addr {
        net::SocketAddr::V4(ipv4) => {
            data.write_all(&[0x01; 1])?;
            data.write_all(&ipv4.ip().octets())?;
        }
        net::SocketAddr::V6(ipv6) => {
            data.write_all(&[0x04; 1])?;
            data.write_all(&ipv6.ip().octets())?;
        }
    }
    data.write_all(&addr.port().to_be_bytes())?;

    Ok(data)
}
