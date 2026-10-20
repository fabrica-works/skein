//! Bounded PEM admission shared by conversion and paste (forge-setup.md, 5.2 and 5.8).
use crate::item;
use skein_lib::{
    Secret, Writer,
    base64::{self, Alphabet},
};

/// One RSA PRIVATE KEY block, within Pasted's input bound and DER's key bound
/// (forge-setup.md, 5.2 and 6.4). The host proves the RSA structure by signing.
pub(super) fn pem(pem: &Secret, key_bytes: u32) -> Option<Secret> {
    if u64::try_from(pem.len()).ok()? > u64::from(key_bytes).checked_mul(2)? {
        return None;
    }
    let mut block = pem.expose();
    for byte in pem.expose() {
        if !whitespace(*byte) {
            break;
        }
        block = block.get(1..)?;
    }
    for byte in pem.expose().iter().rev() {
        if !whitespace(*byte) {
            break;
        }
        block = block.get(..block.len().checked_sub(1)?)?;
    }
    let payload = block.strip_prefix(b"-----BEGIN RSA PRIVATE KEY-----")?;
    let payload = match payload.strip_prefix(b"\r\n") {
        Some(rest) => rest,
        None => payload.strip_prefix(b"\n")?,
    };
    let payload = payload.strip_suffix(b"-----END RSA PRIVATE KEY-----")?;
    if !payload.ends_with(b"\n") {
        return None;
    }
    let bound = usize::try_from(base64::encoded_len(Alphabet::Standard, key_bytes)?).ok()?;
    let mut length = 0_usize;
    let mut cr = false;
    for byte in payload {
        if cr && *byte != b'\n' {
            return None;
        }
        cr = *byte == b'\r';
        if *byte != b'\r' && *byte != b'\n' {
            length = length.checked_add(1)?;
            if length > bound {
                return None;
            }
        }
    }
    let mut text = Writer::new(length);
    for byte in payload {
        if *byte != b'\r' && *byte != b'\n' {
            text.put(&[*byte]).ok()?;
        }
    }
    let text = Secret::new(text.finish());
    let der = Secret::new(base64::decode(Alphabet::Standard, text.expose(), key_bytes).ok()?);
    item::app_key(der.expose(), key_bytes)?;
    Some(der)
}
fn whitespace(byte: u8) -> bool {
    byte == b' ' || (b'\t'..=b'\r').contains(&byte)
}
