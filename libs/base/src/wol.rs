//! Wake-on-LAN: magic packets and where to send them.
//!
//! A magic packet is 6 bytes of `0xFF` followed by the target MAC repeated 16 times, sent over
//! UDP. The sleeping machine's network card only looks at the payload, so any port works;
//! 9 (discard) is the convention.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs, UdpSocket};

/// Peer option holding the MAC address(es) typed by the user, comma or space separated.
pub const MAC_OPTION: &str = "wol-mac";
/// Peer option holding an optional extra destination, `host` or `host:port`: a subnet broadcast
/// address such as `192.168.1.255`, or a public address whose router forwards UDP to the LAN.
pub const TARGET_OPTION: &str = "wol-target";
pub const DEFAULT_PORT: u16 = 9;

pub type Mac = [u8; 6];

/// Accepts `aa:bb:cc:dd:ee:ff`, `aa-bb-cc-dd-ee-ff`, `aabb.ccdd.eeff` and `aabbccddeeff`.
pub fn parse_mac(s: &str) -> Option<Mac> {
    let s = s.trim();
    if s.is_empty()
        || !s
            .chars()
            .all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '-' | '.'))
    {
        return None;
    }
    let hex: Vec<u8> = s.bytes().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 12 {
        return None;
    }
    let mut mac = [0u8; 6];
    for (i, pair) in hex.chunks(2).enumerate() {
        mac[i] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(mac)
}

/// Splits a user-typed list on commas, semicolons and whitespace. Returns the MACs that parsed
/// and the pieces that did not.
pub fn parse_mac_list(s: &str) -> (Vec<Mac>, Vec<String>) {
    let mut macs = Vec::new();
    let mut invalid = Vec::new();
    for part in s
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|p| !p.is_empty())
    {
        match parse_mac(part) {
            Some(mac) => push_unique(&mut macs, mac),
            None => invalid.push(part.to_owned()),
        }
    }
    (macs, invalid)
}

pub fn push_unique(macs: &mut Vec<Mac>, mac: Mac) {
    if !macs.contains(&mac) {
        macs.push(mac);
    }
}

pub fn format_mac(mac: &Mac) -> String {
    mac.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

pub fn magic_packet(mac: &Mac) -> [u8; 102] {
    let mut packet = [0xFFu8; 102];
    for chunk in packet[6..].chunks_mut(6) {
        chunk.copy_from_slice(mac);
    }
    packet
}

/// The subnet-directed broadcast address of an interface, e.g. 192.168.1.255 for
/// 192.168.1.20/24. `None` for /31 and /32 links, which have no broadcast address.
pub fn directed_broadcast(addr: Ipv4Addr, netmask: Ipv4Addr) -> Option<Ipv4Addr> {
    let mask = u32::from(netmask);
    if mask >= 0xFFFF_FFFE {
        return None;
    }
    Some(Ipv4Addr::from(u32::from(addr) | !mask))
}

/// Resolves a `host`, `host:port`, `ip` or `[ipv6]:port` destination; the port defaults to 9.
pub fn resolve_target(target: &str) -> std::io::Result<Vec<SocketAddr>> {
    let target = target.trim();
    if let Ok(ip) = target.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, DEFAULT_PORT)]);
    }
    let addrs: Vec<SocketAddr> = match target.to_socket_addrs() {
        Ok(addrs) => addrs.collect(),
        Err(_) => (target, DEFAULT_PORT).to_socket_addrs()?.collect(),
    };
    if addrs.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{target} has no address"),
        ));
    }
    Ok(addrs)
}

/// A UDP socket allowed to send to broadcast addresses, bound to `local` (port chosen by the OS)
/// so the packet leaves through that interface.
pub fn broadcast_socket(local: IpAddr) -> std::io::Result<UdpSocket> {
    let socket = UdpSocket::bind(SocketAddr::new(local, 0))?;
    socket.set_broadcast(true)?;
    Ok(socket)
}

/// The unspecified address of the same family as `dest`, for binding a socket that reaches it.
pub fn unspecified_for(dest: &SocketAddr) -> IpAddr {
    match dest {
        SocketAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        SocketAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: Mac = [0x00, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e];

    #[test]
    fn parses_common_mac_spellings() {
        for s in [
            "00:1a:2b:3c:4d:5e",
            "00-1A-2B-3C-4D-5E",
            "001a.2b3c.4d5e",
            "001A2B3C4D5E",
            "  00:1a:2b:3c:4d:5e  ",
        ] {
            assert_eq!(parse_mac(s), Some(MAC), "{s}");
        }
    }

    #[test]
    fn rejects_bad_macs() {
        for s in [
            "",
            "00:1a:2b:3c:4d",
            "00:1a:2b:3c:4d:5e:6f",
            "00:1a:2b:3c:4d:zz",
            "00 1a 2b 3c 4d 5e",
        ] {
            assert_eq!(parse_mac(s), None, "{s}");
        }
    }

    #[test]
    fn parses_lists_and_reports_invalid_parts() {
        let (macs, invalid) =
            parse_mac_list("00:1a:2b:3c:4d:5e, 00-1a-2b-3c-4d-5e; nope\n11:22:33:44:55:66");
        assert_eq!(macs, vec![MAC, [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]]);
        assert_eq!(invalid, vec!["nope".to_owned()]);
    }

    #[test]
    fn magic_packet_layout() {
        let p = magic_packet(&MAC);
        assert_eq!(&p[..6], &[0xFF; 6]);
        for i in 0..16 {
            assert_eq!(&p[6 + i * 6..12 + i * 6], &MAC);
        }
    }

    #[test]
    fn formats_mac() {
        assert_eq!(format_mac(&MAC), "00:1a:2b:3c:4d:5e");
    }

    #[test]
    fn directed_broadcast_addresses() {
        let ip = |s: &str| s.parse::<Ipv4Addr>().unwrap();
        assert_eq!(
            directed_broadcast(ip("192.168.1.20"), ip("255.255.255.0")),
            Some(ip("192.168.1.255"))
        );
        assert_eq!(
            directed_broadcast(ip("10.4.7.9"), ip("255.255.240.0")),
            Some(ip("10.4.15.255"))
        );
        assert_eq!(
            directed_broadcast(ip("10.0.0.1"), ip("255.255.255.255")),
            None
        );
        assert_eq!(
            directed_broadcast(ip("10.0.0.1"), ip("255.255.255.254")),
            None
        );
    }

    #[test]
    fn resolves_targets_with_default_port() {
        let one = |s: &str| resolve_target(s).unwrap()[0];
        assert_eq!(one("192.168.1.255"), "192.168.1.255:9".parse().unwrap());
        assert_eq!(one("192.168.1.255:7"), "192.168.1.255:7".parse().unwrap());
        assert_eq!(one("::1"), "[::1]:9".parse().unwrap());
        assert_eq!(one("[::1]:7"), "[::1]:7".parse().unwrap());
        assert_eq!(one("localhost").port(), DEFAULT_PORT);
    }

    #[test]
    fn sends_a_magic_packet_over_loopback() {
        let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
        rx.set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let tx = broadcast_socket("127.0.0.1".parse().unwrap()).unwrap();
        tx.send_to(&magic_packet(&MAC), rx.local_addr().unwrap())
            .unwrap();
        let mut buf = [0u8; 200];
        let (n, _) = rx.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], &magic_packet(&MAC)[..]);
    }
}
