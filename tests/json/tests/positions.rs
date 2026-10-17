//! Where the reference and the world place answers (json.md, 3.6 and 6).
use skein_json::tokenizer::{Limits, Position};
use skein_json_world::{
    reference,
    world::{self, Settings},
};
const LIMITS: Limits = Limits { depth: 4, string: 16, number: 8, chunk: 4, length: 1 << 16 };
const DOCUMENT: &[u8] = b"{\r\n  \"caf\xc3\xa9\": [1,\n true]}";
const fn at(offset: u32, line: u32, column: u32) -> Position {
    Position { offset, line, column }
}
const TOKENS: [Position; 7] =
    [at(0, 1, 1), at(5, 2, 3), at(14, 2, 12), at(15, 2, 13), at(19, 3, 2), at(23, 3, 6), at(24, 3, 7)];
#[test]
fn the_reference_and_the_world_place_each_token_where_counted() {
    assert_eq!(reference::reading(DOCUMENT, &LIMITS).at, TOKENS);
    for seed in 0..16 {
        let mut rng = skein_lib::Rng::new(seed);
        let run = world::check(DOCUMENT, &Settings::calm(&mut rng, LIMITS), seed);
        assert_eq!(run.positions[..TOKENS.len()], TOKENS, "seed {seed}");
    }
}

use skein_json::tokenizer::Error;
const SHORT_STRING: Limits = Limits { string: 4, ..LIMITS };
const FAILURES: [(&[u8], Limits, Error, Position); 22] = [
    (b"{}\n {}", LIMITS, Error::Trailing, at(4, 2, 2)),
    (b"[\n[\n[\n[\n[", LIMITS, Error::TooDeep, at(8, 5, 1)),
    (b"[-\n]", LIMITS, Error::Number, at(1, 1, 2)),
    (b"[1\n", LIMITS, Error::Truncated, at(3, 2, 1)),
    (b"nul!", LIMITS, Error::Unexpected, at(3, 1, 4)),
    (b"\"ab\\x\"", LIMITS, Error::Escape, at(3, 1, 4)),
    (b"\"\\u12G4\"", LIMITS, Error::Escape, at(1, 1, 2)),
    (b"{\"k\\q\":1}", LIMITS, Error::Escape, at(3, 1, 4)),
    (b"\"a\\uDC00\"", LIMITS, Error::Surrogate, at(2, 1, 3)),
    (b"\"\\uD800\"", LIMITS, Error::Surrogate, at(1, 1, 2)),
    (b"\"x\\uD800\\u0041\"", LIMITS, Error::Surrogate, at(2, 1, 3)),
    (b"\"\\uDC00\\uD800\"", LIMITS, Error::Surrogate, at(1, 1, 2)),
    (b"\"ab\xff\"", LIMITS, Error::Utf8, at(3, 1, 4)),
    (b"\"abc\xe2\x28\xa1\"", LIMITS, Error::Utf8, at(4, 1, 5)),
    (b"\"\xc3\"", LIMITS, Error::Utf8, at(1, 1, 2)),
    (b"[\n\"a\x01\"]", LIMITS, Error::Control, at(4, 2, 3)),
    (b"\"abcde\"", SHORT_STRING, Error::StringTooLong, at(5, 1, 6)),
    (b"\"a\xF0\x9F\x98\x80\"", SHORT_STRING, Error::StringTooLong, at(2, 1, 3)),
    (b"\"abcd\\n\"", SHORT_STRING, Error::StringTooLong, at(5, 1, 6)),
    (b"{\"abcde\":1}", SHORT_STRING, Error::StringTooLong, at(6, 1, 7)),
    (b"\"\\ud83d\\u12G4\"", SHORT_STRING, Error::Escape, at(1, 1, 2)),
    (b"\"a\\ud83d\\ude00\"", SHORT_STRING, Error::StringTooLong, at(2, 1, 3)),
];
#[test]
fn the_reference_and_the_world_place_each_failure_by_the_table() {
    for (document, limits, error, position) in FAILURES {
        assert_eq!(reference::reading(document, &limits).failed_at, Some(position));
        for seed in 0..16 {
            let mut rng = skein_lib::Rng::new(seed);
            let run = world::check(document, &Settings::calm(&mut rng, limits), seed);
            assert_eq!(run.answers.last(), Some(&skein_json::tokenizer::Event::Failed(error)));
            assert_eq!(run.positions.last(), Some(&position));
        }
    }
}
