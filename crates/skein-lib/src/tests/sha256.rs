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
