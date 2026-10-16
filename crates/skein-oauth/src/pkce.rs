//! PKCE S256 for the bounded verifier supplied by the caller.

use crate::DecodeError;
use alloc::boxed::Box;

/// A bounded RFC 7636 S256 challenge, without base64 padding.
pub fn challenge(verifier: &[u8]) -> Result<Box<[u8]>, DecodeError> {
    if !(43..=128).contains(&verifier.len()) {
        return Err(DecodeError::Malformed);
    }
    for &byte in verifier {
        if !(byte.is_ascii_alphanumeric() || b"-._~".contains(&byte)) {
            return Err(DecodeError::Malformed);
        }
    }
    let digest = skein_lib::sha256::digest(verifier);
    let mut encoded = skein_lib::Writer::new(43);
    skein_lib::base64::encode(skein_lib::base64::Alphabet::Url, &digest, &mut encoded)
        .expect("a SHA-256 digest encodes to 43 characters");
    Ok(encoded.finish())
}

#[cfg(test)]
mod tests {
    use super::challenge;
    #[test]
    fn rfc_7636_example_uses_s256_without_padding() {
        let verifier = b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(challenge(verifier).expect("valid").as_ref(), b"E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }
    #[test]
    fn max_length_verifier_spans_three_hash_blocks() {
        assert_eq!(challenge(&[b'A'; 64]).expect("valid").as_ref(), b"1T7aemN8mcx_tWbZbp-hCb8VxHhBCj9etNTE4mzQgfY");
        assert_eq!(challenge(&[b'B'; 128]).expect("valid").as_ref(), b"erqnAab0u42eo4cqMVWX628sz9AzktjRBWCDf2E20Go");
    }
}
