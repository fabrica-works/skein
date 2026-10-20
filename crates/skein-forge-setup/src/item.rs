//! Values retained by the owner, and their decoders (forge-setup.md, 4.2 and 6.4).
use alloc::boxed::Box;
use core::num::NonZeroU64;
use skein_lib::{List, Secret, Writer};

/// Values kept by the host, never paths in the kit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    AdminToken,
    ApiToken,
    GitToken,
    User,
    WebhookSecret,
    /// The App's nonzero id, little-endian.
    AppId,
    /// The App's lowercase slug.
    AppSlug,
    /// The App's private key, PKCS#1 DER.
    AppKey,
    /// Accepted installation ids, ascending and distinct.
    Accepted,
    /// A pasted PEM, read once and never written.
    Pasted,
}

/// The bounds `fits` asks of each item; a zero fits no nonempty value of its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub token_bytes: u32,
    pub name_bytes: u32,
    pub key_bytes: u32,
    pub installations: u32,
}

/// A `User` item's value: exactly 8 bytes, little-endian, never 0.
#[must_use]
pub fn user(bytes: &[u8]) -> Option<NonZeroU64> {
    NonZeroU64::new(u64::from_le_bytes(bytes.try_into().ok()?))
}
/// A token's or a secret's item: 1 to `token_bytes` bytes, each visible
/// ASCII (0x21 to 0x7e), so it fits a header as it is.
#[must_use]
pub fn token(bytes: &[u8], token_bytes: u32) -> Option<&[u8]> {
    if bytes.is_empty() || bytes.len() > usize::try_from(token_bytes).ok()? {
        return None;
    }
    for byte in bytes {
        if !(0x21..=0x7e).contains(byte) {
            return None;
        }
    }
    Some(bytes)
}
/// 8 bytes little-endian, never 0.
#[must_use]
pub fn app_id(bytes: &[u8]) -> Option<NonZeroU64> {
    user(bytes)
}
/// 1 to `name_bytes` bytes of `a-z`, `0-9` and `-`, the first not `-`:
/// GitHub's slug, safe in a path and a host name.
#[must_use]
pub fn app_slug(bytes: &[u8], name_bytes: u32) -> Option<&[u8]> {
    if bytes.is_empty() || bytes.len() > usize::try_from(name_bytes).ok()? || bytes.first() == Some(&b'-') {
        return None;
    }
    for byte in bytes {
        if !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && *byte != b'-' {
            return None;
        }
    }
    Some(bytes)
}
/// 1 to `key_bytes` bytes containing exactly one DER SEQUENCE with a minimal
/// definite length, in short form or the 0x81 and 0x82 long forms.
/// This checks the envelope's length alone; the host proves the key can sign.
#[must_use]
pub fn app_key(bytes: &[u8], key_bytes: u32) -> Option<&[u8]> {
    if bytes.len() > usize::try_from(key_bytes).ok()? {
        return None;
    }
    let (length, body) = match bytes {
        [0x30, length @ 0..=0x7f, body @ ..] | [0x30, 0x81, length @ 0x80..=0xff, body @ ..] => {
            (usize::from(*length), body)
        }
        [0x30, 0x82, high @ 1..=0xff, low, body @ ..] => (usize::from(u16::from_be_bytes([*high, *low])), body),
        _ => return None,
    };
    if body.len() != length {
        return None;
    }
    Some(bytes)
}
/// 0 to `installations` ids, each 8 bytes little-endian and nonzero,
/// ascending and distinct. The empty value is `Some` of no ids.
#[must_use]
pub fn accepted(bytes: &[u8], installations: u32) -> Option<Box<[u64]>> {
    let chunks = bytes.chunks_exact(8);
    let count = u32::try_from(chunks.len()).ok()?;
    if !chunks.remainder().is_empty() || count > installations {
        return None;
    }
    let mut previous = 0;
    for chunk in chunks {
        let id = app_id(chunk)?.get();
        if id <= previous {
            return None;
        }
        previous = id;
    }
    let mut ids = List::with_capacity(count);
    for chunk in bytes.chunks_exact(8) {
        let id = u64::from_le_bytes(chunk.try_into().ok()?);
        ids.push(id).expect("validated installation count");
    }
    Some(ids.into_boxed())
}
/// A pasted PEM: 1 to twice `key_bytes` bytes, each visible ASCII
/// (0x21 to 0x7e), a space, `\n` or `\r`.
#[must_use]
pub fn pasted(bytes: &[u8], key_bytes: u32) -> Option<&[u8]> {
    let bound = u64::from(key_bytes).checked_mul(2)?;
    if bytes.is_empty() || u64::try_from(bytes.len()).ok()? > bound {
        return None;
    }
    for byte in bytes {
        if !(0x20..=0x7e).contains(byte) && *byte != b'\n' && *byte != b'\r' {
            return None;
        }
    }
    Some(bytes)
}
/// The `Accepted` value of `ids` (taken ascending and distinct, as
/// `accepted` returns them): 8 bytes little-endian each.
#[must_use]
pub fn accepted_item(ids: &[u64]) -> Secret {
    let mut out = Writer::new(ids.len().checked_mul(8).expect("accepted ids fit their item"));
    for id in ids {
        out.put(&id.to_le_bytes()).expect("accepted item capacity");
    }
    Secret::new(out.finish())
}
/// Whether `bytes` decode as `item`'s value within `bounds`.
#[must_use]
pub fn fits(item: Item, bytes: &[u8], bounds: &Bounds) -> bool {
    match item {
        Item::User => user(bytes).is_some(),
        Item::AdminToken | Item::ApiToken | Item::GitToken | Item::WebhookSecret => {
            token(bytes, bounds.token_bytes).is_some()
        }
        Item::AppId => app_id(bytes).is_some(),
        Item::AppSlug => app_slug(bytes, bounds.name_bytes).is_some(),
        Item::AppKey => app_key(bytes, bounds.key_bytes).is_some(),
        Item::Accepted => accepted(bytes, bounds.installations).is_some(),
        Item::Pasted => pasted(bytes, bounds.key_bytes).is_some(),
    }
}
