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
    assert_eq!(machine.up(Up::End).0, Some(Event::Failed(tokenizer::Error::Truncated)));
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
