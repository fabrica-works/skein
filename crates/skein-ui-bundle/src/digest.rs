use std::collections::BTreeMap;
use std::fmt;

/// The directory name: sixteen lowercase hexadecimal ASCII bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Digest(pub [u8; 16]);

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{}", char::from(byte))?;
        }
        Ok(())
    }
}

pub(crate) fn of(files: &BTreeMap<String, Vec<u8>>) -> Digest {
    let mut encoded = Vec::new();
    for (path, bytes) in files {
        encoded.extend_from_slice(path.as_bytes());
        encoded.push(0);
        encoded.extend_from_slice(&u64::try_from(bytes.len()).expect("file length fits u64").to_le_bytes());
        encoded.extend_from_slice(bytes);
    }
    let hash = crate::sha256::digest(&encoded);
    let mut name = [0; 16];
    for (pair, byte) in name.chunks_exact_mut(2).zip(hash) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        pair[0] = HEX[usize::from(byte >> 4)];
        pair[1] = HEX[usize::from(byte & 15)];
    }
    Digest(name)
}
