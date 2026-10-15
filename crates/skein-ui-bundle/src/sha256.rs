// SHA-256, FIPS 180-4, section 6.2.

const K: [u32; 64] = [
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

pub(crate) fn digest(input: &[u8]) -> [u8; 32] {
    let mut data = input.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    let bits = u64::try_from(input.len()).expect("input length fits u64").wrapping_mul(8);
    data.extend_from_slice(&bits.to_be_bytes());
    let mut h =
        [0x6a09_e667_u32, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a, 0x510e_527f, 0x9b05_688c, 0x1f83_d9ab, 0x5be0_cd19];
    for block in data.chunks_exact(64) {
        let mut w = [0_u32; 64];
        for (i, word) in w.get_mut(..16).expect("sixteen words").iter_mut().enumerate() {
            let start = i * 4;
            let bytes: [u8; 4] = block.get(start..start + 4_usize).expect("block word").try_into().expect("four bytes");
            *word = u32::from_be_bytes(bytes);
        }
        for i in 16..64 {
            let a = *w.get(i - 15).expect("prior word");
            let b = *w.get(i - 2).expect("prior word");
            let s0 = a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3_u32);
            let s1 = b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10_u32);
            let next = w
                .get(i - 16_usize)
                .expect("prior word")
                .wrapping_add(s0)
                .wrapping_add(*w.get(i - 7_usize).expect("prior word"))
                .wrapping_add(s1);
            *w.get_mut(i).expect("word") = next;
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let choose = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(choose)
                .wrapping_add(*K.get(i).expect("round constant"))
                .wrapping_add(*w.get(i).expect("round word"));
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let majority = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(majority);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (value, update) in h.iter_mut().zip(v) {
            *value = value.wrapping_add(update);
        }
    }
    let mut out = [0_u8; 32];
    for (i, word) in h.iter().enumerate() {
        let bytes = word.to_be_bytes();
        for (slot, byte) in out.get_mut(i * 4..i * 4 + 4).expect("digest word").iter_mut().zip(bytes) {
            *slot = byte;
        }
    }
    out
}
