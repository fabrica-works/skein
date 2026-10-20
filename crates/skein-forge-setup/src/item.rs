//! Values retained by the owner, and their decoders (forge-setup.md, 4.2 and 6.4).
/// Values kept by the host, never paths in the kit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    AdminToken,
    ApiToken,
    GitToken,
    User,
    WebhookSecret,
}

/// A `User` item's value: exactly 8 bytes, little-endian, never 0.
#[must_use]
pub fn user(bytes: &[u8]) -> Option<core::num::NonZeroU64> {
    core::num::NonZeroU64::new(u64::from_le_bytes(bytes.try_into().ok()?))
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
/// Whether `bytes` decode as `item`'s value.
pub(crate) fn fits(item: Item, bytes: &[u8], token_bytes: u32) -> bool {
    match item {
        Item::User => user(bytes).is_some(),
        Item::AdminToken | Item::ApiToken | Item::GitToken | Item::WebhookSecret => token(bytes, token_bytes).is_some(),
    }
}
