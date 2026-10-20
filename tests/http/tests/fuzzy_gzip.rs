//! The gzip encoder swept (testing-strategy.md, 8; http.md, 3.1): many
//! bodies drawn from each seed, random, repetitive and mixed, of lengths
//! about the blocks' edges, each encoded into room of its bound and more.
//! The member never passes the bound, room short of the bound is refused,
//! the same body gives the same bytes, and the frame holds the header, the
//! body's CRC-32 and its length. Round trips through an inflater are the
//! focused tests' (skein-http's `tests/gzip.rs`).
//!
//! A sweep asserts that what it injects fell (testing-strategy.md, 3): each
//! kind of body, a body past one block, and members stored at the bound
//! and coded well under it.

use std::collections::BTreeMap;

use skein_http::gzip;
use skein_lib::Rng;

const ROUNDS: u64 = 100;

/// The most a block holds (RFC 1951, 3.2.4).
const BLOCK: usize = 65_535;

/// The member's header: gzip, DEFLATE, no flags, no time, an unknown system.
const HEADER: [u8; 10] = [0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Random,
    Runs,
    Words,
    Copies,
    Mixed,
}

/// JSON and source text, the bodies the encoder is for.
const WORDS: [&[u8]; 12] = [
    b"{\"type\":\"function_call_output\",\"call_id\":\"call_",
    b"\",\"output\":\"",
    b"fn ",
    b"(&mut self) -> Option<usize> {\\n",
    b"    let ",
    b" = ",
    b".checked_add(1)?;\\n",
    b"}\\n",
    b"{\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"",
    b"\"}]},",
    b"//! ",
    b" the ",
];

/// A number in `low..=high`.
fn between(rng: &mut Rng, low: usize, high: usize) -> usize {
    let low = u64::try_from(low).expect("a length fits u64");
    let high = u64::try_from(high).expect("a length fits u64");
    usize::try_from(rng.between(low, high)).expect("a number drawn within lengths fits usize")
}

fn length(rng: &mut Rng) -> usize {
    match rng.below(6) {
        0 => between(rng, 0, 15),
        1 => between(rng, 0, 1_023),
        2 => between(rng, 0, 19_999),
        // About the edge of the first and the second block.
        3 => between(rng, BLOCK - 8, BLOCK + 8),
        4 => between(rng, 2 * BLOCK - 4, 2 * BLOCK + 4),
        _ => between(rng, 20_000, 100_000),
    }
}

fn byte(rng: &mut Rng) -> u8 {
    u8::try_from(rng.below(256)).expect("a number below 256 fits u8")
}

fn one_of(rng: &mut Rng, kinds: &[Kind]) -> Kind {
    kinds[between(rng, 0, kinds.len() - 1)]
}

/// A body of `length` bytes of the given kind.
fn body(rng: &mut Rng, kind: Kind, length: usize) -> Vec<u8> {
    let mut body = Vec::with_capacity(length);
    while body.len() < length {
        let pick = match kind {
            Kind::Mixed => one_of(rng, &[Kind::Random, Kind::Runs, Kind::Words, Kind::Copies]),
            other @ (Kind::Random | Kind::Runs | Kind::Words | Kind::Copies) => other,
        };
        match pick {
            Kind::Random | Kind::Mixed => {
                for _ in 0..rng.between(1, 300) {
                    body.push(byte(rng));
                }
            }
            Kind::Runs => {
                let byte = byte(rng);
                for _ in 0..rng.between(1, 600) {
                    body.push(byte);
                }
            }
            Kind::Words => body.extend_from_slice(WORDS[between(rng, 0, WORDS.len() - 1)]),
            Kind::Copies => {
                if body.len() < 4 {
                    body.push(byte(rng));
                    continue;
                }
                // From anywhere before, near and past the window's 32 KiB.
                let back = between(rng, 1, body.len().min(40_000));
                let from = body.len() - back;
                let count = between(rng, 1, 400).min(back);
                body.extend_from_within(from..from + count);
            }
        }
    }
    body.truncate(length);
    body
}

/// The CRC-32 of `body`, bit by bit (RFC 1952, 8).
fn crc(body: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in body {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[test]
fn every_member_is_within_its_bound_framed_and_the_same_for_the_same_body() {
    let mut fell: BTreeMap<String, u32> = BTreeMap::new();
    for round in 0..ROUNDS {
        let seed = 0x00a1_0000 + round;
        let mut rng = Rng::new(seed);
        let kind = one_of(&mut rng, &[Kind::Random, Kind::Runs, Kind::Words, Kind::Copies, Kind::Mixed]);
        let length = length(&mut rng);
        let body = body(&mut rng, kind, length);
        let bound = gzip::bound(body.len()).unwrap();

        let mut out = vec![0_u8; bound];
        let written =
            gzip::encode(&body, &mut out).unwrap_or_else(|| panic!("seed {seed:#x}: room of the bound refused"));
        assert!(written <= bound, "seed {seed:#x}: {written} bytes past the bound {bound}");
        let member = &out[..written];
        assert_eq!(member[..10], HEADER, "seed {seed:#x}: the header");
        let trailer = &member[written - 8..];
        assert_eq!(trailer[..4], crc(&body).to_le_bytes(), "seed {seed:#x}: the CRC");
        assert_eq!(trailer[4..], u32::try_from(body.len()).unwrap().to_le_bytes(), "seed {seed:#x}: ISIZE");

        // More room, filled with something else first: the same bytes.
        let extra = between(&mut rng, 1, 64);
        let mut more = vec![byte(&mut rng); bound + extra];
        assert_eq!(gzip::encode(&body, &mut more), Some(written), "seed {seed:#x}: the length again");
        assert_eq!(&more[..written], member, "seed {seed:#x}: the same body, other bytes");

        // Room short of the bound, by as little as one byte: refused.
        let short = between(&mut rng, 1, bound);
        let mut less = vec![0_u8; bound - short];
        assert_eq!(gzip::encode(&body, &mut less), None, "seed {seed:#x}: room short by {short} taken");

        *fell.entry(format!("{kind:?}")).or_default() += 1;
        if body.len() > BLOCK {
            *fell.entry("past one block".into()).or_default() += 1;
        }
        if written == bound {
            *fell.entry("stored at the bound".into()).or_default() += 1;
        }
        if body.len() >= 1_000 && written * 3 < body.len() {
            *fell.entry("coded under a third".into()).or_default() += 1;
        }
    }
    for what in
        ["Random", "Runs", "Words", "Copies", "Mixed", "past one block", "stored at the bound", "coded under a third"]
    {
        assert!(fell.get(what).copied().unwrap_or(0) > 0, "nothing fell as {what}: {fell:?}");
    }
}
