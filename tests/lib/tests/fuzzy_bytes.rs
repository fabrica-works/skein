//! The byte search against a naive one (lib.md, 6), over many random
//! haystacks and needles, and streaming SHA-256 across every cut (lib.md, 6.2).

use skein_lib::Rng;
use skein_lib::bytes::{count, find_from};
use skein_lib_tests::search::{fill, naive, naive_count, up_to};

#[test]
fn the_search_agrees_with_a_naive_one() {
    let mut rng = Rng::new(0x5EA2_C4ED);
    let mut haystack_buffer = [0_u8; 64];
    let mut needle_buffer = [0_u8; 12];
    for round in 0_u32..20_000 {
        let letters = rng.between(1, 4);
        let len = up_to(&mut rng, haystack_buffer.len());
        fill(&mut rng, &mut haystack_buffer[..len], letters);
        let haystack = &haystack_buffer[..len];
        let needle: &[u8] = if rng.chance(500) && !haystack.is_empty() {
            // A piece of the haystack, so that it occurs at least once.
            let start = up_to(&mut rng, haystack.len() - 1);
            &haystack[start..start + 1 + up_to(&mut rng, haystack.len() - start - 1)]
        } else {
            let len = up_to(&mut rng, needle_buffer.len());
            fill(&mut rng, &mut needle_buffer[..len], letters);
            &needle_buffer[..len]
        };
        for from in 0..=haystack.len() + 1 {
            assert_eq!(find_from(haystack, needle, from), naive(haystack, needle, from), "round {round}");
        }
        for cap in [0, 1, 2, 3, u32::MAX] {
            assert_eq!(count(haystack, needle, cap), naive_count(haystack, needle, cap), "round {round}");
        }
    }
}

#[test]
fn random_bytes_round_trip_through_both_alphabets() {
    use skein_lib::{
        Writer,
        base64::{self, Alphabet},
    };
    let mut rng = Rng::new(0xBACE_6400);
    let mut buffer = [0_u8; 64];
    for _ in 0..1_000 {
        let len = up_to(&mut rng, buffer.len());
        for byte in &mut buffer[..len] {
            *byte = u8::try_from(rng.below(256)).expect("one byte");
        }
        for alphabet in [Alphabet::Standard, Alphabet::Url] {
            let size = base64::encoded_len(alphabet, u32::try_from(len).expect("short")).expect("short");
            let mut writer = Writer::new(usize::try_from(size).expect("short"));
            base64::encode(alphabet, &buffer[..len], &mut writer).expect("measured");
            assert_eq!(base64::decode(alphabet, &writer.finish(), 64).expect("canonical").as_ref(), &buffer[..len]);
        }
    }
}

/// The digest's 64 lowercase hexadecimal digits.
fn printed_digest(bytes: [u8; 32]) -> String {
    use std::fmt::Write;
    let mut printed = String::with_capacity(64);
    for byte in bytes {
        write!(printed, "{byte:02x}").expect("writing into a string");
    }
    printed
}

#[test]
fn a_million_as_digest_to_the_fips_180_4_value() {
    let mut state = skein_lib::sha256::Sha256::new();
    for _ in 0..1_000 {
        state.update(&[b'a'; 1_000]);
    }
    assert_eq!(printed_digest(state.finish()), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
}

#[test]
fn every_message_to_200_bytes_digests_alike_cut_at_any_point() {
    let mut buffer = [0_u8; 200];
    for (byte, value) in buffer.iter_mut().zip(0_u8..) {
        *byte = value;
    }
    for length in 0..=200 {
        let message = &buffer[..length];
        let whole = printed_digest(skein_lib::sha256::digest(message));
        for cut in 0..=length {
            let mut state = skein_lib::sha256::Sha256::new();
            state.update(&message[..cut]);
            state.update(&message[cut..]);
            assert_eq!(printed_digest(state.finish()), whole, "{length} bytes, cut at {cut}");
        }
    }
}

#[test]
fn random_messages_cut_at_two_random_points_digest_alike() {
    let mut rng = Rng::new(0x5A26_0001);
    let mut buffer = [0_u8; 200];
    for round in 0..20_000 {
        let length = up_to(&mut rng, buffer.len());
        let message = &mut buffer[..length];
        for byte in message.iter_mut() {
            *byte = u8::try_from(rng.below(256)).expect("one byte");
        }
        let first = up_to(&mut rng, length);
        let second = first + up_to(&mut rng, length - first);
        let mut state = skein_lib::sha256::Sha256::new();
        state.update(&message[..first]);
        state.update(&message[first..second]);
        state.update(&message[second..]);
        assert_eq!(
            printed_digest(state.finish()),
            printed_digest(skein_lib::sha256::digest(message)),
            "round {round}, cuts {first}, {second}"
        );
    }
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
fn rfc_4231_cases_cut_at_every_point_give_the_same_macs() {
    for (key, data, printed) in RFC_4231 {
        let keyed = skein_lib::sha256::Hmac::new(key);
        for cut in 0..=data.len() {
            let mut mac = keyed.clone();
            mac.update(&data[..cut]);
            mac.update(&data[cut..]);
            assert_eq!(printed_digest(mac.finish()), printed, "{printed}, cut at {cut}");
        }
    }
}
