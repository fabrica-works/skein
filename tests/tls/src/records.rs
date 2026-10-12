//! TLS records as the worlds read and corrupt them (tls.md, 5).

/// The length of a record's body, from its five-byte header.
#[must_use]
pub fn length(header: &[u8]) -> usize {
    usize::from(u16::from_be_bytes([header[3], header[4]]))
}

/// Ciphertext cut into whole records.
#[must_use]
pub fn split(mut ciphertext: &[u8]) -> Vec<&[u8]> {
    let mut records = Vec::new();
    while !ciphertext.is_empty() {
        let end = 5 + length(ciphertext);
        records.push(&ciphertext[..end]);
        ciphertext = &ciphertext[end..];
    }
    records
}

/// Changes the first record's last body byte so it fails to decrypt.
pub fn corrupt_first(ciphertext: &mut [u8]) {
    let last = 4 + length(ciphertext);
    ciphertext[last] ^= 1;
}
