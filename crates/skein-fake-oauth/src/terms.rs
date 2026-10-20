//! Issued registrations, carried names and loopback redirects (oauth.md, 5).
use crate::{Config, Field, Limits};
use alloc::boxed::Box;
use skein_lib::{List, bytes};

/// Maximum issued client ids retained in a fake registration.
pub const ISSUED: usize = 4;
/// The issuer’s optional registration and revocation terms (oauth.md, 5).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Terms {
    /// Client id issued by each authorization, with the last repeated.
    pub issued: Box<[Box<[u8]>]>,
    /// Leave the issued client id out of the redirect.
    pub omit_issued: bool,
    /// Endpoint accepting scripted revocations.
    pub revocation_endpoint: Option<Box<[u8]>>,
    /// Accept different ports on the configured loopback address.
    pub varying_port: bool,
    /// Optional signed ID tokens and issuer documents.
    pub openid: Option<crate::OpenId>,
}

pub(super) fn valid(terms: Option<&Terms>, limits: &Limits) -> bool {
    let Some(terms) = terms else { return true };
    if terms.issued.len() > ISSUED {
        return false;
    }
    for id in &terms.issued {
        if id.len() > usize::try_from(limits.document.string_bytes).expect("u32 fits") {
            return false;
        }
    }
    if !crate::openid::valid(terms.openid.as_ref(), limits) {
        return false;
    }
    match &terms.revocation_endpoint {
        Some(url) => url.len() <= usize::try_from(limits.uri_bytes).expect("u32 fits"),
        None => true,
    }
}
pub(super) fn worst_case(limits: &Limits) -> Option<u64> {
    List::<Box<[u8]>>::worst_case(limits.carried)?
        .checked_add(u64::from(limits.carried).checked_mul(u64::from(limits.document.string_bytes))?)?
        .checked_add(u64::try_from(ISSUED).ok()?.checked_mul(
            u64::from(limits.document.string_bytes).checked_add(u64::try_from(size_of::<Box<[u8]>>()).ok()?)?,
        )?)?
        .checked_add(u64::from(limits.uri_bytes))?
        .checked_add(crate::openid::worst_case(limits)?)
}
pub(super) fn latest(config: &Config, count: usize) -> &[u8] {
    let Some(terms) = &config.terms else { return &config.client_id };
    if count == 0 {
        return &config.client_id;
    }
    match terms.issued.get(count.saturating_sub(1).min(terms.issued.len().saturating_sub(1))) {
        Some(id) => id,
        None => &config.client_id,
    }
}
pub(super) fn issued(config: &Config, count: usize) -> Option<Box<[u8]>> {
    let terms = config.terms.as_ref()?;
    if terms.omit_issued || terms.issued.is_empty() {
        return None;
    }
    Some(bytes::copy_of(latest(config, count)))
}
pub(super) fn authorization_client(config: &Config, count: usize, client: Option<&[u8]>) -> bool {
    if client == Some(config.client_id.as_ref()) {
        return true;
    }
    let Some(terms) = &config.terms else { return false };
    for id in terms.issued.iter().take(count) {
        if client == Some(id.as_ref()) {
            return true;
        }
    }
    false
}
pub(super) fn revocation(config: &Config, endpoint: &[u8]) -> bool {
    match &config.terms {
        Some(terms) => terms.revocation_endpoint.as_deref() == Some(endpoint),
        None => false,
    }
}
pub(super) fn observe(fields: &List<Field>, carried: &mut List<Box<[u8]>>, limits: &Limits) -> bool {
    for entry in fields.as_slice() {
        if !oauth_name(&entry.key)
            && (entry.key.len() > usize::try_from(limits.document.string_bytes).expect("u32 fits")
                || carried.push(entry.key.clone()).is_err())
        {
            return false;
        }
    }
    true
}
fn oauth_name(name: &[u8]) -> bool {
    [
        b"response_type".as_slice(),
        b"client_id",
        b"redirect_uri",
        b"scope",
        b"state",
        b"code_challenge",
        b"code_challenge_method",
        b"grant_type",
        b"code",
        b"code_verifier",
        b"client_secret",
        b"refresh_token",
        b"token",
        b"token_type_hint",
    ]
    .contains(&name)
}
pub(super) fn redirect(config: &Config, uri: Option<&[u8]>) -> bool {
    let Some(uri) = uri else { return false };
    if uri.len() > config.redirect_uri.len().saturating_add(5) {
        return false;
    }
    if uri == config.redirect_uri.as_ref() {
        return true;
    }
    let Some(terms) = &config.terms else { return false };
    if !terms.varying_port {
        return false;
    }
    let Some(left) = loopback(&config.redirect_uri) else { return false };
    let Some(right) = loopback(uri) else { return false };
    left == right
}
fn loopback(uri: &[u8]) -> Option<(&[u8], &[u8])> {
    let prefix = if uri.starts_with(b"http://127.0.0.1:") {
        b"http://127.0.0.1:".as_slice()
    } else if uri.starts_with(b"http://[::1]:") {
        b"http://[::1]:".as_slice()
    } else {
        return None;
    };
    let rest = uri.strip_prefix(prefix)?;
    let slash = bytes::find(rest, b"/")?;
    let port = rest.get(..slash)?;
    if port.is_empty() {
        return None;
    }
    for byte in port {
        if !byte.is_ascii_digit() {
            return None;
        }
    }
    Some((prefix, rest.get(slash..)?))
}

pub(super) fn too_long(value: Option<&[u8]>, cap: u32) -> bool {
    match value {
        Some(value) => value.len() > usize::try_from(cap).expect("u32 fits"),
        None => false,
    }
}
