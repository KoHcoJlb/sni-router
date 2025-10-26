use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use tinyvec::ArrayVec;

pub mod http;
pub mod tls;
mod tcp;

pub fn proxy_protocol_prefix(addr: SocketAddr) -> ArrayVec<[u8; 52]> {
    let mut buf = ArrayVec::new();
    buf.extend_from_slice(b"\x0D\x0A\x0D\x0A\x00\x0D\x0A\x51\x55\x49\x54\x0A");
    buf.push(0x21); // Version-Command

    buf.push(if addr.is_ipv4() { 0x11 } else { 0x21 });

    buf.extend_from_slice(&(if addr.is_ipv4() { 6 } else { 18 } * 2u16).to_be_bytes());

    let local =
        if addr.is_ipv4() { Ipv4Addr::LOCALHOST.into() } else { Ipv6Addr::LOCALHOST.into() };
    for addr in [addr.ip(), local] {
        match addr {
            IpAddr::V4(a) => buf.extend_from_slice(&a.octets()),
            IpAddr::V6(a) => buf.extend_from_slice(&a.octets()),
        }
    }

    for port in [addr.port(), 0] {
        buf.extend_from_slice(&port.to_be_bytes());
    }

    buf
}
