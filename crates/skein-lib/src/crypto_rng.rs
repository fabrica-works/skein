//! `ChaCha20` with the original 64-bit counter and nonce (lib.md, 8).
use core::hash::{Hash, Hasher};

use crate::Secret;

/// Secret randomness from a 32-byte key and a process-distinct stream.
#[derive(Clone)]
pub struct CryptoRng {
    words: [u32; 16],
    block: [u8; 64],
    used: usize,
    stream: u64,
    exhausted: bool,
}
impl CryptoRng {
    /// Makes a stream only when the supplied seed holds exactly 32 bytes.
    #[must_use]
    pub fn new(seed: &Secret, stream: u64) -> Option<CryptoRng> {
        if seed.len() != 32 {
            return None;
        }
        let mut words = [0_u32; 16];
        for (slot, word) in words.iter_mut().zip([0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574]) {
            *slot = word;
        }
        for (slot, bytes) in words.get_mut(4..12)?.iter_mut().zip(seed.expose().chunks_exact(4)) {
            *slot = u32::from_le_bytes(bytes.try_into().ok()?);
        }
        for (slot, bytes) in words.get_mut(14..)?.iter_mut().zip(stream.to_le_bytes().chunks_exact(4)) {
            *slot = u32::from_le_bytes(bytes.try_into().ok()?);
        }
        Some(CryptoRng { words, block: [0; 64], used: 64, stream, exhausted: false })
    }
    /// Fills the caller's buffer, preserving unused block bytes between calls.
    pub fn fill(&mut self, out: &mut [u8]) {
        for byte in out {
            if self.used == 64 {
                self.refill();
            }
            *byte = *self.block.get(self.used).expect("unconsumed block byte");
            self.used = self.used.checked_add(1).expect("at most 64 consumed bytes");
        }
    }
    fn refill(&mut self) {
        assert!(!self.exhausted, "a ChaCha stream never repeats its 64-bit counter");
        let mut words = self.words;
        for _ in 0_u32..10 {
            for [a, b, c, d] in [
                [0, 4, 8, 12],
                [1, 5, 9, 13],
                [2, 6, 10, 14],
                [3, 7, 11, 15],
                [0, 5, 10, 15],
                [1, 6, 11, 12],
                [2, 7, 8, 13],
                [3, 4, 9, 14],
            ] {
                quarter(&mut words, a, b, c, d);
            }
        }
        for ((bytes, word), initial) in self.block.chunks_exact_mut(4).zip(words).zip(self.words) {
            for (slot, byte) in bytes.iter_mut().zip(word.wrapping_add(initial).to_le_bytes()) {
                *slot = byte;
            }
        }
        let low = self.words.get_mut(12).expect("counter low word");
        *low = low.wrapping_add(1);
        if *low == 0 {
            let high = self.words.get_mut(13).expect("counter high word");
            *high = high.wrapping_add(1);
            self.exhausted = *high == 0;
        }
        self.used = 0;
    }
}
fn quarter(words: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    let mut a_word = *words.get(a).expect("round index");
    let mut b_word = *words.get(b).expect("round index");
    let mut c_word = *words.get(c).expect("round index");
    let mut d_word = *words.get(d).expect("round index");
    for rotation in [[16_u32, 12_u32], [8_u32, 7_u32]] {
        a_word = a_word.wrapping_add(b_word);
        d_word = (d_word ^ a_word).rotate_left(*rotation.first().expect("first rotation"));
        c_word = c_word.wrapping_add(d_word);
        b_word = (b_word ^ c_word).rotate_left(*rotation.get(1).expect("second rotation"));
    }
    *words.get_mut(a).expect("round index") = a_word;
    *words.get_mut(b).expect("round index") = b_word;
    *words.get_mut(c).expect("round index") = c_word;
    *words.get_mut(d).expect("round index") = d_word;
}
#[expect(clippy::missing_fields_in_debug, reason = "lib.md, 8 forbids showing the key or buffered secret bytes")]
impl core::fmt::Debug for CryptoRng {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CryptoRng").field("stream", &self.stream).finish()
    }
}

// lib.md, 5.1: hash held state, never capacity or storage layout.
impl Hash for CryptoRng {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.stream.hash(state);
        for word in self.words.get(12..14).expect("the two counter words") {
            word.hash(state);
        }
        self.used.hash(state);
        self.exhausted.hash(state);
    }
}
