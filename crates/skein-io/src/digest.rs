//! SHA-256 content versions for conditional whole-file stores (io.md, section 5).

use skein_lib::sha256;

/// A content digest represented as four big-endian 64-bit words.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Digest(pub [u64; 4]);

impl Digest {
    /// Reads SHA-256's bytes as four big-endian words.
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Digest {
        let mut words = [0; 4];
        for (word, bytes) in words.iter_mut().zip(bytes.as_chunks::<8>().0) {
            *word = u64::from_be_bytes(*bytes);
        }
        Digest(words)
    }
}

/// Digests one file's bytes.
#[must_use]
pub fn digest(bytes: &[u8]) -> Digest {
    Digest::from_bytes(sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::{Digest, digest};

    #[test]
    fn published_sha256_vectors() {
        assert_eq!(
            digest(b""),
            Digest([0xe3b0_c442_98fc_1c14, 0x9afb_f4c8_996f_b924, 0x27ae_41e4_649b_934c, 0xa495_991b_7852_b855])
        );
        assert_eq!(
            digest(b"abc"),
            Digest([0xba78_16bf_8f01_cfea, 0x4141_40de_5dae_2223, 0xb003_61a3_9617_7a9c, 0xb410_ff61_f200_15ad])
        );
    }
}
