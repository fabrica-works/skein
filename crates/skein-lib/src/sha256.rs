//! Streaming SHA-256 over bounded pieces (lib.md, 6.2).

/// SHA-256 (FIPS 180-4) over pieces cut anywhere (lib.md, 6.2).
#[derive(Clone)]
pub struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    hashed: u64,
}

impl Sha256 {
    /// A hasher that has hashed nothing.
    #[expect(clippy::new_without_default, reason = "step code names no Default (programming-model.md, 10.2)")]
    #[must_use]
    pub const fn new() -> Sha256 {
        Sha256 { state: INITIAL, block: [0; 64], hashed: 0 }
    }

    /// Hashes the next piece; asserts the standard's bound, 2^64 - 1 bits.
    pub fn update(&mut self, mut piece: &[u8]) {
        let used = usize::try_from(self.hashed % 64).expect("block position fits usize");
        self.hashed = self
            .hashed
            .checked_add(u64::try_from(piece.len()).expect("piece length fits u64"))
            .expect("SHA-256 hashes at most 2^64 - 1 bits");
        assert!(self.hashed <= u64::MAX / 8, "SHA-256 hashes at most 2^64 - 1 bits");
        if used != 0 {
            let missing = 64_usize.saturating_sub(used);
            let count = missing.min(piece.len());
            let (front, rest) = piece.split_at_checked(count).expect("carried bytes fit piece");
            for (slot, byte) in self.block.get_mut(used..).expect("block position fits").iter_mut().zip(front) {
                *slot = *byte;
            }
            if count < missing {
                return;
            }
            compress(&mut self.state, &self.block);
            piece = rest;
        }
        let (blocks, rest) = piece.as_chunks::<64>();
        for block in blocks {
            compress(&mut self.state, block);
        }
        for (slot, byte) in self.block.iter_mut().zip(rest) {
            *slot = *byte;
        }
    }

    /// Pads and ends the digest: its 32 bytes in the standard's order.
    #[must_use]
    pub fn finish(mut self) -> [u8; 32] {
        let used = usize::try_from(self.hashed % 64).expect("block position fits usize");
        for slot in self.block.get_mut(used..).expect("block position fits") {
            *slot = 0;
        }
        *self.block.get_mut(used).expect("block position below 64") = 0x80;
        if used >= 56 {
            compress(&mut self.state, &self.block);
            self.block = [0; 64];
        }
        let bits = self.hashed.checked_mul(8).expect("SHA-256 bit count fits u64").to_be_bytes();
        for (slot, byte) in self.block.get_mut(56..).expect("length occupies last eight bytes").iter_mut().zip(bits) {
            *slot = byte;
        }
        compress(&mut self.state, &self.block);
        let mut digest = [0; 32];
        for (bytes, word) in digest.as_chunks_mut::<4>().0.iter_mut().zip(self.state) {
            *bytes = word.to_be_bytes();
        }
        digest
    }

    /// `new`'s state as if `hashed` bytes had been hashed: the bound's test alone.
    #[cfg(test)]
    pub(crate) const fn having_hashed(hashed: u64) -> Sha256 {
        Sha256 { hashed, ..Sha256::new() }
    }
}

#[expect(clippy::missing_fields_in_debug, reason = "lib.md, 6.2: the count of bytes hashed, not the state")]
impl core::fmt::Debug for Sha256 {
    /// `Sha256 { hashed: N }`.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Sha256").field("hashed", &self.hashed).finish()
    }
}

/// The one-shot form: one message's digest.
#[must_use]
pub fn digest(message: &[u8]) -> [u8; 32] {
    let mut state = Sha256::new();
    state.update(message);
    state.finish()
}

/// HMAC-SHA256 (RFC 2104) over pieces cut anywhere (lib.md, 6.2).
#[derive(Clone)]
pub struct Hmac {
    /// Seeded with the key's block XOR 0x36; takes the message.
    inner: Sha256,
    /// Seeded with the key's block XOR 0x5c; takes `inner`'s digest.
    outer: Sha256,
}

impl Hmac {
    /// A MAC keyed by `key`; a key longer than 64 bytes is hashed first.
    #[must_use]
    pub fn new(key: &[u8]) -> Hmac {
        let hashed;
        let key = if key.len() > 64 {
            hashed = digest(key);
            &hashed
        } else {
            key
        };
        let mut inner_pad = [0x36; 64];
        let mut outer_pad = [0x5c; 64];
        for ((inner, outer), byte) in inner_pad.iter_mut().zip(&mut outer_pad).zip(key) {
            *inner ^= *byte;
            *outer ^= *byte;
        }
        let mut inner = Sha256::new();
        let mut outer = Sha256::new();
        inner.update(&inner_pad);
        outer.update(&outer_pad);
        Hmac { inner, outer }
    }

    /// Authenticates the next piece.
    pub fn update(&mut self, piece: &[u8]) {
        self.inner.update(piece);
    }

    /// Ends the MAC: its 32 bytes.
    #[must_use]
    pub fn finish(mut self) -> [u8; 32] {
        self.outer.update(&self.inner.finish());
        self.outer.finish()
    }
}

#[expect(clippy::missing_fields_in_debug, reason = "lib.md, 6.2: the count of bytes hashed, never the key or its pads")]
impl core::fmt::Debug for Hmac {
    /// `Hmac { hashed: N }`, counting the message's bytes alone.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Hmac").field("hashed", &self.inner.hashed.saturating_sub(64)).finish()
    }
}

/// Whether the bytes agree, in a time that depends on their lengths alone.
#[must_use]
pub fn equal(a: &[u8], b: &[u8]) -> bool {
    let mut difference = 0_u8;
    for (left, right) in a.iter().zip(b) {
        difference |= left ^ right;
    }
    a.len() == b.len() && core::hint::black_box(difference) == 0
}

const INITIAL: [u32; 8] =
    [0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a, 0x510e_527f, 0x9b05_688c, 0x1f83_d9ab, 0x5be0_cd19];

const ROUND: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

#[expect(
    clippy::indexing_slicing,
    reason = "fixed 64-word schedule and 64-round constants are indexed only within their ranges"
)]
#[expect(
    clippy::arithmetic_side_effects,
    reason = "SHA schedule indices are proven by the fixed 16..64 and 0..64 ranges"
)]
#[expect(clippy::many_single_char_names, reason = "SHA-256 names its eight working words a through h")]
fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut words = [0_u32; 64];
    for (index, word) in words.iter_mut().take(16).enumerate() {
        let start = index.checked_mul(4).expect("word offset fits");
        *word = u32::from_be_bytes([block[start], block[start + 1], block[start + 2], block[start + 3]]);
    }
    for index in 16..64 {
        let left = words[index - 15];
        let right = words[index - 2];
        let s0 = left.rotate_right(7) ^ left.rotate_right(18) ^ (left >> 3_u32);
        let s1 = right.rotate_right(17) ^ right.rotate_right(19) ^ (right >> 10_u32);
        words[index] = words[index - 16].wrapping_add(s0).wrapping_add(words[index - 7]).wrapping_add(s1);
    }
    let mut a = state[0];
    let mut b = state[1];
    let mut c = state[2];
    let mut d = state[3];
    let mut e = state[4];
    let mut f = state[5];
    let mut g = state[6];
    let mut h = state[7];
    for index in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ ((!e) & g);
        let first = h.wrapping_add(s1).wrapping_add(choose).wrapping_add(ROUND[index]).wrapping_add(words[index]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let second = s0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(first);
        d = c;
        c = b;
        b = a;
        a = first.wrapping_add(second);
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}
