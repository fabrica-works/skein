//! A job log's location, split for origin admission (forge-github.md, 10).

use alloc::boxed::Box;
use skein_lib::{Writer, bytes};

use crate::api;

/// The scheme, host (with IPv6 brackets), and port of a location.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Origin {
    pub tls: bool,
    pub host: Box<[u8]>,
    pub port: u16,
}

/// A location's origin and its path and query, with no fragment.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Redirect {
    pub origin: Origin,
    pub target: Box<[u8]>,
}

impl Origin {
    /// Host comparison ignores ASCII case; the scheme and port are exact
    /// (forge-connection.md, 4.5).
    #[must_use]
    pub fn same(&self, other: &Origin) -> bool {
        self.tls == other.tls && self.port == other.port && self.host.eq_ignore_ascii_case(&other.host)
    }
}

/// Parses an absolute HTTP or HTTPS location, defaulting ports to 80 or 443.
/// An absent path becomes `/`, including before a query. Refuses user
/// information, an empty host, a malformed port, or a field past its bound.
pub fn parse(location: &[u8], host_bytes: u32, target_bytes: u32) -> Result<Redirect, api::Error> {
    parse_location(location, host_bytes, target_bytes).ok_or(api::Error::InvalidAnswer)
}

fn parse_location(location: &[u8], host_bytes: u32, target_bytes: u32) -> Option<Redirect> {
    for byte in location {
        if !byte.is_ascii() || *byte <= b' ' || *byte == 127 || *byte == b'\\' {
            return None;
        }
    }
    let scheme_end = bytes::find(location, b"://")?;
    let scheme = location.get(..scheme_end)?;
    let tls = if scheme.eq_ignore_ascii_case(b"https") {
        true
    } else if scheme.eq_ignore_ascii_case(b"http") {
        false
    } else {
        return None;
    };
    let rest = location.get(scheme_end.checked_add(3)?..)?;
    let mut end = rest.len();
    for (at, byte) in rest.iter().enumerate() {
        if b"/?#".contains(byte) {
            end = at;
            break;
        }
    }
    let authority = rest.get(..end)?;
    if authority.contains(&b'@') {
        return None;
    }
    let (host, port) = authority_parts(authority, if tls { 443 } else { 80 })?;
    if host.is_empty() || host.len() > usize::try_from(host_bytes).ok()? {
        return None;
    }
    let tail = rest.get(end..)?;
    let fragment = bytes::find(tail, b"#").unwrap_or(tail.len());
    let path = tail.get(..fragment)?;
    let prefix: &[u8] = if path.starts_with(b"/") { b"" } else { b"/" };
    let length = prefix.len().checked_add(path.len())?;
    if length > usize::try_from(target_bytes).ok()? {
        return None;
    }
    let mut target = Writer::new(length);
    target.put(prefix).ok()?;
    target.put(path).ok()?;
    Some(Redirect { origin: Origin { tls, host: bytes::copy_of(host), port }, target: target.finish() })
}

fn authority_parts(authority: &[u8], default_port: u16) -> Option<(&[u8], u16)> {
    let host_end = if authority.starts_with(b"[") {
        let close = bytes::find(authority, b"]")?;
        let address = authority.get(1..close)?;
        if address.is_empty() || !address.contains(&b':') {
            return None;
        }
        for byte in address {
            if !byte.is_ascii_hexdigit() && !b":.".contains(byte) {
                return None;
            }
        }
        close.checked_add(1)?
    } else {
        bytes::find(authority, b":").unwrap_or(authority.len())
    };
    let host = authority.get(..host_end)?;
    if !host.starts_with(b"[") && (host.contains(&b'[') || host.contains(&b']')) {
        return None;
    }
    let suffix = authority.get(host_end..)?;
    if suffix.is_empty() {
        return Some((host, default_port));
    }
    let digits = suffix.strip_prefix(b":")?;
    if digits.is_empty() {
        return None;
    }
    let mut port = 0_u16;
    for byte in digits {
        if !byte.is_ascii_digit() {
            return None;
        }
        port = port.checked_mul(10)?.checked_add(u16::from(byte.checked_sub(b'0')?))?;
    }
    Some((host, port))
}

/// GET the target as given, without credentials or Range. The caller selects
/// the connection by `redirect.origin`; User-Agent and Accept are `JobLog`'s.
#[must_use]
pub fn request(redirect: &Redirect) -> super::request::Request {
    super::request::Request::Followed { target: bytes::copy_of(&redirect.target) }
}
