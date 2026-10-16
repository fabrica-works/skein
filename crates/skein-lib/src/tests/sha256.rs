use crate::sha256;

/// FIPS 180-4's short examples, as printed by sha256sum.
const FIPS: [(&[u8], &str); 4] = [
    (b"", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
    (b"abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
    (b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq", "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"),
    (b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu", "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"),
];

/// `a` N times, and its digest as sha256sum prints it.
const EDGES: [(usize, &str); 5] = [
    (55, "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318"),
    (56, "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"),
    (63, "7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34"),
    (64, "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb"),
    (65, "635361c48bb9eab14198e76ea8ab7f1a41685d6ad62aa9146d301d4f17eb0ae0"),
];

/// A digest as printed: 64 lowercase hex digits.
fn hex(text: &str) -> [u8; 32] {
    let mut digest = [0_u8; 32];
    for (byte, at) in digest.iter_mut().zip((0_usize..64).step_by(2)) {
        *byte = u8::from_str_radix(&text[at..][..2], 16).expect("two hex digits");
    }
    digest
}

#[test]
fn fips_180_4_examples_digest_to_their_published_values() {
    for (message, printed) in FIPS {
        assert_eq!(sha256::digest(message), hex(printed), "{printed}");
        let cut = message.len() / 2;
        let mut state = sha256::Sha256::new();
        state.update(&message[..cut]);
        state.update(&message[cut..]);
        assert_eq!(state.finish(), hex(printed), "{printed}, cut at {cut}");
    }
}

#[test]
fn messages_at_the_padding_edges_digest_as_sha256sum_prints() {
    for (length, printed) in EDGES {
        assert_eq!(sha256::digest(&[b'a'; 65][..length]), hex(printed), "{length} bytes");
    }
}

#[test]
fn messages_at_the_padding_edges_cut_at_a_few_points_digest_alike() {
    for (length, printed) in EDGES {
        let message = &[b'a'; 65][..length];
        for cut in [0, 1, 32, length - 1, length] {
            let mut state = sha256::Sha256::new();
            state.update(&message[..cut]);
            state.update(&message[cut..]);
            assert_eq!(state.finish(), hex(printed), "{length} bytes, cut at {cut}");
        }
    }
}

#[test]
fn a_clone_goes_on_from_the_common_prefix_alone() {
    let mut state = sha256::Sha256::new();
    state.update(b"ab");
    let clone = state.clone();
    state.update(b"c");
    assert_eq!(state.finish(), hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
    assert_eq!(clone.finish(), hex("fb8e20fc2e4c3f248c60c39bd652f3c1347298bb977b8b4d5903b85055620603"));
}

#[test]
fn debug_shows_the_count_hashed_and_not_the_state() {
    let mut state = sha256::Sha256::new();
    state.update(b"abc");
    assert_eq!(alloc::fmt::format(format_args!("{state:?}")), "Sha256 { hashed: 3 }");
}

#[test]
#[should_panic(expected = "SHA-256 hashes at most 2^64 - 1 bits")]
fn hashing_past_the_standard_s_bound_is_a_bug() {
    let mut state = sha256::Sha256::having_hashed(u64::MAX / 8);
    state.update(b"");
    let _finished = state.clone().finish();
    state.update(b"a");
}

/// RFC 4231, section 4: key, data and HMAC-SHA256; case 5 in full.
const RFC_4231: [(&[u8], &[u8], &str); 7] = [
    (&[0x0b; 20], b"Hi There", "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"),
    (b"Jefe", b"what do ya want for nothing?", "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"),
    (&[0xaa; 20], &[0xdd; 50], "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe"),
    (&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25], &[0xcd; 50], "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b"),
    (&[0x0c; 20], b"Test With Truncation", "a3b6167473100ee06e0c796c2955552bfa6f7c0a6a8aef8b93f860aab0cd20c5"),
    (&[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First", "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"),
    (&[0xaa; 131], b"This is a test using a larger than block-size key and a larger than block-size data. The key needs to be hashed before being used by the HMAC algorithm.", "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2"),
];

#[test]
fn rfc_4231_cases_1_to_7_give_their_published_macs() {
    for (key, data, printed) in RFC_4231 {
        let mut mac = sha256::Hmac::new(key);
        mac.update(data);
        assert_eq!(mac.finish(), hex(printed), "{printed}");
    }
}

#[test]
fn rfc_4231_cases_cut_at_a_few_points_give_the_same_macs() {
    for (key, data, printed) in RFC_4231 {
        let keyed = sha256::Hmac::new(key);
        for cut in [0, 1, data.len() / 2, data.len() - 1, data.len()] {
            let mut mac = keyed.clone();
            mac.update(&data[..cut]);
            mac.update(&data[cut..]);
            assert_eq!(mac.finish(), hex(printed), "{printed}, cut at {cut}");
        }
    }
}

#[test]
fn a_cloned_hmac_goes_on_from_the_common_prefix_alone() {
    let mut mac = sha256::Hmac::new(&[0x0b; 20]);
    mac.update(b"Hi ");
    let mut clone = mac.clone();
    mac.update(b"There");
    clone.update(b"There!");
    assert_eq!(mac.finish(), hex("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"));
    assert_eq!(clone.finish(), hex("5b5b45718bbcc73dcc6578f7067d021ba39266484b0b24d1ea44c8a853ab891a"));
}

#[test]
fn hmac_debug_shows_the_count_hashed_and_neither_key_nor_pads() {
    let mut mac = sha256::Hmac::new(b"secret-key");
    mac.update(b"abc");
    assert_eq!(alloc::fmt::format(format_args!("{mac:?}")), "Hmac { hashed: 3 }");
}

#[test]
fn equal_slices_are_equal() {
    for bytes in [b"".as_slice(), b"a", &[0xab; 32]] {
        assert!(sha256::equal(bytes, bytes), "{bytes:?}");
    }
}

#[test]
fn a_difference_at_any_one_position_is_unequal() {
    let original = [0xab; 32];
    for at in 0..32 {
        let mut changed = original;
        changed[at] ^= 1;
        assert!(!sha256::equal(&original, &changed), "difference at {at}");
    }
}

#[test]
fn slices_of_different_lengths_are_unequal() {
    for (left, right) in [(b"ab".as_slice(), b"a".as_slice()), (b"ab", b"abc"), (b"", b"a"), (b"a", b"")] {
        assert!(!sha256::equal(left, right), "{left:?}, {right:?}");
    }
}

#[test]
fn keys_of_63_64_and_65_bytes_give_the_macs_openssl_prints() {
    for (length, printed) in [
        (63, "23670528b6b8c55d2f8ef2f36c1f000cec7c23ec76881c485f2e89b5fe705cca"),
        (64, "ae0c0e4a2340cf50185eb46aaa8723f4769153661612e212fb0d1fa3170c6202"),
        (65, "ed378e5dfa30dc98814ba09b2e610d9b6af66054922ceef9480da094a3f11b2d"),
    ] {
        let mut mac = sha256::Hmac::new(&[b'k'; 65][..length]);
        mac.update(b"abc");
        assert_eq!(mac.finish(), hex(printed), "{length}-byte key");
    }
}
