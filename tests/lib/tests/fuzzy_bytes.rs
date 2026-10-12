//! The byte search against a naive one (lib.md, 6), over many random
//! haystacks and needles.

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
