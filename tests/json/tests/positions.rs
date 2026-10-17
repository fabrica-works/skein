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
const FAILURES: [(&[u8], Error, Position); 5] = [
    (b"{}\n {}", Error::Trailing, at(4, 2, 2)),
    (b"[\n[\n[\n[\n[", Error::TooDeep, at(8, 5, 1)),
    (b"[-\n]", Error::Number, at(1, 1, 2)),
    (b"[1\n", Error::Truncated, at(3, 2, 1)),
    (b"nul!", Error::Unexpected, at(3, 1, 4)),
];
#[test]
fn the_reference_and_the_world_place_each_failure_by_the_table() {
    for (document, error, position) in FAILURES {
        assert_eq!(reference::reading(document, &LIMITS).failed_at, Some(position));
        for seed in 0..16 {
            let mut rng = skein_lib::Rng::new(seed);
            let run = world::check(document, &Settings::calm(&mut rng, LIMITS), seed);
            assert_eq!(run.answers.last(), Some(&skein_json::tokenizer::Event::Failed(error)));
            assert_eq!(run.positions.last(), Some(&position));
        }
    }
}
