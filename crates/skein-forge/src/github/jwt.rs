//! The App's JSON Web Token bytes (forge-github.md, 8.1).
//! The component signs the input with `skein_tls::sign_rs256`.

use alloc::boxed::Box;
use skein_lib::{Decimal, Duration, Secret, Wall, Writer, base64};

/// The largest assembled token, including its two dots.
pub const JWT_BYTES: usize = 512;
/// GitHub's longest life for a token.
pub const LIFE: Duration = Duration::from_secs(600);

/// The unpadded base64url header and claims, separated by a dot.
/// Times round down to epoch seconds; `Wall` cannot represent an earlier time.
/// Refuses an expiry not after issuance or more than [`LIFE`] later.
#[must_use]
pub fn signing_input(app: u64, issued: Wall, expires: Wall) -> Option<Box<[u8]>> {
    let life = expires.as_nanos().checked_sub(issued.as_nanos())?;
    if life == 0 || life > LIFE.as_nanos() {
        return None;
    }
    let issued = Decimal::of(issued.as_secs());
    let expires = Decimal::of(expires.as_secs());
    let app = Decimal::of(app);
    let parts: [&[u8]; 7] =
        [b"{\"iat\":", issued.as_bytes(), b",\"exp\":", expires.as_bytes(), b",\"iss\":\"", app.as_bytes(), b"\"}"];
    let mut length = 0_usize;
    for part in parts {
        length = length.checked_add(part.len())?;
    }
    let mut claims = Writer::new(length);
    for part in parts {
        claims.put(part).ok()?;
    }
    let claims = claims.finish();
    let header = b"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.";
    let encoded = base64::encoded_len(base64::Alphabet::Url, u32::try_from(claims.len()).ok()?)?;
    let mut out = Writer::new(header.len().checked_add(usize::try_from(encoded).ok()?)?);
    out.put(header).ok()?;
    base64::encode(base64::Alphabet::Url, &claims, &mut out).ok()?;
    Some(out.finish())
}

/// The signing input, a dot, and the signature's unpadded base64url.
/// Refuses a result past [`JWT_BYTES`] before allocating it.
#[must_use]
pub fn token(input: &[u8], signature: &[u8; 256]) -> Option<Secret> {
    let encoded = base64::encoded_len(base64::Alphabet::Url, 256)?;
    let length = input.len().checked_add(1)?.checked_add(usize::try_from(encoded).ok()?)?;
    if length > JWT_BYTES {
        return None;
    }
    let mut out = Writer::new(length);
    out.put(input).ok()?;
    out.put(b".").ok()?;
    base64::encode(base64::Alphabet::Url, signature, &mut out).ok()?;
    Some(Secret::new(out.finish()))
}
