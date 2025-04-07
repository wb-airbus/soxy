use network_interface::NetworkInterfaceConfig;
use std::net;

pub(crate) struct BestAddress {
    pub cidr4: Option<(net::Ipv4Addr, u8)>,
    pub cidr6: Option<(net::Ipv6Addr, u8)>,
}

pub(crate) fn find_best_address() -> Result<BestAddress, network_interface::Error> {
    let interfaces = network_interface::NetworkInterface::show()?;

    let mut best_cidr4 = None;
    let mut best_cidr6 = None;

    for interface in interfaces {
        for addr in interface.addr {
            if addr.ip().is_loopback() || addr.ip().is_multicast() || addr.ip().is_unspecified() {
                continue;
            }

            if let Some(mask) = addr.netmask() {
                match addr.ip() {
                    net::IpAddr::V4(ip) => match best_cidr4 {
                        None => match mask {
                            net::IpAddr::V4(mask) => {
                                best_cidr4 = Some((
                                    ip,
                                    u8::try_from(mask.to_bits().count_ones())
                                        .expect("too large v4 mask"),
                                ));
                            }
                            net::IpAddr::V6(_) => unreachable!(),
                        },
                        Some((_, best_mask)) => {
                            let mask_nb_ones = match mask {
                                net::IpAddr::V4(mask) => u8::try_from(mask.to_bits().count_ones())
                                    .expect("too large v4 mask"),
                                net::IpAddr::V6(_) => unreachable!(),
                            };

                            if mask_nb_ones < best_mask {
                                best_cidr4 = Some((ip, mask_nb_ones));
                            }
                        }
                    },

                    net::IpAddr::V6(ip) => match best_cidr6 {
                        None => match mask {
                            net::IpAddr::V6(mask) => {
                                best_cidr6 = Some((
                                    ip,
                                    u8::try_from(mask.to_bits().count_ones())
                                        .expect("too large v6 mask"),
                                ));
                            }
                            net::IpAddr::V4(_) => unreachable!(),
                        },
                        Some((_, best_mask)) => {
                            let mask_nb_ones = match mask {
                                net::IpAddr::V6(mask) => u8::try_from(mask.to_bits().count_ones())
                                    .expect("too large v6 mask"),
                                net::IpAddr::V4(_) => unreachable!(),
                            };

                            if mask_nb_ones < best_mask {
                                best_cidr6 = Some((ip, mask_nb_ones));
                            }
                        }
                    },
                }
            }
        }
    }

    Ok(BestAddress {
        cidr4: best_cidr4,
        cidr6: best_cidr6,
    })
}
