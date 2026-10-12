//! Payload metadata only: this decoder does not authenticate JWT signatures.
//! The owner obtains tokens from its authenticated TLS token endpoint; claims
//! supply caller-selected metadata, never a new authentication decision.
use crate::{ClaimSelector, DecodeError, Json, Limits, common, json};
use alloc::boxed::Box;
use skein_lib::{Duration, Wall, base64, bytes};

#[derive(Clone, PartialEq, Eq, Hash)]
#[expect(missing_debug_implementations, reason = "credential values must never occur in traces")]
pub struct Claims {
    pub metadata: Box<[u8]>,
    pub expires_at: Option<Wall>,
    pub valid: Option<Duration>,
}
/// Reads caller-selected metadata once, at refresh, against an injected wall clock.
/// `valid` is only metadata; the refresh's `expires_in` remains an independent cap.
pub fn read_claims(token: &[u8], selector: &ClaimSelector, wall: Wall, limits: &Limits) -> Result<Claims, DecodeError> {
    common::bearer(token, limits)?;
    common::bounded(&selector.object, limits.string_bytes)?;
    common::bounded(&selector.field, limits.string_bytes)?;
    let first = bytes::find(token, b".").ok_or(DecodeError::Malformed)?;
    let header = token.get(..first).ok_or(DecodeError::Malformed)?;
    let rest = token.get(first.checked_add(1).ok_or(DecodeError::TooLarge)?..).ok_or(DecodeError::Malformed)?;
    let second = bytes::find(rest, b".").ok_or(DecodeError::Malformed)?;
    let payload = rest.get(..second).ok_or(DecodeError::Malformed)?;
    let signature = rest.get(second.checked_add(1).ok_or(DecodeError::TooLarge)?..).ok_or(DecodeError::Malformed)?;
    let _header_len = checked_len(header)?;
    let _signature_len = checked_len(signature)?;
    let decoded = match base64::decode(base64::Alphabet::Url, payload, limits.document_bytes) {
        Ok(bytes) => bytes,
        Err(error) => return Err(refusal(error)),
    };
    let value = Json::from_bytes(&decoded, limits)?;
    let tokens = value.as_tokens();
    let auth = json::value_at(tokens, json::required(tokens, &selector.object)?)?;
    let id = json::text_ref(json::value_at(auth, json::required(auth, &selector.field)?)?)?;
    check_metadata(id, limits)?;
    let expires_at = match json::optional_at(tokens, json::field(tokens, b"exp")?)? {
        Some(value) => {
            let seconds = json::unsigned(value)?;
            Some(Wall::from_nanos(seconds.checked_mul(1_000_000_000).ok_or(DecodeError::TooLarge)?))
        }
        None => None,
    };
    let mut valid = None;
    if let Some(expires_at) = expires_at {
        valid = Some(Duration::from_nanos(expires_at.as_nanos().saturating_sub(wall.as_nanos())));
    }
    Ok(Claims { metadata: bytes::copy_of(id), expires_at, valid })
}
pub(crate) fn check_metadata(id: &[u8], limits: &Limits) -> Result<(), DecodeError> {
    common::bounded(id, limits.token_bytes)?;
    for &byte in id {
        if !(0x21..=0x7e).contains(&byte) {
            return Err(DecodeError::Malformed);
        }
    }
    Ok(())
}
fn checked_len(text: &[u8]) -> Result<u32, DecodeError> {
    if text.is_empty() {
        return Err(DecodeError::Malformed);
    }
    match base64::decoded_len(base64::Alphabet::Url, text) {
        Ok(len) => Ok(len),
        Err(error) => Err(refusal(error)),
    }
}
fn refusal(error: base64::Error) -> DecodeError {
    match error {
        base64::Error::TooLong { max: _ } => DecodeError::TooLarge,
        base64::Error::Symbol { at: _ }
        | base64::Error::Padding { at: _ }
        | base64::Error::Length
        | base64::Error::Bits => DecodeError::Malformed,
    }
}
