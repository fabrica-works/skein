//! Reusable bounded bytes (lib.md, 6): a record whose length is learnt as it is put.

#![expect(clippy::disallowed_types, reason = "a buffer is a Vec allocated once, at its final capacity")]

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::hash::{Hash, Hasher};

use crate::Overflow;

/// Bytes whose length is learnt as they are put, reused from one record to the next.
///
/// Its `Debug` shows its length and capacity, never its bytes: a record holding a
/// secret is built in one (codec.md, section 11).
pub struct Buffer {
    bytes: Vec<u8>,
    capacity: u32,
}

impl Buffer {
    /// Allocates `capacity` bytes once; nothing later reallocates.
    #[must_use]
    pub fn with_capacity(capacity: u32) -> Buffer {
        let size = usize::try_from(capacity).expect("a u32 fits in a usize");
        Buffer { bytes: Vec::with_capacity(size), capacity }
    }

    /// The heap `with_capacity(capacity)` allocates, or `None` past a `u64`.
    #[must_use]
    pub fn worst_case(capacity: u32) -> Option<u64> {
        Some(u64::from(capacity))
    }

    #[must_use]
    pub const fn capacity(&self) -> u32 {
        self.capacity
    }

    #[must_use]
    pub fn len(&self) -> u32 {
        u32::try_from(self.bytes.len()).expect("no longer than its capacity")
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// How many more bytes fit.
    #[must_use]
    pub fn room(&self) -> u32 {
        self.capacity.checked_sub(self.len()).expect("no longer than its capacity")
    }

    /// Appends `bytes`, or refuses them whole past the capacity, writing nothing.
    pub fn put(&mut self, bytes: &[u8]) -> Result<(), Overflow> {
        if bytes.len() > usize::try_from(self.room()).expect("a u32 fits in a usize") {
            return Err(Overflow);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    /// Writes `value` big-endian over the four bytes at `at`, which were put before:
    /// a count or a length learnt after the bytes it measures.
    ///
    /// # Panics
    ///
    /// Asserts the range: a back-patch writes over bytes already put.
    pub fn set_u32(&mut self, at: u32, value: u32) {
        let end = at.checked_add(4).expect("a back-patch writes over bytes already put");
        let start = usize::try_from(at).expect("a u32 fits in a usize");
        let end = usize::try_from(end).expect("a u32 fits in a usize");
        let bytes = self.bytes.get_mut(start..end).expect("a back-patch writes over bytes already put");
        for (byte, value) in bytes.iter_mut().zip(value.to_be_bytes()) {
            *byte = value;
        }
    }

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    /// Keeps the first `len` bytes; a longer `len` changes nothing.
    pub fn truncate(&mut self, len: u32) {
        self.bytes.truncate(usize::try_from(len).expect("a u32 fits in a usize"));
    }

    /// Empties the buffer, keeping its capacity.
    pub fn clear(&mut self) {
        self.bytes.clear();
    }

    /// A copy of the bytes at exactly their length; the buffer is kept as it is.
    #[must_use]
    pub fn to_boxed(&self) -> Box<[u8]> {
        Box::from(self.bytes.as_slice())
    }
}

#[expect(clippy::missing_fields_in_debug, reason = "lib.md, 6: lengths only, never a record's bytes")]
impl core::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Buffer").field("len", &self.len()).field("capacity", &self.capacity).finish()
    }
}

// lib.md, 5.1: hash held state, never capacity or spare room.
impl Hash for Buffer {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
    }
}
