//! OAuth's RS256 check through the TLS ring provider (oauth.md, 2.2; tls.md, 4).

use alloc::boxed::Box;

use rustls::{SignatureScheme, crypto::ring};
use skein_lib::Writer;

/// An RS256 signature check by an RSA key of 2048 to 8192 bits.
#[must_use]
pub fn verify_rs256(modulus: &[u8], exponent: &[u8], message: &[u8], signature: &[u8]) -> bool {
    if modulus.is_empty() || modulus.len() > 1024 || exponent.is_empty() {
        return false;
    }
    let Some(key) = public_key(modulus, exponent) else {
        return false;
    };
    let provider = ring::default_provider();
    for (scheme, algorithms) in provider.signature_verification_algorithms.mapping {
        if *scheme == SignatureScheme::RSA_PKCS1_SHA256 {
            for algorithm in *algorithms {
                if algorithm.verify_signature(&key, message, signature).is_ok() {
                    return true;
                }
            }
        }
    }
    false
}

fn public_key(modulus: &[u8], exponent: &[u8]) -> Option<Box<[u8]>> {
    let n = unsigned(modulus);
    let e = unsigned(exponent);
    let n_len = integer_len(n)?;
    let e_len = integer_len(e)?;
    let contents = element_len(n_len)?.checked_add(element_len(e_len)?)?;
    let mut writer = Writer::new(element_len(contents)?);
    header(&mut writer, 0x30, contents);
    integer(&mut writer, n, n_len);
    integer(&mut writer, e, e_len);
    Some(writer.finish())
}

// DER INTEGER is signed: remove redundant zeroes, then protect a high bit.
fn unsigned(bytes: &[u8]) -> &[u8] {
    for (offset, byte) in bytes.iter().enumerate() {
        if *byte != 0 {
            return bytes.get(offset..).expect("offset belongs to bytes");
        }
    }
    bytes.get(..1).expect("nonempty input")
}

fn integer_len(bytes: &[u8]) -> Option<usize> {
    let first = bytes.first()?;
    bytes.len().checked_add(usize::from(first & 0x80 != 0))
}

fn length_bytes(len: usize) -> usize {
    if len < 128 {
        return 0;
    }
    let bits = usize::BITS.checked_sub(len.leading_zeros()).expect("bit count");
    usize::try_from(bits.div_ceil(8)).expect("usize's byte count")
}

fn element_len(len: usize) -> Option<usize> {
    len.checked_add(2)?.checked_add(length_bytes(len))
}

fn header(writer: &mut Writer, tag: u8, len: usize) {
    writer.put(&[tag]).expect("measured tag");
    let count = length_bytes(len);
    if count == 0 {
        writer.put(&[u8::try_from(len).expect("short DER length")]).expect("measured length");
    } else {
        let marker = 0x80 | u8::try_from(count).expect("usize byte count fits u8");
        writer.put(&[marker]).expect("measured length marker");
        let bytes = len.to_be_bytes();
        let start = bytes.len().checked_sub(count).expect("length fits usize");
        writer.put(bytes.get(start..).expect("length bytes")).expect("measured long length");
    }
}

fn integer(writer: &mut Writer, bytes: &[u8], len: usize) {
    header(writer, 0x02, len);
    if len > bytes.len() {
        writer.put(&[0]).expect("measured sign byte");
    }
    writer.put(bytes).expect("measured integer");
}
