//! The gzip encoder (http.md, 3.1): every member read back by a small
//! inflater of the tests' own (RFC 1951 and 1952), which checks the CRC
//! and ISIZE as it goes and that every code is complete; the empty body,
//! one byte, runs of the longest match, a match at both ends of the window,
//! a body of several blocks, incompressible bytes stored at exactly the
//! bound, each block written the shortest way, the room asked for from both
//! sides, determinism, the Huffman codes built and cut to their widest, and
//! one golden member.

#![expect(clippy::disallowed_types, reason = "a test builds bodies and inflates members in a Vec")]
#![expect(
    clippy::arithmetic_side_effects,
    reason = "the inflater's cursor and the fixtures' sizes use ordinary test arithmetic"
)]

use alloc::vec::Vec;

use skein_lib::{Rng, bytes};

use crate::gzip::{self, CRC};

/// A member's first four bytes: gzip, DEFLATE, no flags.
const MAGIC: [u8; 4] = [0x1f, 0x8b, 8, 0];

/// The block types (RFC 1951, 3.2.3).
const STORED: u32 = 0;
const FIXED: u32 = 1;
const OWN: u32 = 2;

/// The order in which a block's header sends the widths of its width code
/// (RFC 1951, 3.2.7).
const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// The longest match, the farthest one, and a block's most bytes.
const LONGEST: usize = 258;
const WINDOW: usize = 32_768;
const BLOCK: usize = 65_535;

/// Each length symbol from 257: its base and extra bits (RFC 1951, 3.2.5).
const LENGTHS: [(usize, u32); 29] = [
    (3, 0),
    (4, 0),
    (5, 0),
    (6, 0),
    (7, 0),
    (8, 0),
    (9, 0),
    (10, 0),
    (11, 1),
    (13, 1),
    (15, 1),
    (17, 1),
    (19, 2),
    (23, 2),
    (27, 2),
    (31, 2),
    (35, 3),
    (43, 3),
    (51, 3),
    (59, 3),
    (67, 4),
    (83, 4),
    (99, 4),
    (115, 4),
    (131, 5),
    (163, 5),
    (195, 5),
    (227, 5),
    (258, 0),
];

/// Each distance symbol: its base and extra bits (RFC 1951, 3.2.5).
const DISTANCES: [(usize, u32); 30] = [
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (5, 1),
    (7, 1),
    (9, 2),
    (13, 2),
    (17, 3),
    (25, 3),
    (33, 4),
    (49, 4),
    (65, 5),
    (97, 5),
    (129, 6),
    (193, 6),
    (257, 7),
    (385, 7),
    (513, 8),
    (769, 8),
    (1025, 9),
    (1537, 9),
    (2049, 10),
    (3073, 10),
    (4097, 11),
    (6145, 11),
    (8193, 12),
    (12289, 12),
    (16385, 13),
    (24577, 13),
];

/// What a member inflates to, and how it was coded.
#[derive(Debug)]
struct Inflated {
    body: Vec<u8>,
    /// Each block's type, in order.
    blocks: Vec<u32>,
    /// Each match's length and distance, in order.
    matches: Vec<(usize, usize)>,
}

/// A member's DEFLATE data read bit by bit, each byte's lowest bit first
/// (RFC 1951, 3.1.1).
struct Input<'a> {
    bytes: &'a [u8],
    bit: usize,
}

impl Input<'_> {
    /// `width` bits, the first lowest.
    fn bits(&mut self, width: u32) -> usize {
        let mut value = 0;
        for index in 0..width {
            let byte = self.bytes[self.bit / 8];
            value |= usize::from((byte >> (self.bit % 8)) & 1) << index;
            self.bit += 1;
        }
        value
    }

    /// Skips to the next byte's start, and returns where that is.
    fn align(&mut self) -> usize {
        self.bit = self.bit.div_ceil(8) * 8;
        self.bit / 8
    }
}

/// A canonical Huffman code (RFC 1951, 3.2.2), for decoding: how many codes
/// there are of each width, and the symbols in the codes' order.
struct Code {
    counts: [usize; 16],
    symbols: Vec<usize>,
}

impl Code {
    /// The code whose symbols have these widths, 0 for a symbol unused,
    /// asserted complete: every string of 15 bits starts one code.
    fn new(widths: &[usize]) -> Code {
        let mut counts = [0_usize; 16];
        let mut symbols = Vec::new();
        let mut filled = 0_usize;
        for (width, count) in counts.iter_mut().enumerate().skip(1) {
            for (symbol, each) in widths.iter().enumerate() {
                if *each == width {
                    *count += 1;
                    symbols.push(symbol);
                    filled += 1 << (15 - width);
                }
            }
        }
        for width in widths {
            assert!(*width <= 15, "no code is wider than 15 bits: {widths:?}");
        }
        assert_eq!(filled, 1 << 15_u32, "a complete code: {widths:?}");
        Code { counts, symbols }
    }

    /// The fixed literal/length code (RFC 1951, 3.2.6).
    fn literals() -> Code {
        let mut widths = Vec::new();
        for symbol in 0..288 {
            widths.push(fixed_width(symbol));
        }
        Code::new(&widths)
    }

    /// The fixed distance code: five bits each, 30 and 31 never used.
    fn distances() -> Code {
        Code::new(&[5; 32])
    }

    /// The next symbol, its code read one bit at a time, most significant
    /// first.
    fn decode(&self, input: &mut Input<'_>) -> usize {
        let mut value = 0;
        let mut first = 0;
        let mut index = 0;
        for count in &self.counts[1..] {
            value |= input.bits(1);
            if value < first + count {
                return self.symbols[index + value - first];
            }
            index += count;
            first = (first + count) << 1_u32;
            value <<= 1_u32;
        }
        panic!("no code is longer than 15 bits");
    }
}

/// A literal/length symbol's width in the fixed codes (RFC 1951, 3.2.6).
fn fixed_width(symbol: usize) -> usize {
    match symbol {
        0..=143 | 280.. => 8,
        144..=255 => 9,
        _ => 7,
    }
}

/// Reads a member back, asserting its header and trailer: the body's CRC-32,
/// computed bit by bit here, and its length. Each block is asserted shorter
/// than the other ways it could have been written that the inflater can
/// reckon: a coded block than the same bytes stored, a block in its own
/// codes than the same symbols in the fixed codes.
fn inflate(member: &[u8]) -> Inflated {
    assert_eq!(member[..4], MAGIC, "a gzip member of DEFLATE data, no flags");
    let mut input = Input { bytes: &member[10..], bit: 0 };
    let mut inflated = Inflated { body: Vec::new(), blocks: Vec::new(), matches: Vec::new() };
    let mut last = false;
    while !last {
        let start = input.bit;
        let before = inflated.body.len();
        last = input.bits(1) == 1;
        let kind = u32::try_from(input.bits(2)).unwrap();
        inflated.blocks.push(kind);
        let fixed = match kind {
            STORED => {
                let at = input.align();
                let length = usize::from(u16::from_le_bytes([input.bytes[at], input.bytes[at + 1]]));
                let check = usize::from(!u16::from_le_bytes([input.bytes[at + 2], input.bytes[at + 3]]));
                assert_eq!(length, check, "NLEN is LEN's complement");
                inflated.body.extend_from_slice(&input.bytes[at + 4..at + 4 + length]);
                input.bit = (at + 4 + length) * 8;
                None
            }
            FIXED => Some(codes(&mut input, &mut inflated, &Code::literals(), &Code::distances())),
            OWN => {
                let (literals, distances) = header(&mut input);
                Some(codes(&mut input, &mut inflated, &literals, &distances))
            }
            _ => panic!("block type {kind} is reserved"),
        };
        let took = input.bit - start;
        let length = inflated.body.len() - before;
        assert!(length <= BLOCK, "a block of {length} bytes");
        let stored = (start + 3).div_ceil(8) * 8 - start + 32 + 8 * length;
        match (kind, fixed) {
            (FIXED, Some(fixed)) => {
                assert_eq!(took, 3 + fixed, "the inflater reckons the fixed codes as they were read");
                assert!(took < stored, "a fixed block of {took} bits, stored in {stored}");
            }
            (OWN, Some(fixed)) => {
                assert!(took < 3 + fixed, "a block in its own codes of {took} bits, in the fixed in {}", 3 + fixed);
                assert!(took < stored, "a block in its own codes of {took} bits, stored in {stored}");
            }
            _ => assert_eq!(took, stored, "a stored block as it was reckoned"),
        }
    }
    let at = 10 + input.align();
    let trailer = &member[at..];
    assert_eq!(trailer.len(), 8, "the member ends with its trailer");
    assert_eq!(trailer[..4], crc(&inflated.body).to_le_bytes(), "the CRC-32");
    assert_eq!(trailer[4..], u32::try_from(inflated.body.len()).unwrap().to_le_bytes(), "ISIZE");
    inflated
}

/// Reads a block's header in its own codes (RFC 1951, 3.2.7), and returns
/// its literal/length and distance codes.
fn header(input: &mut Input<'_>) -> (Code, Code) {
    let literals = input.bits(5) + 257;
    let distances = input.bits(5) + 1;
    let sent = input.bits(4) + 4;
    let mut widths = [0; 19];
    for symbol in &ORDER[..sent] {
        widths[*symbol] = input.bits(3);
    }
    let code = Code::new(&widths);
    let mut all = Vec::new();
    while all.len() < literals + distances {
        let (width, repeats) = match code.decode(input) {
            width @ 0..=15 => (width, 1),
            16 => (*all.last().expect("16 repeats a width before it"), 3 + input.bits(2)),
            17 => (0, 3 + input.bits(3)),
            18 => (0, 11 + input.bits(7)),
            symbol => panic!("width symbol {symbol}"),
        };
        all.resize(all.len() + repeats, width);
    }
    assert_eq!(all.len(), literals + distances, "the runs end with the widths");
    assert!(literals <= 286, "{literals} literal/length widths");
    assert!(all[256] > 0, "the end has a code");
    (Code::new(&all[..literals]), Code::new(&all[literals..]))
}

/// Reads a block in these codes, to its end, and returns the bits its
/// symbols would take in the fixed codes.
fn codes(input: &mut Input<'_>, inflated: &mut Inflated, literals: &Code, distances: &Code) -> usize {
    let mut fixed = 0;
    for _ in 0..=input.bytes.len() * 8 {
        let symbol = literals.decode(input);
        fixed += fixed_width(symbol);
        match symbol {
            0..=255 => inflated.body.push(u8::try_from(symbol).unwrap()),
            256 => return fixed,
            257..=285 => {
                let (base, extra) = LENGTHS[symbol - 257];
                let length = base + input.bits(extra);
                let (base, far) = DISTANCES[distances.decode(input)];
                let distance = base + input.bits(far);
                fixed += usize::try_from(extra + 5 + far).unwrap();
                assert!(distance <= WINDOW, "a match reaches back {distance}, past the window");
                assert!(distance <= inflated.body.len(), "a match reaches back {distance}, before the body");
                for _ in 0..length {
                    inflated.body.push(inflated.body[inflated.body.len() - distance]);
                }
                inflated.matches.push((length, distance));
            }
            _ => panic!("length symbol {symbol} is not used"),
        }
    }
    panic!("a block ends before its member does");
}

/// The CRC-32 of `bytes` (RFC 1952, 8), a bit at a time.
fn crc(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8_u32 {
            crc = if crc & 1 == 1 { (crc >> 1_u32) ^ 0xEDB8_8320 } else { crc >> 1_u32 };
        }
    }
    !crc
}

/// `body`'s member, in room of exactly the bound.
fn gzip(body: &[u8]) -> Vec<u8> {
    let mut out = bytes::zeroed(gzip::bound(body.len()).unwrap());
    let length = gzip::encode(body, &mut out).unwrap();
    Vec::from(&out[..length])
}

/// `length` bytes from `seed`, each as likely as another.
fn random(seed: u64, length: usize) -> Vec<u8> {
    let mut rng = Rng::new(seed);
    let mut bytes = Vec::new();
    for _ in 0..length {
        bytes.push(u8::try_from(rng.below(256)).unwrap());
    }
    bytes
}

/// About `length` bytes of JSON-shaped text from `seed`: words drawn from a
/// few, as a request body repeats its keys and its code.
fn wordy(seed: u64, length: usize) -> Vec<u8> {
    const WORDS: [&[u8]; 12] = [
        b"{\"type\":\"function_call_output\",",
        b"\"call_id\":\"call_",
        b"\"output\":\"",
        b"fn encode(body: &[u8]) -> Option<usize> {\\n",
        b"    let ",
        b"match ",
        b"Some(",
        b"None => ",
        b"}\\n",
        b"\"},",
        b"0123",
        b"\\\"",
    ];
    let mut rng = Rng::new(seed);
    let mut body = Vec::new();
    while body.len() < length {
        body.extend_from_slice(WORDS[usize::try_from(rng.below(12)).unwrap()]);
    }
    body
}

/// Each round trip's member is read back to its body, within the bound.
fn round_trip(body: &[u8]) -> Inflated {
    let member = gzip(body);
    assert!(member.len() <= gzip::bound(body.len()).unwrap(), "within the bound");
    let inflated = inflate(&member);
    assert_eq!(inflated.body, body, "the body read back");
    inflated
}

#[test]
fn the_bound_is_the_body_stored_in_blocks_of_65535_bytes_and_the_frame() {
    assert_eq!(gzip::bound(0), Some(10 + 5 + 8), "one empty block at least");
    assert_eq!(gzip::bound(1), Some(10 + 5 + 1 + 8));
    assert_eq!(gzip::bound(BLOCK), Some(10 + 5 + BLOCK + 8));
    assert_eq!(gzip::bound(BLOCK + 1), Some(10 + 10 + BLOCK + 1 + 8), "a second block");
    assert_eq!(gzip::bound(usize::MAX - 18), None, "past usize");
}

#[test]
fn the_empty_body_is_one_empty_block_in_the_fixed_codes() {
    let member = gzip(b"");
    assert_eq!(member, [0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    let inflated = round_trip(b"");
    assert_eq!(inflated.blocks, [FIXED]);
}

#[test]
fn one_byte_is_one_literal() {
    let inflated = round_trip(b"{");
    assert_eq!(inflated.blocks, [FIXED]);
    assert!(inflated.matches.is_empty(), "nothing to match");
}

#[test]
fn a_run_is_matched_at_distance_one_in_matches_of_258_bytes() {
    let body = [b' '; 1 + 3 * LONGEST];
    let inflated = round_trip(&body);
    assert_eq!(inflated.matches, [(LONGEST, 1); 3]);
    let shorter = round_trip(&body[..LONGEST]);
    assert_eq!(shorter.matches, [(LONGEST - 1, 1)], "a match ends with the body");
}

#[test]
fn a_match_reaches_back_32768_bytes_and_no_further() {
    // No four bytes of the phrase occur twice in it.
    let phrase = b"\"role\":\"assistant\",\"content\":[{";
    for (gap, reached) in [(WINDOW, true), (WINDOW + 1, false)] {
        let mut body = Vec::new();
        body.extend_from_slice(phrase);
        body.resize(gap, b'.');
        body.extend_from_slice(phrase);
        let inflated = round_trip(&body);
        let mut farthest = 0;
        for (_, distance) in &inflated.matches {
            farthest = farthest.max(*distance);
        }
        if reached {
            assert_eq!(inflated.matches.last(), Some(&(phrase.len(), WINDOW)), "the phrase, matched whole");
        } else {
            assert_eq!(farthest, 1, "the run's matches only: the phrase is out of reach");
        }
    }
}

#[test]
fn a_long_body_is_cut_into_blocks_of_65535_bytes() {
    let body = wordy(2, 2 * BLOCK + 100);
    let inflated = round_trip(&body);
    assert_eq!(inflated.blocks, [OWN, OWN, FIXED], "the last block's hundred bytes too few for codes of their own");
    let member = gzip(&body);
    assert!(member.len() * 4 < body.len(), "words compress: {} of {}", member.len(), body.len());
}

#[test]
fn incompressible_bytes_are_stored_at_exactly_the_bound() {
    let body = random(3, BLOCK + 1000);
    let inflated = round_trip(&body);
    assert_eq!(inflated.blocks, [STORED; 2]);
    assert_eq!(gzip(&body).len(), gzip::bound(body.len()).unwrap(), "every block stored");
}

#[test]
fn each_block_is_written_the_shortest_way() {
    // The inflater asserts each block shorter than the ways it can reckon.
    let mut body = wordy(4, BLOCK);
    body.truncate(BLOCK);
    body.extend_from_slice(&random(5, BLOCK));
    body.extend_from_slice(&wordy(6, 1000));
    let inflated = round_trip(&body);
    assert_eq!(inflated.blocks, [OWN, STORED, OWN]);
    assert_eq!(round_trip(b"{\"type\":\"message\"}").blocks, [FIXED], "too short for codes of its own");
}

#[test]
fn mixed_bodies_round_trip() {
    for seed in 0..20 {
        let mut body = Vec::new();
        let mut rng = Rng::new(seed);
        for piece in 0..rng.between(0, 8) {
            let length = usize::try_from(rng.between(0, 600)).unwrap();
            if rng.chance(500) {
                body.extend_from_slice(&random(seed * 100 + piece, length));
            } else {
                body.extend_from_slice(&wordy(seed * 100 + piece, length));
            }
        }
        round_trip(&body);
    }
}

#[test]
fn the_room_must_hold_the_bound() {
    let body = wordy(7, 300);
    let bound = gzip::bound(body.len()).unwrap();
    let mut out = bytes::zeroed(bound - 1);
    assert_eq!(gzip::encode(&body, &mut out), None, "a byte short of the bound");
    let mut out = bytes::zeroed(bound);
    let length = gzip::encode(&body, &mut out).unwrap();
    assert_eq!(inflate(&out[..length]).body, body);
}

#[test]
fn the_same_body_gives_the_same_bytes_whatever_the_room_held() {
    let body = wordy(8, 5000);
    let member = gzip(&body);
    let mut out = Vec::new();
    out.resize(gzip::bound(body.len()).unwrap() + 100, 0xa5);
    let length = gzip::encode(&body, &mut out).unwrap();
    assert_eq!(out[..length], member);
}

#[test]
fn the_trailer_holds_the_crc_and_the_length() {
    let member = gzip(b"123456789");
    let trailer = &member[member.len() - 8..];
    assert_eq!(trailer[..4], 0xCBF4_3926_u32.to_le_bytes(), "CRC-32's check value");
    assert_eq!(trailer[4..], [9, 0, 0, 0]);
    assert_eq!(gzip::crc(b"123456789"), Some(0xCBF4_3926));
}

#[test]
fn the_crc_table_is_each_byte_alone() {
    for (byte, entry) in (0_u8..=255).zip(CRC) {
        let mut crc = u32::from(byte);
        for _ in 0..8_u32 {
            crc = if crc & 1 == 1 { (crc >> 1_u32) ^ 0xEDB8_8320 } else { crc >> 1_u32 };
        }
        assert_eq!(entry, crc, "the entry for {byte}");
    }
}

/// The code the encoder builds for `counts`, no wider than `widest`, each
/// symbol's width asserted: the code complete, each symbol's bits read
/// back to it, and a commoner symbol never given a wider code.
fn huffman(counts: &[u32], widest: u32) -> Vec<usize> {
    let mut codes = Vec::new();
    codes.resize(counts.len(), gzip::Code { bits: 0, width: 0 });
    gzip::huffman(counts, widest, &mut codes).unwrap();
    let mut widths = Vec::new();
    for each in &codes {
        widths.push(usize::try_from(each.width).unwrap());
    }
    let code = Code::new(&widths);
    for (symbol, each) in codes.iter().enumerate() {
        if each.width > 0 {
            let mut bytes = [0_u8; 2];
            for bit in 0..each.width {
                let at = usize::try_from(bit).unwrap();
                bytes[at / 8] |= u8::try_from((each.bits >> bit) & 1).unwrap() << (at % 8);
            }
            assert_eq!(code.decode(&mut Input { bytes: &bytes, bit: 0 }), symbol, "symbol {symbol}'s bits");
        }
    }
    for (one, width) in widths.iter().enumerate() {
        assert!(*width <= usize::try_from(widest).unwrap(), "symbol {one} is {width} bits wide");
        for (other, than) in widths.iter().enumerate() {
            if counts[one] > counts[other] && counts[other] > 0 {
                assert!(width <= than, "symbol {one} is commoner than {other} and wider");
            }
        }
    }
    widths
}

/// The fewest bits `counts` take in any prefix code: the weight of every
/// node an unbounded Huffman tree is built of.
fn least(counts: &[u32]) -> u64 {
    let mut weights = Vec::new();
    for count in counts {
        if *count > 0 {
            weights.push(u64::from(*count));
        }
    }
    let mut total = 0;
    while weights.len() > 1 {
        weights.sort_unstable();
        let node = weights.remove(0) + weights.remove(0);
        total += node;
        weights.push(node);
    }
    total
}

/// The bits `counts` take in codes of `widths`.
fn weight(counts: &[u32], widths: &[usize]) -> u64 {
    let mut total = 0;
    for (count, width) in counts.iter().zip(widths) {
        total += u64::from(*count) * u64::try_from(*width).unwrap();
    }
    total
}

#[test]
fn a_huffman_code_is_complete_canonical_and_as_short_as_any() {
    // Counts no more than five times apart build no tree deeper than 15:
    // the code is the tree's. Counts from 1 may: the code is no shorter.
    for (seed, rarest) in (0..20).zip([1000, 1].into_iter().cycle()) {
        let mut rng = Rng::new(seed);
        let mut counts = [0_u32; 288];
        for count in &mut counts {
            if rng.chance(700) {
                *count = u32::try_from(rng.between(rarest, 5000)).unwrap();
            }
        }
        let widths = huffman(&counts, 15);
        if rarest == 1 {
            assert!(weight(&counts, &widths) >= least(&counts), "seed {seed}: no shorter than the tree");
        } else {
            assert_eq!(weight(&counts, &widths), least(&counts), "seed {seed}: the counts in the fewest bits");
        }
    }
}

#[test]
fn a_code_deeper_than_its_widest_is_cut_to_it() {
    // Counts that grow as Fibonacci's numbers build a tree as deep as there
    // are symbols.
    let mut counts = [0_u32; 30];
    let (mut one, mut two) = (1, 1);
    for count in &mut counts {
        *count = one;
        (one, two) = (two, one + two);
    }
    let widths = huffman(&counts, 15);
    assert_eq!(widths.iter().max(), Some(&15), "cut to 15 bits");
    assert!(weight(&counts, &widths) > least(&counts), "the cut code is longer than the tree");
    let widths = huffman(&counts[..19], 7);
    assert_eq!(widths.iter().max(), Some(&7), "a width code cut to 7 bits");
    assert_eq!(huffman(&[1; 19], 7).iter().max(), Some(&5), "19 alike: four and five bits");
}

#[test]
fn a_code_of_one_symbol_or_none_has_two_of_one_bit() {
    let mut counts = [0_u32; 30];
    assert_eq!(huffman(&counts, 15)[..3], [1, 1, 0], "none: 0 and 1");
    counts[0] = 7;
    assert_eq!(huffman(&counts, 15)[..3], [1, 1, 0], "0 alone: 0 and 1");
    counts[0] = 0;
    counts[1] = 7;
    assert_eq!(huffman(&counts, 15)[..3], [1, 1, 0], "1 alone: 0 and 1");
    counts[1] = 0;
    counts[29] = 7;
    let widths = huffman(&counts, 15);
    assert_eq!((widths[0], widths[1], widths[29]), (1, 0, 1), "29 alone: 0 and 29");
}

#[test]
fn a_code_past_its_limits_is_refused() {
    let mut codes = [gzip::Code { bits: 0, width: 0 }; 30];
    assert_eq!(gzip::huffman(&[1; 30], 0, &mut codes), None, "no width");
    assert_eq!(gzip::huffman(&[1; 30], 16, &mut codes), None, "wider than DEFLATE's 15");
    assert_eq!(gzip::huffman(&[1; 29], 15, &mut codes), None, "a code for each count");
    assert_eq!(gzip::huffman(&[1; 30], 4, &mut codes), None, "30 symbols in four bits");
}

/// A small request body's member, byte for byte: one block in its own
/// codes. Checked once, on 2026-10-20, by writing these bytes to a file and
/// running `gzip -dc file | cmp - body` and `gzip -t file`, where `body`
/// held [`GOLDEN_BODY`].
const GOLDEN_BODY: &[u8] = b"{\"model\":\"gpt-5-codex\",\"input\":[{\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"hello\"}]},{\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"hello again\"}]}],\"stream\":true}";

const GOLDEN: &[u8] = &[
    0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xa5, 0x8c, 0x41, 0x0a, 0x84, 0x30, 0x10, 0x04, 0xbf,
    0xb2, 0xf4, 0x79, 0x3c, 0xee, 0x65, 0xbe, 0xb2, 0xc8, 0x12, 0xb4, 0x71, 0x85, 0x98, 0x09, 0x71, 0x02, 0x2e, 0x92,
    0xbf, 0x8b, 0xfa, 0x04, 0x4f, 0x75, 0xa8, 0xa2, 0x76, 0x2c, 0x36, 0x32, 0x42, 0x31, 0x65, 0xef, 0xde, 0xdd, 0x60,
    0x23, 0x37, 0x08, 0xe6, 0x94, 0xab, 0x43, 0x3f, 0x3b, 0x8a, 0x45, 0x42, 0x51, 0x57, 0x16, 0x08, 0x06, 0x4b, 0xce,
    0x74, 0x1b, 0xff, 0xe7, 0xd3, 0x5c, 0xe9, 0xd7, 0xb9, 0x39, 0x04, 0x17, 0x14, 0x3f, 0xc6, 0x68, 0x68, 0x7d, 0x93,
    0x27, 0x83, 0x57, 0x98, 0xc2, 0x9c, 0xce, 0x4d, 0x2f, 0x58, 0xbd, 0x30, 0x2c, 0x50, 0x2f, 0x95, 0xed, 0x00, 0xef,
    0x1e, 0x0e, 0x98, 0xb7, 0x00, 0x00, 0x00,
];

#[test]
fn a_small_request_body_gives_its_golden_member() {
    let member = gzip(GOLDEN_BODY);
    assert_eq!(member, GOLDEN, "{member:?}");
    assert_eq!(round_trip(GOLDEN_BODY).blocks, [OWN]);
}
