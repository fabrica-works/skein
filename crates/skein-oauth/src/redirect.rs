//! Registration admission, loopback redirects and bounded URL encoding.

use crate::{ClientLimits, Failure, Registration};
use alloc::boxed::Box;
use core::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use core::str::FromStr;
use skein_lib::{Writer, bytes};

/// Validates an owner's registration before an exchange or component starts.
pub fn validate_registration(registration: &Registration, limits: &ClientLimits, sign_in: bool) -> Result<(), Failure> {
    for value in [&registration.authorization_url, &registration.token_endpoint, &registration.redirect_uri] {
        if value.is_empty()
            || value.len() > usize::try_from(limits.uri_bytes).expect("u32 fits")
            || value.contains(&b'#')
        {
            return Err(Failure::Limit);
        }
    }
    if !issuer_endpoint(&registration.authorization_url) || !issuer_endpoint(&registration.token_endpoint) {
        return Err(Failure::Malformed);
    }
    if sign_in {
        if registration.client_secret.is_none() && redirect_address(&registration.redirect_uri).is_err() {
            return Err(Failure::InvalidRedirect);
        }
        if registration.client_secret.is_some() && !registration.redirect_uri.starts_with(b"https://") {
            return Err(Failure::InvalidRedirect);
        }
    }
    if registration.client_id.is_empty()
        || registration.client_id.len() > usize::try_from(limits.document.client_bytes).expect("u32 fits")
        || registration.scope.len() > usize::try_from(limits.scope_bytes).expect("u32 fits")
    {
        return Err(Failure::Limit);
    }
    if let Some(secret) = &registration.client_secret
        && (secret.is_empty() || secret.len() > usize::try_from(limits.document.client_bytes).expect("u32 fits"))
    {
        return Err(Failure::Limit);
    }
    Ok(())
}

// oauth.md, section 1 permits HTTP only when the issuer address itself is
// loopback. Names are not resolved by this sans-IO client: a numeric host
// proves the address, and userinfo or suffixes cannot change the authority.
#[expect(clippy::disallowed_methods, reason = "the OAuth protocol machine validates bounded numeric URI hosts")]
fn issuer_endpoint(uri: &[u8]) -> bool {
    if uri.starts_with(b"https://") {
        return true;
    }
    let Some(rest) = uri.strip_prefix(b"http://") else { return false };
    let mut end = rest.len();
    for separator in [b"/".as_slice(), b"?"] {
        if let Some(at) = bytes::find(rest, separator) {
            end = end.min(at);
        }
    }
    let authority = rest.get(..end).expect("the separator is within the URI");
    if let Some(rest) = authority.strip_prefix(b"[") {
        let Some(end) = bytes::find(rest, b"]") else { return false };
        let host = rest.get(..end).expect("the bracket is within the authority");
        let suffix_at = end.checked_add(1).expect("the bracket is within the bounded URI");
        let suffix = rest.get(suffix_at..).expect("the closing bracket fits");
        let Ok(host) = core::str::from_utf8(host) else { return false };
        match Ipv6Addr::from_str(host) {
            Ok(address) => address.is_loopback() && endpoint_port(suffix),
            Err(_) => false,
        }
    } else {
        let end = match bytes::find(authority, b":") {
            Some(end) => end,
            None => authority.len(),
        };
        let host = authority.get(..end).expect("the port separator fits");
        let suffix = authority.get(end..).expect("the port separator fits");
        let Ok(host) = core::str::from_utf8(host) else { return false };
        match Ipv4Addr::from_str(host) {
            Ok(address) => address.is_loopback() && endpoint_port(suffix),
            Err(_) => false,
        }
    }
}

fn endpoint_port(suffix: &[u8]) -> bool {
    if suffix.is_empty() {
        return true;
    }
    let Some(port) = suffix.strip_prefix(b":") else { return false };
    port_number(port)
}

fn port_number(port: &[u8]) -> bool {
    redirect_port(port).is_some()
}

fn redirect_port(port: &[u8]) -> Option<u16> {
    if port.is_empty() || port.len() > 5 {
        return None;
    }
    let mut number = 0_u32;
    for &digit in port {
        if !digit.is_ascii_digit() {
            return None;
        }
        let tens = number.checked_mul(10_u32)?;
        let value = tens.checked_add(u32::from(digit.wrapping_sub(b'0')))?;
        number = value;
    }
    if number == 0 { None } else { u16::try_from(number).ok() }
}

/// Address the caller binds for a public redirect; `localhost` is never resolved.
/// Admits only exact loopback spellings, an explicit nonzero port and a path
/// (oauth.md, sections 1 and 5); the caller still compares the whole URI exactly.
pub fn redirect_address(uri: &[u8]) -> Result<SocketAddr, Failure> {
    if uri.contains(&b'#') {
        return Err(Failure::InvalidRedirect);
    }
    let Some(rest) = uri.strip_prefix(b"http://") else { return Err(Failure::InvalidRedirect) };
    let Some(path_at) = bytes::find(rest, b"/") else { return Err(Failure::InvalidRedirect) };
    let authority = rest.get(..path_at).expect("the path separator is within the URI");
    let address;
    let port = if let Some(value) = authority.strip_prefix(b"127.0.0.1:") {
        address = IpAddr::V4(Ipv4Addr::LOCALHOST);
        value
    } else if let Some(value) = authority.strip_prefix(b"[::1]:") {
        address = IpAddr::V6(Ipv6Addr::LOCALHOST);
        value
    } else if let Some(value) = authority.strip_prefix(b"localhost:") {
        address = IpAddr::V4(Ipv4Addr::LOCALHOST);
        value
    } else {
        return Err(Failure::InvalidRedirect);
    };
    match redirect_port(port) {
        Some(port) => Ok(SocketAddr::new(address, port)),
        None => Err(Failure::InvalidRedirect),
    }
}

pub(crate) fn authorization_url(
    registration: &Registration,
    state: &[u8],
    challenge: Option<&[u8]>,
    limits: &ClientLimits,
) -> Result<Box<[u8]>, Failure> {
    let fields = [
        Some((b"response_type".as_slice(), b"code".as_slice())),
        Some((b"client_id".as_slice(), registration.client_id.as_ref())),
        Some((b"redirect_uri".as_slice(), registration.redirect_uri.as_ref())),
        Some((b"state".as_slice(), state)),
        if registration.scope.is_empty() { None } else { Some((b"scope".as_slice(), registration.scope.as_ref())) },
        match challenge {
            Some(value) => Some((b"code_challenge".as_slice(), value)),
            None => None,
        },
        match challenge {
            Some(_) => Some((b"code_challenge_method".as_slice(), b"S256".as_slice())),
            None => None,
        },
    ];
    let query = form_fields(&fields, limits.url_bytes)?;
    let prefix = registration.authorization_url.len().checked_add(1).ok_or(Failure::Limit)?;
    let length = prefix.checked_add(query.len()).ok_or(Failure::Limit)?;
    if length > usize::try_from(limits.url_bytes).expect("u32 fits") {
        return Err(Failure::Limit);
    }
    let mut writer = Writer::new(length);
    writer.put(&registration.authorization_url).expect("measured URL");
    let separator = if registration.authorization_url.contains(&b'?') { b"&" } else { b"?" };
    writer.put(separator).expect("measured URL");
    writer.put(&query).expect("measured URL");
    Ok(writer.finish())
}

pub(crate) fn form_fields(fields: &[Option<(&[u8], &[u8])>], cap: u32) -> Result<Box<[u8]>, Failure> {
    let mut length = 0_usize;
    let mut count = 0_usize;
    for field in fields {
        let Some((key, value)) = field else { continue };
        if count != 0 {
            length = length.checked_add(1).ok_or(Failure::Limit)?;
        }
        length = length.checked_add(encoded_len(key)?).ok_or(Failure::Limit)?;
        length = length.checked_add(1).ok_or(Failure::Limit)?;
        length = length.checked_add(encoded_len(value)?).ok_or(Failure::Limit)?;
        count = count.checked_add(1).ok_or(Failure::Limit)?;
    }
    if length > usize::try_from(cap).expect("u32 fits") {
        return Err(Failure::Limit);
    }
    let mut writer = Writer::new(length);
    let mut first = true;
    for field in fields {
        let Some((key, value)) = field else { continue };
        if !first {
            writer.put(b"&").expect("measured fields");
        }
        first = false;
        write_encoded(&mut writer, key);
        writer.put(b"=").expect("measured fields");
        write_encoded(&mut writer, value);
    }
    Ok(writer.finish())
}

fn encoded_len(value: &[u8]) -> Result<usize, Failure> {
    let mut length = 0_usize;
    for &byte in value {
        length = length.checked_add(if plain(byte) { 1 } else { 3 }).ok_or(Failure::Limit)?;
    }
    Ok(length)
}

fn write_encoded(writer: &mut Writer, value: &[u8]) {
    const HEX: &[u8] = b"0123456789ABCDEF";
    for &byte in value {
        if plain(byte) {
            writer.put(&[byte]).expect("measured encoding");
        } else {
            let high = *HEX.get(usize::from(byte >> 4_u32)).expect("hex digit");
            let low = *HEX.get(usize::from(byte & 15_u8)).expect("hex digit");
            writer.put(&[b'%', high, low]).expect("measured encoding");
        }
    }
}

const fn plain(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.' || byte == b'_' || byte == b'~'
}
