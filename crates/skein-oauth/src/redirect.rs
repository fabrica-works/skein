//! Registration admission, loopback redirects and bounded URL encoding.

use crate::{ClientLimits, Failure, Registration};
use alloc::boxed::Box;
use core::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use core::str::FromStr;
use skein_lib::{Decimal, List, Writer, bytes};

/// An issuer's additions to its client registration (oauth.md, section 1).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Terms {
    pub parameters: Box<[Parameter]>,
    pub openid: Option<OpenId>,
    pub issued_client_id: bool,
    pub varying_port: bool,
    pub resource: Option<Box<[u8]>>,
    pub required_scopes: Option<Box<[u8]>>,
    pub revocation_endpoint: Option<Box<[u8]>>,
    pub sign_in_again: Box<[Box<[u8]>]>,
}

/// One parameter added to the authorization URL, and when it is sent.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Parameter {
    pub name: Box<[u8]>,
    pub value: Box<[u8]>,
    pub sent: Sent,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sent {
    EverySignIn,
    NewRegistration,
}

/// The ID token's issuer and, where configured, its published key set.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct OpenId {
    pub issuer: Box<[u8]>,
    pub key_set: Option<Box<[u8]>>,
}

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
        if registration.client_secret.is_none()
            && parse_redirect(&registration.redirect_uri, varying_port(registration)).is_err()
        {
            return Err(Failure::InvalidRedirect);
        }
        if registration.client_secret.is_some()
            && (varying_port(registration)
                || (!registration.redirect_uri.starts_with(b"https://")
                    && parse_redirect(&registration.redirect_uri, false).is_err()))
        {
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
    validate_terms(registration, limits)?;
    Ok(())
}

// oauth.md, section 1 permits HTTP only when the issuer address itself is
// loopback. Names are not resolved by this sans-IO client: a numeric host
// proves the address, and userinfo or suffixes cannot change the authority.
#[expect(clippy::disallowed_methods, reason = "the OAuth protocol machine validates bounded numeric URI hosts")]
pub(crate) fn issuer_endpoint(uri: &[u8]) -> bool {
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
    let number = parse_port(port)?;
    if number == 0 { None } else { Some(number) }
}

fn parse_port(port: &[u8]) -> Option<u16> {
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
    u16::try_from(number).ok()
}

/// Address the caller binds for a public redirect; `localhost` is never resolved.
/// Admits only exact loopback spellings, an explicit nonzero port and a path
/// (oauth.md, sections 1 and 5); the caller still compares the whole URI exactly.
pub fn redirect_address(uri: &[u8]) -> Result<SocketAddr, Failure> {
    parse_redirect(uri, false)
}

fn parse_redirect(uri: &[u8], varying: bool) -> Result<SocketAddr, Failure> {
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
    } else if let Some(value) = authority.strip_prefix(b"localhost:")
        && !varying
    {
        address = IpAddr::V4(Ipv4Addr::LOCALHOST);
        value
    } else {
        return Err(Failure::InvalidRedirect);
    };
    match if varying { parse_port(port) } else { redirect_port(port) } {
        Some(port) => Ok(SocketAddr::new(address, port)),
        None => Err(Failure::InvalidRedirect),
    }
}

pub(crate) fn authorization_url(
    registration: &Registration,
    state: &[u8],
    challenge: Option<&[u8]>,
    nonce: Option<&[u8]>,
    hints: [Option<&[u8]>; 3],
    limits: &ClientLimits,
) -> Result<Box<[u8]>, Failure> {
    let [held_id, id_token, subject] = hints;
    for (value, cap) in [
        (held_id, limits.document.issued_client_bytes),
        (id_token, limits.document.id_token_bytes),
        (subject, limits.document.subject_bytes),
    ] {
        if let Some(value) = value
            && value.len() > usize::try_from(cap).expect("u32 fits")
        {
            return Err(Failure::Limit);
        }
    }
    let held_id = if issued_id(registration) { held_id } else { None };
    let client_id = held_id.unwrap_or(&registration.client_id);
    let fields = [
        Some((b"response_type".as_slice(), b"code".as_slice())),
        Some((b"client_id".as_slice(), client_id)),
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
    let parameters = match &registration.terms {
        Some(terms) => terms.parameters.as_ref(),
        None => &[],
    };
    let Ok(count) = u32::try_from(parameters.len()) else { return Err(Failure::Limit) };
    let capacity = count.checked_add(11).ok_or(Failure::Limit)?;
    let mut all = List::with_capacity(capacity);
    for field in fields {
        all.push(field).expect("measured URL fields");
    }
    for parameter in parameters {
        if held_id.is_none() || parameter.sent == Sent::EverySignIn {
            all.push(Some((parameter.name.as_ref(), parameter.value.as_ref()))).expect("measured URL fields");
        }
    }
    all.push(resource_field(registration)).expect("measured URL fields");
    all.push(match nonce {
        Some(value) => Some((b"nonce".as_slice(), value)),
        None => None,
    })
    .expect("measured URL fields");
    for (name, value) in [(b"id_token_hint".as_slice(), id_token), (b"login_hint".as_slice(), subject)] {
        if held_id.is_some()
            && let Some(value) = value
        {
            all.push(Some((name, value))).expect("measured URL fields");
        }
    }
    let query = form_fields(all.as_slice(), limits.url_bytes)?;
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

fn varying_port(registration: &Registration) -> bool {
    match &registration.terms {
        Some(terms) => terms.varying_port,
        None => false,
    }
}

pub(crate) fn resource_field(registration: &Registration) -> Option<(&[u8], &[u8])> {
    match registration.terms.as_ref()?.resource.as_deref() {
        Some(value) => Some((b"resource".as_slice(), value)),
        None => None,
    }
}

pub(crate) fn has_openid(registration: &Registration) -> bool {
    match &registration.terms {
        Some(terms) => terms.openid.is_some(),
        None => false,
    }
}

pub(crate) fn sign_in_values(
    registration: &mut Registration,
    nonce: Option<&[u8]>,
    port: Option<u16>,
    limits: &ClientLimits,
) -> Result<(), Failure> {
    let openid = has_openid(registration);
    match (openid, nonce) {
        (true, Some(value)) if (16..=usize::try_from(limits.nonce_bytes).expect("u32 fits")).contains(&value.len()) => {
        }
        (false, None) => {}
        (true, Some(_) | None) | (false, Some(_)) => return Err(Failure::Limit),
    }
    match (varying_port(registration), port) {
        (false, None) => return Ok(()),
        (true, Some(port)) if port != 0 => {
            parse_redirect(&registration.redirect_uri, true)?;
            let rest = registration.redirect_uri.strip_prefix(b"http://").expect("parsed redirect");
            let path_at = bytes::find(rest, b"/").expect("parsed path");
            let authority = rest.get(..path_at).expect("parsed authority");
            let host =
                if authority.starts_with(b"127.0.0.1:") { b"127.0.0.1:".as_slice() } else { b"[::1]:".as_slice() };
            let path = rest.get(path_at..).expect("parsed path");
            let digits = Decimal::of(u64::from(port));
            let prefix = 7_usize.checked_add(host.len()).ok_or(Failure::Limit)?;
            let prefix = prefix.checked_add(digits.as_bytes().len()).ok_or(Failure::Limit)?;
            let length = prefix.checked_add(path.len()).ok_or(Failure::Limit)?;
            if length > usize::try_from(limits.uri_bytes).expect("u32 fits") {
                return Err(Failure::Limit);
            }
            let mut writer = Writer::new(length);
            for part in [b"http://".as_slice(), host, digits.as_bytes(), path] {
                writer.put(part).expect("measured redirect");
            }
            registration.redirect_uri = writer.finish();
        }
        (true, None | Some(_)) | (false, Some(_)) => return Err(Failure::InvalidRedirect),
    }
    Ok(())
}

fn validate_terms(registration: &Registration, limits: &ClientLimits) -> Result<(), Failure> {
    let Some(terms) = &registration.terms else { return Ok(()) };
    let (issuer, key_set) = match &terms.openid {
        Some(openid) => (Some(openid.issuer.as_ref()), openid.key_set.as_deref()),
        None => (None, None),
    };
    for uri in [issuer, key_set, terms.revocation_endpoint.as_deref()].into_iter().flatten() {
        if uri.len() > usize::try_from(limits.uri_bytes).expect("u32 fits") {
            return Err(Failure::Limit);
        }
        if uri.is_empty() || uri.contains(&b'#') || !issuer_endpoint(uri) {
            return Err(Failure::Malformed);
        }
    }
    if issuer.is_some()
        && (limits.keys == 0 || limits.key_set_bytes == 0 || (key_set.is_none() && limits.metadata_bytes == 0))
    {
        return Err(Failure::Limit);
    }
    if issuer.is_some() && !scope_contains(&registration.scope, b"openid") {
        return Err(Failure::Malformed);
    }
    let mut parameter_bytes = 0_usize;
    for parameter in &terms.parameters {
        if parameter.name.is_empty() {
            return Err(Failure::Limit);
        }
        parameter_bytes = parameter_bytes.checked_add(parameter.name.len()).ok_or(Failure::Limit)?;
        parameter_bytes = parameter_bytes.checked_add(parameter.value.len()).ok_or(Failure::Limit)?;
    }
    for name in &terms.sign_in_again {
        if name.is_empty() {
            return Err(Failure::Limit);
        }
        parameter_bytes = parameter_bytes.checked_add(name.len()).ok_or(Failure::Limit)?;
    }
    if parameter_bytes > usize::try_from(limits.parameter_bytes).expect("u32 fits") {
        return Err(Failure::Limit);
    }
    for (value, cap) in
        [(terms.resource.as_deref(), limits.resource_bytes), (terms.required_scopes.as_deref(), limits.scope_bytes)]
    {
        if let Some(value) = value
            && value.len() > usize::try_from(cap).expect("u32 fits")
        {
            return Err(Failure::Limit);
        }
    }
    Ok(())
}

pub(crate) fn scope_contains(scopes: &[u8], wanted: &[u8]) -> bool {
    let mut start = 0;
    for end in 0..=scopes.len() {
        if end == scopes.len() || scopes.get(end) == Some(&b' ') {
            if scopes.get(start..end) == Some(wanted) {
                return true;
            }
            start = end.saturating_add(1);
        }
    }
    false
}

pub(crate) fn issued_id(registration: &Registration) -> bool {
    match &registration.terms {
        Some(terms) => terms.issued_client_id,
        None => false,
    }
}

/// Names required scopes not granted by a record (oauth.md, section 2.2).
#[must_use]
pub fn missing_scopes(registration: &Registration, record: &crate::SavedToken) -> Box<[u8]> {
    let required = match &registration.terms {
        Some(terms) => terms.required_scopes.as_deref().unwrap_or(b""),
        None => b"",
    };
    let granted = record.granted_scopes.as_deref().unwrap_or(&registration.scope);
    let mut writer = Writer::new(0);
    let mut length = 0_usize;
    for writing in [false, true] {
        let mut start = 0;
        let mut first = true;
        for end in 0..=required.len() {
            if end != required.len() && required.get(end) != Some(&b' ') {
                continue;
            }
            let scope = required.get(start..end).expect("scope endpoints fit");
            start = end.saturating_add(1);
            if scope.is_empty() || scope_contains(granted, scope) {
                continue;
            }
            if writing {
                if !first {
                    writer.put(b" ").expect("measured missing scopes");
                }
                writer.put(scope).expect("measured missing scopes");
            } else {
                if !first {
                    length = length.checked_add(1).expect("within required scopes");
                }
                length = length.checked_add(scope.len()).expect("within required scopes");
            }
            first = false;
        }
        if !writing {
            writer = Writer::new(length);
        }
    }
    writer.finish()
}
