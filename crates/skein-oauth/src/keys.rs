//! Bounded issuer documents and RSA signing keys (oauth.md, sections 2.2 and 5).
use crate::{Check, ClientLimits, Json, json};
use alloc::boxed::Box;
use skein_json::Token;
use skein_lib::{List, base64, bytes};

#[expect(dead_code, reason = "held keys expose their components for the ID-token verifier")]
pub(crate) struct Key {
    pub(crate) kid: Option<Box<[u8]>>,
    /// The modulus without leading zero bytes.
    pub(crate) modulus: Box<[u8]>,
    pub(crate) exponent: Box<[u8]>,
}

pub(crate) fn issuer_key_set(metadata: &[u8], issuer: &[u8], limits: &ClientLimits) -> Result<Box<[u8]>, Check> {
    let value = document(metadata, limits.metadata_bytes, limits)?;
    let tokens = value.as_tokens();
    if text(tokens, b"issuer")? != issuer {
        return Err(Check::Keys);
    }
    let uri = text(tokens, b"jwks_uri")?;
    if uri.is_empty()
        || uri.len() > usize::try_from(limits.uri_bytes).expect("u32 fits")
        || uri.contains(&b'#')
        || !crate::redirect::issuer_endpoint(uri)
    {
        return Err(Check::Keys);
    }
    Ok(bytes::copy_of(uri))
}

pub(crate) fn fitting_keys(key_set: &[u8], limits: &ClientLimits) -> Result<Box<[Key]>, Check> {
    let value = document(key_set, limits.key_set_bytes, limits)?;
    let tokens = value.as_tokens();
    let array = json::value_at(tokens, json::required(tokens, b"keys").or(Err(Check::Keys))?).or(Err(Check::Keys))?;
    if array.first() != Some(&Token::ArrayStart) {
        return Err(Check::Keys);
    }
    let mut keys = List::with_capacity(limits.keys);
    let mut at = 1_usize;
    for _entry in 0..array.len() {
        if array.get(at) == Some(&Token::ArrayEnd) {
            break;
        }
        let end = json::span(array, at).or(Err(Check::Keys))?;
        let entry = array.get(at..end).ok_or(Check::Keys)?;
        if let Ok(key) = fitting_key(entry, limits) {
            keys.push(key).or(Err(Check::Keys))?;
        }
        at = end;
    }
    if keys.is_empty() { Err(Check::Keys) } else { Ok(keys.into_boxed()) }
}

fn document(input: &[u8], cap: u32, limits: &ClientLimits) -> Result<Json, Check> {
    if input.len() > usize::try_from(cap).expect("u32 fits") {
        return Err(Check::Keys);
    }
    Json::from_bytes(input, &limits.document).or(Err(Check::Keys))
}

fn fitting_key(entry: &[Token], limits: &ClientLimits) -> Result<Key, Check> {
    if text(entry, b"kty")? != b"RSA" {
        return Err(Check::Keys);
    }
    if let Some(value) = optional_text(entry, b"use")?
        && value != b"sig"
    {
        return Err(Check::Keys);
    }
    let modulus =
        base64::decode(base64::Alphabet::Url, text(entry, b"n")?, limits.document.string_bytes).or(Err(Check::Keys))?;
    let mut start = 0_usize;
    for byte in &modulus {
        if *byte != 0 {
            break;
        }
        start = start.checked_add(1).ok_or(Check::Keys)?;
    }
    let modulus = modulus.get(start..).ok_or(Check::Keys)?;
    let first = modulus.first().ok_or(Check::Keys)?;
    let bits = u32::try_from(modulus.len())
        .or(Err(Check::Keys))?
        .checked_mul(8)
        .ok_or(Check::Keys)?
        .checked_sub(first.leading_zeros())
        .ok_or(Check::Keys)?;
    if !(2048..=limits.modulus_bits).contains(&bits) {
        return Err(Check::Keys);
    }
    let exponent =
        base64::decode(base64::Alphabet::Url, text(entry, b"e")?, limits.document.string_bytes).or(Err(Check::Keys))?;
    if exponent.is_empty() {
        return Err(Check::Keys);
    }
    let mut kid = None;
    if let Some(value) = optional_text(entry, b"kid")? {
        kid = Some(bytes::copy_of(value));
    }
    Ok(Key { kid, modulus: bytes::copy_of(modulus), exponent })
}

fn text<'a>(tokens: &'a [Token], name: &[u8]) -> Result<&'a [u8], Check> {
    optional_text(tokens, name)?.ok_or(Check::Keys)
}
fn optional_text<'a>(tokens: &'a [Token], name: &[u8]) -> Result<Option<&'a [u8]>, Check> {
    let field = json::field(tokens, name).or(Err(Check::Keys))?;
    match json::optional_at(tokens, field).or(Err(Check::Keys))? {
        Some(value) => Ok(Some(json::text_ref(value).or(Err(Check::Keys))?)),
        None => Ok(None),
    }
}
