//! The writer (json.md, 4): measured, then written into exactly that
//! length; escaping; refusals of what the caller's data gets wrong; and
//! assertions on what its code gets wrong.

use alloc::boxed::Box;

use super::{key, number, string, tokens};
use crate::Token;
use crate::writer::{self, Encoder, Limits, Refusal};

const WRITER: Limits = Limits { depth: 4, length: 256 };

/// Both passes of `encode`: the document, or why it was refused.
fn encoded(limits: Limits, encode: fn(&mut Encoder)) -> Result<Box<[u8]>, Refusal> {
    let mut measure = Encoder::measure(&limits);
    encode(&mut measure);
    let len = measure.measured()?;
    let mut write = Encoder::write(len, &limits);
    encode(&mut write);
    let document = write.finish();
    assert_eq!(document.len(), usize::try_from(len).unwrap(), "written at the length measured");
    Ok(document)
}

fn written(encode: fn(&mut Encoder)) -> Box<[u8]> {
    encoded(WRITER, encode).unwrap()
}

#[test]
fn a_document_is_measured_then_written_compact() {
    let document = written(|json| {
        json.object_start();
        json.key(b"name");
        json.string(b"read");
        json.key(b"input");
        json.object_start();
        json.key(b"path");
        json.string(b"src/lib.rs");
        json.key(b"lines");
        json.array_start();
        json.unsigned(1);
        json.signed(-40);
        json.number(b"2.5e-3");
        json.array_end();
        json.object_end();
        json.key(b"flags");
        json.array_start();
        json.boolean(true);
        json.boolean(false);
        json.null();
        json.array_end();
        json.key(b"empty");
        json.object_start();
        json.object_end();
        json.object_end();
    });
    assert_eq!(
        &*document,
        br#"{"name":"read","input":{"path":"src/lib.rs","lines":[1,-40,2.5e-3]},"flags":[true,false,null],"empty":{}}"#
    );
}

#[test]
fn a_document_may_be_a_single_scalar() {
    assert_eq!(&*written(|json| json.string(b"")), b"\"\"");
    assert_eq!(&*written(Encoder::null), b"null");
    assert_eq!(&*written(|json| json.signed(i64::MIN)), b"-9223372036854775808");
    assert_eq!(&*written(|json| json.unsigned(u64::MAX)), b"18446744073709551615");
    assert_eq!(&*written(|json| json.signed(0)), b"0");
}

#[test]
fn strings_escape_quotes_backslashes_and_control_characters() {
    assert_eq!(&*written(|json| json.string(b"\"\\/\x08\x0C\n\r\t")), br#""\"\\/\b\f\n\r\t""#);
    assert_eq!(&*written(|json| json.string(b"\x00\x01\x1F\x7F")), b"\"\\u0000\\u0001\\u001f\x7F\"");
    assert_eq!(&*written(|json| json.string("é€😀".as_bytes())), "\"é€😀\"".as_bytes(), "UTF-8 as it is");
    assert_eq!(&*written(|json| json.string(b"ab\ncd\"")), br#""ab\ncd\"""#);
    let document = written(|json| {
        json.object_start();
        json.key(b"a\"b");
        json.null();
        json.object_end();
    });
    assert_eq!(&*document, br#"{"a\"b":null}"#, "keys are escaped too");
}

#[test]
fn adjacent_escapes_fit_exactly_at_their_compact_length() {
    for (text, expected) in [
        (b"\x00\n\\\"\x1f".as_slice(), br#""\u0000\n\\\"\u001f""#.as_slice()),
        (b"plain\x00\n\\\"\x1f", br#""plain\u0000\n\\\"\u001f""#),
        (b"\x00\n\\\"\x1fplain", br#""\u0000\n\\\"\u001fplain""#),
    ] {
        let length = u32::try_from(expected.len()).unwrap();
        let limits = Limits { length, ..WRITER };
        let mut measure = Encoder::measure(&limits);
        measure.string(text);
        assert_eq!(measure.measured(), Ok(length));
        let mut write = Encoder::write(length, &limits);
        write.string(text);
        assert_eq!(write.finish().as_ref(), expected);
        assert_eq!(tokens(expected), [string(text)]);
        let mut short = Encoder::measure(&Limits { length: length - 1, ..limits });
        short.string(text);
        assert_eq!(short.measured(), Err(Refusal::TooLong));
    }
}

#[test]
#[should_panic(expected = "a document measured first is never refused")]
fn a_writing_pass_with_invalid_utf8_is_a_bug() {
    let mut json = Encoder::write(4, &WRITER);
    json.string(b"\xC0\x80");
}

#[test]
fn what_the_writer_writes_the_tokenizer_reads_back() {
    let document = written(|json| {
        json.array_start();
        json.string(b"\x00\x1F\"\\\n\x7F\xC3\xA9");
        json.object_start();
        json.key(b"\t");
        json.number(b"-0.5E+2");
        json.object_end();
        json.array_end();
    });
    assert_eq!(
        tokens(&document),
        [
            Token::ArrayStart,
            string(b"\x00\x1F\"\\\n\x7F\xC3\xA9"),
            Token::ObjectStart,
            key(b"\t"),
            number(b"-0.5E+2"),
            Token::ObjectEnd,
            Token::ArrayEnd,
        ]
    );
}

#[test]
fn tokens_are_written_as_they_were_read() {
    let document = br#"{"a":[1,true,false,null,"x",{},[]],"b":-2e3}"#;
    let read = tokens(document);
    let mut measure = Encoder::measure(&WRITER);
    for token in &read {
        measure.token(token);
    }
    let len = measure.measured().unwrap();
    let mut write = Encoder::write(len, &WRITER);
    for token in &read {
        write.token(token);
    }
    assert_eq!(&*write.finish(), document);
}

#[test]
fn text_that_is_not_utf8_is_refused() {
    assert_eq!(encoded(WRITER, |json| json.string(b"\xFF")), Err(Refusal::Text));
    assert_eq!(encoded(WRITER, |json| json.string(b"\xC3")), Err(Refusal::Text), "cut short");
    assert_eq!(encoded(WRITER, |json| json.string(b"\xED\xA0\x80")), Err(Refusal::Text), "a surrogate");
    let refused = encoded(WRITER, |json| {
        json.object_start();
        json.key(b"\xC0\x80");
        json.null();
        json.object_end();
    });
    assert_eq!(refused, Err(Refusal::Text));
}

#[test]
fn a_number_must_be_one() {
    for bad in [&b""[..], b"01", b"1.", b"-", b"+1", b".5", b"1e", b"NaN", b"0x1", b" 1"] {
        let mut measure = Encoder::measure(&WRITER);
        measure.number(bad);
        assert_eq!(measure.measured(), Err(Refusal::Number), "{}", bad.escape_ascii());
    }
}

#[test]
fn a_document_past_the_limits_is_refused_when_measured() {
    let short = Limits { depth: 2, length: 7 };
    assert_eq!(encoded(short, |json| json.string(b"abcde")).as_deref(), Ok(&b"\"abcde\""[..]), "seven fits");
    assert_eq!(encoded(short, |json| json.string(b"abcdef")), Err(Refusal::TooLong));
    assert_eq!(encoded(short, |json| json.string(b"\n\n\n")), Err(Refusal::TooLong), "measured escaped");
    let deep = encoded(short, |json| {
        json.array_start();
        json.array_start();
        json.array_start();
        json.array_end();
        json.array_end();
        json.array_end();
    });
    assert_eq!(deep, Err(Refusal::TooDeep));
    let first = encoded(short, |json| {
        json.array_start();
        json.string(b"\xFF");
        json.string(b"a long string, past the length");
        json.array_end();
    });
    assert_eq!(first, Err(Refusal::Text), "the first refusal met, and the length last");
}

#[test]
fn the_worst_case_is_the_stack_and_the_document() {
    assert_eq!(writer::worst_case(&Limits { depth: 8, length: 1000 }), Some(8 + 1000));
}

#[test]
#[should_panic(expected = "a key is an object's")]
fn a_key_outside_an_object_is_a_bug() {
    let mut json = Encoder::measure(&WRITER);
    json.array_start();
    json.key(b"a");
}

#[test]
#[should_panic(expected = "a value in an object follows its key")]
fn a_value_without_its_key_is_a_bug() {
    let mut json = Encoder::measure(&WRITER);
    json.object_start();
    json.null();
}

#[test]
#[should_panic(expected = "a document holds one value")]
fn a_second_value_is_a_bug() {
    let mut json = Encoder::measure(&WRITER);
    json.null();
    json.null();
}

#[test]
#[should_panic(expected = "an end is the innermost container's")]
fn an_end_of_the_wrong_container_is_a_bug() {
    let mut json = Encoder::measure(&WRITER);
    json.array_start();
    json.object_end();
}

#[test]
#[should_panic(expected = "a document is measured whole")]
fn measuring_half_a_document_is_a_bug() {
    let mut json = Encoder::measure(&WRITER);
    json.array_start();
    json.measured().unwrap();
}

#[test]
#[should_panic(expected = "a document measured first fits")]
fn a_writing_pass_longer_than_its_measure_is_a_bug() {
    let mut json = Encoder::write(3, &WRITER);
    json.string(b"ab");
}

#[test]
#[should_panic(expected = "a writer is finished full")]
fn a_writing_pass_shorter_than_its_measure_is_a_bug() {
    let mut json = Encoder::write(5, &WRITER);
    json.string(b"a");
    drop(json.finish());
}

#[test]
fn borrowed_json_is_grammar_checked_and_writes_compact_spelling() {
    let limits = crate::tokenizer::Limits { depth: 4, string: 0, number: 32, chunk: 3, length: 256 };
    for (valid, compact) in [
        (br#" {"a":[null,true,false,-2e3,"\u0061"]} "#.as_slice(), br#"{"a":[null,true,false,-2e3,"a"]}"#.as_slice()),
        (b"null".as_slice(), b"null".as_slice()),
        (b"\"longer than the chunk\"".as_slice(), b"\"longer than the chunk\"".as_slice()),
        (
            br#""\u0000\u0008\u000c\u000A\u000d\u0009\/\u0022\u005c\ud83d\ude00""#.as_slice(),
            "\"\\u0000\\b\\f\\n\\r\\t/\\\"\\\\😀\"".as_bytes(),
        ),
    ] {
        assert_eq!(crate::tokenizer::validate(valid, &limits), Ok(()));
        let mut measure = Encoder::measure(&WRITER);
        measure.array_start();
        measure.json_text(valid, &limits).unwrap();
        measure.array_end();
        let length = measure.measured().unwrap();
        let mut write = Encoder::write(length, &WRITER);
        write.array_start();
        write.json_text(valid, &limits).unwrap();
        write.array_end();
        let written = write.finish();
        assert_eq!(written.first(), Some(&b'['));
        assert_eq!(written.last(), Some(&b']'));
        assert_eq!(&written[1..written.len() - 1], compact);
    }
    for bad in
        [b"".as_slice(), b"{", b"[1,]", b"{} {}", b"01", b"{\"x\":\"\\uD800\"}", b"\"\xFF\"", b"{\"x\":\"\x01\"}"]
    {
        assert!(crate::tokenizer::validate(bad, &limits).is_err(), "{bad:?}");
    }
    assert_eq!(crate::tokenizer::validate(b"[[[[[]]]]]", &limits), Err(crate::tokenizer::Error::TooDeep));
    let mut measure = Encoder::measure(&WRITER);
    measure.array_start();
    measure.json_text(b"[[[[]]]]", &limits).unwrap();
    measure.array_end();
    assert_eq!(measure.measured(), Err(Refusal::TooDeep), "embedding counts the open parent");
    assert_eq!(written(|out| out.string_parts(&[b"Error: ", b"a\n\"b"])).as_ref(), br#""Error: a\n\"b""#);
}
