//! Positions (json.md, 3.6), counted by hand.
use super::{LIMITS, Machine, number, placed};
use crate::{
    Token,
    tokenizer::{self, Event, Limits, Position, Request},
};
use skein_lib::stream::Up;

const fn at(offset: u32, line: u32, column: u32) -> Position {
    Position { offset, line, column }
}
const AFTER_WHITESPACE: [(&[u8], Position); 5] = [
    (b"1", at(0, 1, 1)),
    (b"   1", at(3, 1, 4)),
    (b"\t\n\n 1", at(4, 3, 2)),
    (b"\r\n\r\n1", at(4, 3, 1)),
    (b" \r 1", at(3, 1, 4)),
];
#[test]
fn a_token_is_at_its_first_byte_after_any_whitespace() {
    for (document, expected) in AFTER_WHITESPACE {
        assert_eq!(placed(document, LIMITS, &[]).first(), Some(&(Event::Token(number(b"1")), expected)));
    }
}
const WIDE: [(&[u8], Position); 3] = [
    (b"[\"\xc3\xa9\",1]", at(6, 1, 7)),
    (b"[\"\xe2\x82\xac\",1]", at(7, 1, 8)),
    (b"[\"\xf0\x9f\x98\x80\",1]", at(8, 1, 9)),
];
#[test]
fn a_column_counts_each_byte_of_a_wide_character() {
    for (document, expected) in WIDE {
        assert_eq!(placed(document, LIMITS, &[])[2], (Event::Token(number(b"1")), expected));
    }
}
#[test]
fn a_number_ended_by_a_line_feed_stays_on_its_line() {
    let answers = placed(b"[1\n,2]", LIMITS, &[]);
    assert_eq!(
        &answers[1..4],
        &[
            (Event::Token(number(b"1")), at(1, 1, 2)),
            (Event::Token(number(b"2")), at(4, 2, 2)),
            (Event::Token(Token::ArrayEnd), at(5, 2, 3))
        ]
    );
}
#[test]
fn long_and_skipped_are_at_their_value_s_first_byte() {
    let answers = placed(b"[\n\"abcdef\", {\"k\":\n[1]}]", LIMITS, &[Request::Next, Request::Text(2), Request::Skip]);
    assert_eq!(
        &answers[1..4],
        &[
            (Event::Long(6), at(2, 2, 1)),
            (Event::Skipped(10), at(12, 2, 11)),
            (Event::Token(Token::ArrayEnd), at(22, 3, 5))
        ]
    );
}
#[test]
fn the_position_holds_from_an_answer_until_the_next_demand() {
    let mut machine = Machine::new(LIMITS);
    machine.down(Request::Next);
    machine.bytes(b"\n");
    machine.bytes(b"[");
    assert_eq!(machine.tokenizer.position(), at(1, 2, 1));
    machine.up(Up::End);
    assert_eq!(machine.tokenizer.position(), at(1, 2, 1));
}
#[test]
fn before_the_first_answer_and_after_restart_it_is_the_start() {
    let mut machine = Machine::new(LIMITS);
    assert_eq!(machine.tokenizer.position(), at(0, 1, 1));
    machine.down(Request::Next);
    machine.bytes(b" ");
    assert_eq!(machine.tokenizer.position(), at(0, 1, 1));
    assert_eq!(machine.up(Up::End).0, Some(Event::Failed(Error::Truncated)));
    machine.tokenizer.restart();
    assert_eq!(machine.tokenizer.position(), at(0, 1, 1));
}
#[test]
#[should_panic(expected = "the end position fits u32")]
fn a_length_of_u32_max_has_no_worst_case_and_no_tokenizer() {
    let limits = Limits { length: u32::MAX, ..LIMITS };
    assert_eq!(tokenizer::worst_case(&limits), None);
    let _tokenizer = tokenizer::Tokenizer::new(&limits);
}

use crate::tokenizer::Error;
const UNEXPECTED: [(&[u8], Error, Position); 5] = [
    (b"[}", Error::Unexpected, at(1, 1, 2)),
    (b"{\"a\" 1}", Error::Unexpected, at(5, 1, 6)),
    (b"nul!", Error::Unexpected, at(3, 1, 4)),
    (b"t\nue", Error::Unexpected, at(1, 1, 2)),
    (b"{}\n {}", Error::Trailing, at(4, 2, 2)),
];
#[test]
fn unexpected_and_trailing_are_at_the_byte_refused() {
    for (document, error, position) in UNEXPECTED {
        assert_eq!(placed(document, LIMITS, &[]).last(), Some(&(Event::Failed(error), position)), "{document:?}");
    }
}
#[test]
fn too_deep_is_at_the_bracket_that_opens_past_the_depth() {
    assert_eq!(placed(b"[\n[\n[\n[\n[", LIMITS, &[]).last(), Some(&(Event::Failed(Error::TooDeep), at(8, 5, 1))));
}
const NUMBERS: [(&[u8], Error, Position); 5] = [
    (b"01", Error::Number, at(0, 1, 1)),
    (b" [1.e5]", Error::Number, at(2, 1, 3)),
    (b"[-\n]", Error::Number, at(1, 1, 2)),
    (b"-", Error::Number, at(0, 1, 1)),
    (b"[123456789]", Error::NumberTooLong, at(1, 1, 2)),
];
#[test]
fn a_bad_or_too_long_number_is_at_its_first_byte() {
    for (document, error, position) in NUMBERS {
        assert_eq!(placed(document, LIMITS, &[]).last(), Some(&(Event::Failed(error), position)), "{document:?}");
    }
}
const LONG: [(&[u8], Limits, Position); 2] = [
    (b"[1,2]", Limits { length: 4, ..LIMITS }, at(4, 1, 5)),
    (b"\"abcdef\"", Limits { length: 6, ..LIMITS }, at(5, 1, 6)),
];
#[test]
fn too_long_is_at_the_end_of_what_was_read() {
    for (document, limits, position) in LONG {
        assert_eq!(
            placed(document, limits, &[]).last(),
            Some(&(Event::Failed(Error::TooLong), position)),
            "{document:?}"
        );
    }
}
const TRUNCATED: [(&[u8], Position); 4] =
    [(b"[1", at(2, 1, 3)), (b"[1\n", at(3, 2, 1)), (b"tru", at(1, 1, 2)), (b"\"abcdef", at(5, 1, 6))];
#[test]
fn truncated_is_where_the_next_byte_would_be() {
    for (document, position) in TRUNCATED {
        assert_eq!(
            placed(document, LIMITS, &[]).last(),
            Some(&(Event::Failed(Error::Truncated), position)),
            "{document:?}"
        );
    }
}
#[test]
fn a_stream_failure_is_at_the_end_of_what_was_read() {
    let mut machine = Machine::new(LIMITS);
    machine.down(Request::Next);
    machine.bytes(b"[");
    machine.down(Request::Next);
    machine.bytes(b"\n");
    let fault = skein_lib::stream::Fault::Reset;
    assert_eq!(machine.up(Up::Failed(fault)).0, Some(Event::Failed(Error::Stream(fault))));
    assert_eq!(machine.tokenizer.position(), at(2, 2, 1));
}
const DONE: [(&[u8], Position); 3] = [(b"1", at(1, 1, 2)), (b"[1] ", at(4, 1, 5)), (b"[1]\n\n", at(5, 3, 1))];
#[test]
fn done_is_at_the_end_of_what_was_read() {
    for (document, position) in DONE {
        assert_eq!(placed(document, LIMITS, &[]).last(), Some(&(Event::Done, position)), "{document:?}");
    }
}

fn string_failure_positions(rows: &[(&[u8], Error, Position)], limits: Limits) {
    for &(document, error, position) in rows {
        for chunk in [1, 2, 3, 4, 7] {
            assert_eq!(
                placed(document, Limits { chunk, ..limits }, &[]).last(),
                Some(&(Event::Failed(error), position)),
                "{document:?}, chunk {chunk}"
            );
        }
    }
}
const ESCAPES: [(&[u8], Error, Position); 3] = [
    (b"\"ab\\x\"", Error::Escape, at(3, 1, 4)),
    (b"\"\\u12G4\"", Error::Escape, at(1, 1, 2)),
    (b"{\"k\\q\":1}", Error::Escape, at(3, 1, 4)),
];
#[test]
fn a_bad_escape_is_at_its_backslash() {
    string_failure_positions(&ESCAPES, LIMITS);
}
const SURROGATES: [(&[u8], Error, Position); 4] = [
    (b"\"a\\uDC00\"", Error::Surrogate, at(2, 1, 3)),
    (b"\"\\uD800\"", Error::Surrogate, at(1, 1, 2)),
    (b"\"x\\uD800\\u0041\"", Error::Surrogate, at(2, 1, 3)),
    (b"\"\\uDC00\\uD800\"", Error::Surrogate, at(1, 1, 2)),
];
#[test]
fn a_lone_surrogate_is_at_its_half_s_backslash() {
    string_failure_positions(&SURROGATES, LIMITS);
}
const INVALID_UTF8: [(&[u8], Error, Position); 3] = [
    (b"\"ab\xff\"", Error::Utf8, at(3, 1, 4)),
    (b"\"abc\xe2\x28\xa1\"", Error::Utf8, at(4, 1, 5)),
    (b"\"\xc3\"", Error::Utf8, at(1, 1, 2)),
];
#[test]
fn invalid_utf8_is_at_the_first_byte_of_its_character() {
    string_failure_positions(&INVALID_UTF8, LIMITS);
}
const CONTROL: [(&[u8], Error, Position); 1] = [(b"[\n\"a\x01\"]", Error::Control, at(4, 2, 3))];
#[test]
fn a_control_character_is_at_its_own_byte() {
    string_failure_positions(&CONTROL, LIMITS);
}
const SHORT_STRING: Limits = Limits { string: 4, ..LIMITS };
const LONG_STRINGS: [(&[u8], Error, Position); 4] = [
    (b"\"abcde\"", Error::StringTooLong, at(5, 1, 6)),
    (b"\"a\xF0\x9F\x98\x80\"", Error::StringTooLong, at(2, 1, 3)),
    (b"\"abcd\\n\"", Error::StringTooLong, at(5, 1, 6)),
    (b"{\"abcde\":1}", Error::StringTooLong, at(6, 1, 7)),
];
#[test]
fn a_string_past_its_limit_is_at_the_character_or_escape() {
    string_failure_positions(&LONG_STRINGS, SHORT_STRING);
}
const PAIRS: [(&[u8], Error, Position); 2] =
    [(b"\"\\ud83d\\u12G4\"", Error::Escape, at(1, 1, 2)), (b"\"a\\ud83d\\ude00\"", Error::StringTooLong, at(2, 1, 3))];
#[test]
fn a_surrogate_pair_is_one_character_at_its_high_half_s_backslash() {
    string_failure_positions(&PAIRS, SHORT_STRING);
}
