//! Access-token metadata is unauthenticated; `verify_id_token` authenticates
//! an `OpenID` ID token and checks its sign-in claims (oauth.md, section 2.2).
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

/// What the exchange expects of its ID token.
pub(crate) struct Expected<'a> {
    pub(crate) issuer: &'a [u8],
    pub(crate) client_id: &'a [u8],
    pub(crate) nonce: &'a [u8],
}

pub(crate) fn verify_id_token(
    token: &[u8],
    keys: &[crate::keys::Key],
    expected: &Expected<'_>,
    wall: Wall,
    skew: Duration,
    limits: &Limits,
) -> Result<Box<[u8]>, crate::Check> {
    use crate::Check;
    let first = bytes::find(token, b".").ok_or(Check::Signature)?;
    let header = token.get(..first).ok_or(Check::Signature)?;
    let rest = token.get(first.checked_add(1).ok_or(Check::Signature)?..).ok_or(Check::Signature)?;
    let second = bytes::find(rest, b".").ok_or(Check::Signature)?;
    let payload = rest.get(..second).ok_or(Check::Signature)?;
    let signature = rest.get(second.checked_add(1).ok_or(Check::Signature)?..).ok_or(Check::Signature)?;
    let input_len = first.checked_add(1).ok_or(Check::Signature)?.checked_add(second).ok_or(Check::Signature)?;
    let input = token.get(..input_len).ok_or(Check::Signature)?;
    let header = base64::decode(base64::Alphabet::Url, header, limits.document_bytes).or(Err(Check::Signature))?;
    let header = Json::from_bytes(&header, limits).or(Err(Check::Signature))?;
    let tokens = header.as_tokens();
    if claim_text(tokens, b"alg").or(Err(Check::Signature))? != b"RS256" {
        return Err(Check::Signature);
    }
    let kid = json::optional_at(tokens, json::field(tokens, b"kid").or(Err(Check::Keys))?).or(Err(Check::Keys))?;
    let kid = match kid {
        Some(value) => Some(json::text_ref(value).or(Err(Check::Keys))?),
        None => None,
    };
    let key = pick_key(kid, keys)?;
    let signature =
        base64::decode(base64::Alphabet::Url, signature, limits.id_token_bytes).or(Err(Check::Signature))?;
    if !skein_tls::verify_rs256(&key.modulus, &key.exponent, input, &signature) {
        return Err(Check::Signature);
    }
    let payload = base64::decode(base64::Alphabet::Url, payload, limits.document_bytes).or(Err(Check::Issuer))?;
    check_claims(&payload, expected, wall, skew, limits)
}

pub(crate) fn pick_key<'k>(
    kid: Option<&[u8]>,
    keys: &'k [crate::keys::Key],
) -> Result<&'k crate::keys::Key, crate::Check> {
    if let Some(kid) = kid {
        for key in keys {
            if key.kid.as_deref() == Some(kid) {
                return Ok(key);
            }
        }
    } else if let [key] = keys {
        return Ok(key);
    }
    Err(crate::Check::Keys)
}

pub(crate) fn check_claims(
    payload: &[u8],
    expected: &Expected<'_>,
    wall: Wall,
    skew: Duration,
    limits: &Limits,
) -> Result<Box<[u8]>, crate::Check> {
    use crate::Check;
    let value = Json::from_bytes(payload, limits).or(Err(Check::Issuer))?;
    let tokens = value.as_tokens();
    if claim_text(tokens, b"iss").or(Err(Check::Issuer))? != expected.issuer {
        return Err(Check::Issuer);
    }
    check_audience(tokens, expected.client_id).or(Err(Check::Audience))?;
    let at = json::required(tokens, b"exp").or(Err(Check::Expiry))?;
    let value = json::value_at(tokens, at).or(Err(Check::Expiry))?;
    let exp = json::unsigned(value).or(Err(Check::Expiry))?;
    let exp = exp.checked_mul(1_000_000_000).ok_or(Check::Expiry)?;
    if exp <= wall.as_nanos().saturating_sub(skew.as_nanos()) {
        return Err(Check::Expiry);
    }
    let nonce = claim_text(tokens, b"nonce").or(Err(Check::Nonce))?;
    if !skein_lib::sha256::equal(nonce, expected.nonce) {
        return Err(Check::Nonce);
    }
    let subject = claim_text(tokens, b"sub").or(Err(Check::Missing))?;
    if subject.is_empty() || subject.len() > usize::try_from(limits.subject_bytes).expect("u32 fits") {
        return Err(Check::Missing);
    }
    Ok(bytes::copy_of(subject))
}

fn claim_text<'a>(tokens: &'a [skein_json::Token], name: &[u8]) -> Result<&'a [u8], DecodeError> {
    json::text_ref(json::value_at(tokens, json::required(tokens, name)?)?)
}

fn check_audience(tokens: &[skein_json::Token], client: &[u8]) -> Result<(), DecodeError> {
    use skein_json::Token;
    let audience = json::value_at(tokens, json::required(tokens, b"aud")?)?;
    if let [Token::String(value)] = audience {
        return if value.as_ref() == client { Ok(()) } else { Err(DecodeError::Malformed) };
    }
    if audience.first() != Some(&Token::ArrayStart) || audience.last() != Some(&Token::ArrayEnd) {
        return Err(DecodeError::WrongType);
    }
    let end = audience.len().checked_sub(1).ok_or(DecodeError::Malformed)?;
    let members = audience.get(1..end).ok_or(DecodeError::Malformed)?;
    let mut found = false;
    for member in members {
        let value = json::text_ref(core::slice::from_ref(member))?;
        found |= value == client;
    }
    if !found || (members.len() > 1 && claim_text(tokens, b"azp")? != client) {
        return Err(DecodeError::Malformed);
    }
    Ok(())
}
