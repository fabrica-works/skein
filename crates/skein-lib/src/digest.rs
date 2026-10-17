//! Keyless, target-independent state digests (lib.md, 5.1).

use core::hash::{Hash, Hasher};

/// A keyless `Hasher` over SHA-256, alike on every target (lib.md, 5.1).
#[derive(Clone, Debug)]
pub struct StateHasher {
    sha: crate::sha256::Sha256,
}

impl StateHasher {
    /// A hasher that has hashed nothing.
    #[expect(clippy::new_without_default, reason = "step code names no Default (programming-model.md, 10.2)")]
    #[must_use]
    pub const fn new() -> StateHasher {
        StateHasher { sha: crate::sha256::Sha256::new() }
    }

    /// Ends it: the SHA-256 of every byte written, in order.
    #[must_use]
    pub fn digest(self) -> [u8; 32] {
        self.sha.finish()
    }
}

impl Hasher for StateHasher {
    /// The digest's first 8 bytes, big-endian, without ending it.
    fn finish(&self) -> u64 {
        let digest = self.sha.clone().finish();
        u64::from_be_bytes(digest.first_chunk().copied().expect("a digest holds eight bytes"))
    }

    fn write(&mut self, bytes: &[u8]) {
        self.sha.update(bytes);
    }

    fn write_u8(&mut self, n: u8) {
        self.write(&n.to_le_bytes());
    }

    fn write_u16(&mut self, n: u16) {
        self.write(&n.to_le_bytes());
    }

    fn write_u32(&mut self, n: u32) {
        self.write(&n.to_le_bytes());
    }

    fn write_u64(&mut self, n: u64) {
        self.write(&n.to_le_bytes());
    }

    fn write_u128(&mut self, n: u128) {
        self.write(&n.to_le_bytes());
    }

    fn write_i8(&mut self, n: i8) {
        self.write(&n.to_le_bytes());
    }

    fn write_i16(&mut self, n: i16) {
        self.write(&n.to_le_bytes());
    }

    fn write_i32(&mut self, n: i32) {
        self.write(&n.to_le_bytes());
    }

    fn write_i64(&mut self, n: i64) {
        self.write(&n.to_le_bytes());
    }

    fn write_i128(&mut self, n: i128) {
        self.write(&n.to_le_bytes());
    }

    fn write_usize(&mut self, n: usize) {
        self.write_u64(u64::try_from(n).expect("no target's usize passes 64 bits"));
    }

    fn write_isize(&mut self, n: isize) {
        self.write_i64(i64::try_from(n).expect("no target's isize passes 64 bits"));
    }
}

/// `value`'s digest, hashed whole.
#[must_use]
pub fn state_digest<T: Hash + ?Sized>(value: &T) -> [u8; 32] {
    let mut hasher = StateHasher::new();
    value.hash(&mut hasher);
    hasher.digest()
}
