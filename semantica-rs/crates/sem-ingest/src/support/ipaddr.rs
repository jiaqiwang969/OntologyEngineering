//! Python 3.11.14 `ipaddress` semantics for the address classes the SSRF guards use:
//! parsing (`ip_address`) and the `is_private` / `is_loopback` / `is_link_local` /
//! `is_reserved` / `is_multicast` / `is_unspecified` properties, with the exact network
//! tables of the reference interpreter (IPv4-mapped IPv6 addresses delegate to IPv4).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use sem_utils::SemanticaError;

/// `ipaddress.ip_address(text)`: an IPv4 or IPv6 address (`ValueError` otherwise). A scope id
/// (`fe80::1%eth0`) is accepted and dropped, as the classification ignores it.
pub fn ip_address(text: &str) -> Result<IpAddr, SemanticaError> {
    if let Some(v4) = parse_ipv4(text) {
        return Ok(IpAddr::V4(v4));
    }
    let (addr, scope) = match text.split_once('%') {
        Some((a, s)) => (a, Some(s)),
        None => (text, None),
    };
    let scope_ok = scope.is_none_or(|s| !s.is_empty() && !s.contains('%'));
    if scope_ok
        && !addr.is_empty()
        && let Ok(v6) = addr.parse::<Ipv6Addr>()
    {
        return Ok(IpAddr::V6(v6));
    }
    Err(SemanticaError::Value(format!(
        "{} does not appear to be an IPv4 or IPv6 address",
        sem_utils::pycompat::fmt::str_repr(text)
    )))
}

fn parse_ipv4(text: &str) -> Option<Ipv4Addr> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut octets = [0u8; 4];
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() || part.len() > 3 || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        if part.len() > 1 && part.starts_with('0') {
            return None;
        }
        let v: u32 = part.parse().ok()?;
        if v > 255 {
            return None;
        }
        octets[i] = v as u8;
    }
    Some(Ipv4Addr::from(octets))
}

fn v4_in(addr: Ipv4Addr, net: [u8; 4], prefix: u32) -> bool {
    let a = u32::from(addr);
    let n = u32::from(Ipv4Addr::from(net));
    if prefix == 0 {
        return true;
    }
    let mask = u32::MAX << (32 - prefix);
    (a & mask) == (n & mask)
}

fn v6_in(addr: Ipv6Addr, net: &str, prefix: u32) -> bool {
    let a = u128::from(addr);
    let n = u128::from(net.parse::<Ipv6Addr>().expect("valid network"));
    if prefix == 0 {
        return true;
    }
    let mask = u128::MAX << (128 - prefix);
    (a & mask) == (n & mask)
}

const V4_PRIVATE: &[([u8; 4], u32)] = &[
    ([0, 0, 0, 0], 8),
    ([10, 0, 0, 0], 8),
    ([127, 0, 0, 0], 8),
    ([169, 254, 0, 0], 16),
    ([172, 16, 0, 0], 12),
    ([192, 0, 0, 0], 24),
    ([192, 0, 0, 170], 31),
    ([192, 0, 2, 0], 24),
    ([192, 168, 0, 0], 16),
    ([198, 18, 0, 0], 15),
    ([198, 51, 100, 0], 24),
    ([203, 0, 113, 0], 24),
    ([240, 0, 0, 0], 4),
    ([255, 255, 255, 255], 32),
];

const V4_PRIVATE_EXCEPTIONS: &[([u8; 4], u32)] = &[([192, 0, 0, 9], 32), ([192, 0, 0, 10], 32)];

const V6_PRIVATE: &[(&str, u32)] = &[
    ("::1", 128),
    ("::", 128),
    ("::ffff:0:0", 96),
    ("64:ff9b:1::", 48),
    ("100::", 64),
    ("2001::", 23),
    ("2001:db8::", 32),
    ("2002::", 16),
    ("3fff::", 20),
    ("fc00::", 7),
    ("fe80::", 10),
];

const V6_PRIVATE_EXCEPTIONS: &[(&str, u32)] = &[
    ("2001:1::1", 128),
    ("2001:1::2", 128),
    ("2001:3::", 32),
    ("2001:4:112::", 48),
    ("2001:20::", 28),
    ("2001:30::", 28),
];

const V6_RESERVED: &[(&str, u32)] = &[
    ("::", 8),
    ("100::", 8),
    ("200::", 7),
    ("400::", 6),
    ("800::", 5),
    ("1000::", 4),
    ("4000::", 3),
    ("6000::", 3),
    ("8000::", 3),
    ("a000::", 3),
    ("c000::", 3),
    ("e000::", 4),
    ("f000::", 5),
    ("f800::", 6),
    ("fe00::", 9),
];

/// `IPv6Address.ipv4_mapped` (`::ffff:a.b.c.d`).
pub fn ipv4_mapped(addr: Ipv6Addr) -> Option<Ipv4Addr> {
    let segs = addr.segments();
    if segs[..5] == [0, 0, 0, 0, 0] && segs[5] == 0xffff {
        let o = addr.octets();
        Some(Ipv4Addr::new(o[12], o[13], o[14], o[15]))
    } else {
        None
    }
}

fn v4_is_private(a: Ipv4Addr) -> bool {
    V4_PRIVATE.iter().any(|(n, p)| v4_in(a, *n, *p))
        && V4_PRIVATE_EXCEPTIONS.iter().all(|(n, p)| !v4_in(a, *n, *p))
}

/// `ip.is_private`.
pub fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => v4_is_private(a),
        IpAddr::V6(a) => {
            if let Some(m) = ipv4_mapped(a) {
                return v4_is_private(m);
            }
            V6_PRIVATE.iter().any(|(n, p)| v6_in(a, n, *p))
                && V6_PRIVATE_EXCEPTIONS.iter().all(|(n, p)| !v6_in(a, n, *p))
        }
    }
}

/// `ip.is_loopback`.
pub fn is_loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => v4_in(a, [127, 0, 0, 0], 8),
        IpAddr::V6(a) => match ipv4_mapped(a) {
            Some(m) => v4_in(m, [127, 0, 0, 0], 8),
            None => u128::from(a) == 1,
        },
    }
}

/// `ip.is_link_local`.
pub fn is_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => v4_in(a, [169, 254, 0, 0], 16),
        IpAddr::V6(a) => match ipv4_mapped(a) {
            Some(m) => v4_in(m, [169, 254, 0, 0], 16),
            None => v6_in(a, "fe80::", 10),
        },
    }
}

/// `ip.is_reserved`.
pub fn is_reserved(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => v4_in(a, [240, 0, 0, 0], 4),
        IpAddr::V6(a) => match ipv4_mapped(a) {
            Some(m) => v4_in(m, [240, 0, 0, 0], 4),
            None => V6_RESERVED.iter().any(|(n, p)| v6_in(a, n, *p)),
        },
    }
}

/// `ip.is_multicast`.
pub fn is_multicast(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => v4_in(a, [224, 0, 0, 0], 4),
        IpAddr::V6(a) => match ipv4_mapped(a) {
            Some(m) => v4_in(m, [224, 0, 0, 0], 4),
            None => v6_in(a, "ff00::", 8),
        },
    }
}

/// `ip.is_unspecified`.
pub fn is_unspecified(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => u32::from(a) == 0,
        IpAddr::V6(a) => match ipv4_mapped(a) {
            Some(m) => u32::from(m) == 0,
            None => u128::from(a) == 0,
        },
    }
}

/// `str(ip)`: Python's canonical text form (compressed IPv6, mapped IPv4 in dotted form).
pub fn ip_to_string(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(a) => a.to_string(),
        IpAddr::V6(a) => {
            if let Some(m) = ipv4_mapped(a) {
                // Python 3.11.14 prints IPv4-mapped addresses as ::ffff:a.b.c.d.
                return format!("::ffff:{m}");
            }
            a.to_string()
        }
    }
}

/// `ip in network` for a CIDR given as text (`"10.0.0.0/8"`, `"fc00::/7"`).
pub fn in_network(ip: IpAddr, cidr: &str) -> bool {
    let (net, prefix) = match cidr.split_once('/') {
        Some((n, p)) => (n, p.parse::<u32>().unwrap_or(0)),
        None => (cidr, 0),
    };
    match (ip, ip_address(net)) {
        (IpAddr::V4(a), Ok(IpAddr::V4(n))) => v4_in(a, n.octets(), prefix),
        (IpAddr::V6(a), Ok(IpAddr::V6(_))) => v6_in(a, net, prefix),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        ip_address(s).unwrap()
    }

    #[test]
    fn classification() {
        assert!(is_private(ip("10.0.0.5")));
        assert!(is_private(ip("198.18.0.5")));
        assert!(!is_private(ip("93.184.216.34")));
        assert!(!is_private(ip("100.64.0.1")));
        assert!(is_private(ip("64:ff9b:1::1")));
        assert!(!is_private(ip("64:ff9b::8c52:7003")));
        assert!(is_reserved(ip("64:ff9b::8c52:7003")));
        assert!(is_private(ip("2001:db8::1")));
        assert!(!is_private(ip("2001:4860:4860::8888")));
        assert!(is_loopback(ip("::1")));
        assert!(is_link_local(ip("fe80::1")));
        assert!(is_private(ip("::ffff:127.0.0.1")));
        assert!(ip_address("01.2.3.4").is_err());
        assert!(ip_address("fe80::1%eth0").is_ok());
    }
}
