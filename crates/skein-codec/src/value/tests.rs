//! The checks and accessors on both sides of their bounds (codec.md, 8.3).

#![expect(clippy::disallowed_macros, reason = "tests format Debug to check that content is redacted")]

use super::{Items, Misread, Node, Part, Path, Problem, Refusal, Value};
use crate::Reason;
use crate::schema::{DeclIx, FieldIx, Shape, Span, VariantIx, parse};

// Room for the accessor fixture and every smaller refusal fixture.
const TEST_ROOM: u32 = 32;

type Check = (&'static [u8], &'static [u8], Part, Reason);

fn check(source: &[u8], root: &[u8], cases: &[Check]) {
    let schema = parse(source).expect("valid test schema");
    let root = schema.find(root).expect("test root");
    let mut value = Value::with_room(TEST_ROOM);
    for &(bad, twin, part, reason) in cases {
        value.decode(&schema, root, twin.into()).expect("nearest twin decodes");
        assert_eq!(
            value.decode(&schema, root, bad.into()),
            Err(Refusal::Problem(Problem { path: Path { decl: root, part }, reason })),
            "{bad:?}"
        );
        assert_eq!(value.bytes(), b"", "a refused decode leaves no bytes");
        assert_eq!(value.span(Node(0)), Err(Misread { node: Node(0) }));
    }
}

#[test]
fn versions_and_tags_refuse_at_their_own_paths() {
    check(
        b"family f 7\nversioned record R {}",
        b"R",
        &[
            (b"\0", b"\0\x07", Part::Version, Reason::Short),
            (b"\0\x08", b"\0\x07", Part::Version, Reason::Version),
            (b"\0\x07\0", b"\0\x07", Part::Tail, Reason::Trailing),
        ],
    );
    check(
        b"family f 7\nenum E { a, b }",
        b"E",
        &[
            (b"", b"\x01", Part::Tag, Reason::Short),
            (b"\x02", b"\x01", Part::Tag, Reason::Tag),
            (b"\x01\0", b"\x01", Part::Tag, Reason::Trailing),
        ],
    );
}

#[test]
fn scalar_refusals_name_the_field_through_its_containers() {
    let field = Part::Field(FieldIx::new(0));
    for ty in ["u8", "u16", "u32", "u64", "duration", "fixed 2"] {
        let source = format!("family f 1\nrecord R {{ x: {ty} }}");
        let bytes = match ty {
            "u8" => &b"\0"[..],
            "u16" | "fixed 2" => b"\0\0",
            "u32" => b"\0\0\0\0",
            _ => b"\0\0\0\0\0\0\0\0",
        };
        check(source.as_bytes(), b"R", &[(b"", bytes, field, Reason::Short)]);
    }
    check(
        b"family f 1\nrecord R { x: bool }",
        b"R",
        &[(b"", b"\0", field, Reason::Short), (b"\x02", b"\x01", field, Reason::Bool)],
    );
    check(
        b"family f 1\nrecord R { x: option option bool }",
        b"R",
        &[
            (b"", b"\0", field, Reason::Short),
            (b"\x02", b"\x01\0", field, Reason::Tag),
            (b"\x01", b"\x01\0", field, Reason::Short),
            (b"\x01\x02", b"\x01\x01\0", field, Reason::Tag),
            (b"\x01\x01\x02", b"\x01\x01\x01", field, Reason::Bool),
        ],
    );
    check(
        b"family f 1\nrecord R { x: list 1 option bool }",
        b"R",
        &[(b"\0\0\0\x01\x01\x02", b"\0\0\0\x01\x01\x01", field, Reason::Bool)],
    );
}

#[test]
fn lengths_refuse_before_content_and_text_checks() {
    let field = Part::Field(FieldIx::new(0));
    for ty in ["bytes 1", "secret 1", "text 1"] {
        let source = format!("family f 1\nrecord R {{ x: {ty} }}");
        check(
            source.as_bytes(),
            b"R",
            &[
                (b"\0\0\0", b"\0\0\0\0", field, Reason::Short),
                (b"\0\0\0\x02", b"\0\0\0\x01a", field, Reason::Bound),
                (b"\0\0\0\x01", b"\0\0\0\x01a", field, Reason::Short),
            ],
        );
    }
    check(b"family f 1\nrecord R { x: text 1 }", b"R", &[(b"\0\0\0\x01\xff", b"\0\0\0\x01a", field, Reason::Utf8)]);
}

#[test]
fn counts_check_bound_and_minimum_before_reading_items() {
    let field = Part::Field(FieldIx::new(0));
    check(
        b"family f 1\nrecord R { x: list 1 u16 }",
        b"R",
        &[
            (b"\0\0\0", b"\0\0\0\0", field, Reason::Short),
            (b"\0\0\0\x02", b"\0\0\0\x01\0\0", field, Reason::Bound),
            (b"\0\0\0\x01\0", b"\0\0\0\x01\0\0", field, Reason::Short),
        ],
    );
    check(
        b"family f 1\nversioned record Inner {}\nrecord R { x: list 1 Inner }",
        b"R",
        &[(b"\0\0\0\x01\x02", b"\0\0\0\x01\0\x01", field, Reason::Short)],
    );
}

#[test]
fn nested_records_check_their_own_version() {
    let schema = parse(b"family f 7\nversioned record Inner {}\nrecord R { x: Inner }").expect("schema");
    let mut value = Value::with_room(2);
    let root = schema.find(b"R").expect("root");
    let inner = schema.find(b"Inner").expect("inner");
    for (bad, reason) in [(&b"\0"[..], Reason::Short), (&b"\0\x08"[..], Reason::Version)] {
        value.decode(&schema, root, b"\0\x07".as_slice().into()).expect("nested version matches");
        assert_eq!(
            value.decode(&schema, root, bad.into()),
            Err(Refusal::Problem(Problem { path: Path { decl: inner, part: Part::Version }, reason }))
        );
    }
}

#[test]
fn room_counts_empty_records_and_precedes_their_bytes() {
    // Root, list and four empty records take six nodes, despite only four bytes.
    const EXACT_ROOM: u32 = 6;
    const SHORT_ROOM: u32 = 5;
    let schema = parse(b"family f 1\nrecord Blank {}\nrecord R { x: list 4 Blank }").expect("schema");
    let root = schema.find(b"R").expect("root");
    let mut value = Value::with_room(EXACT_ROOM);
    value.decode(&schema, root, b"\0\0\0\x04".as_slice().into()).expect("exact room");
    let mut short = Value::with_room(SHORT_ROOM);
    assert_eq!(short.decode(&schema, root, b"\0\0\0\x04".as_slice().into()), Err(Refusal::Room));
    let mut none = Value::with_room(0);
    assert_eq!(none.decode(&schema, root, Box::default()), Err(Refusal::Room));
    let schema = parse(b"family f 1\nrecord R { x: bool }").expect("schema");
    let mut root_only = Value::with_room(1);
    assert_eq!(root_only.decode(&schema, DeclIx::new(0), b"\x02".as_slice().into()), Err(Refusal::Room));
}

fn fields(schema: &crate::schema::Schema, decl: DeclIx) -> FieldIx {
    match schema.declaration(decl).shape {
        Shape::Record { first, .. } => first,
        Shape::Enum { .. } => unreachable!("test record"),
    }
}

#[test]
fn accessors_read_content_and_hop_over_nested_siblings() {
    let schema = parse(b"family f 1\nrecord Blank {}\nenum E { a, b: Blank }\nrecord Other { foreign: u8 }\nrecord R { xs: list 2 option u16 e: E n: option u8 no: option u8 b: bool f: fixed 2 raw: bytes 2 text: text 2 key: secret 2 wide: u64 }").expect("schema");
    let root = schema.find(b"R").expect("root");
    let wire = b"\0\0\0\x02\x01\x12\x34\0\x01\x01\xff\0\x01ab\0\0\0\x02cd\0\0\0\x02ef\0\0\0\x02gh\xff\xff\xff\xff\xff\xff\xff\xff";
    let mut value = Value::with_room(TEST_ROOM);
    let node = value.decode(&schema, root, wire.as_slice().into()).expect("record");
    let first = fields(&schema, root).index();
    let field =
        |offset| value.field(node, FieldIx::new(first.checked_add(offset).expect("field index"))).expect("field");
    let xs = field(0);
    let mut items = value.items(xs).expect("list");
    let some = value.next_item(&mut items).expect("first item");
    let uint = value.some(some).expect("option").expect("some");
    assert_eq!(value.uint(uint), Ok(0x1234));
    let absent = value.next_item(&mut items).expect("second item");
    assert_eq!(value.some(absent), Ok(None));
    assert_eq!(value.next_item(&mut items), None);
    assert_eq!(value.tag(field(1)), Ok(VariantIx::new(1)));
    assert_eq!(value.payload(field(1)), Ok(Some(Node(6))));
    assert_eq!(value.uint(value.some(field(2)).expect("option").expect("some")), Ok(255));
    assert_eq!(value.some(field(3)), Ok(None));
    assert_eq!(value.boolean(field(4)), Ok(true));
    assert_eq!(value.content(field(5)), Ok(&b"ab"[..]));
    assert_eq!(value.content(field(6)), Ok(&b"cd"[..]));
    assert_eq!(value.content(field(7)), Ok(&b"ef"[..]));
    assert_eq!(value.expose(field(8)), Ok(&b"gh"[..]));
    assert_eq!(value.uint(field(9)), Ok(u64::MAX));
    assert_eq!(value.span(uint), Ok(Span { start: 5, end: 7 }));
    assert_eq!(value.field(node, FieldIx::new(first.checked_sub(1).expect("earlier field"))), Err(Misread { node }));
    assert_eq!(value.field(node, FieldIx::new(first.checked_add(10).expect("past fields"))), Err(Misread { node }));
    assert_eq!(value.clone(), value);
}

#[test]
fn every_accessor_refuses_wrong_and_absent_nodes() {
    let schema = parse(b"family f 1\nrecord R { b: bool }").expect("schema");
    let mut value = Value::with_room(2);
    let root = value.decode(&schema, DeclIx::new(0), b"\0".as_slice().into()).expect("record");
    let boolean = value.field(root, FieldIx::new(0)).expect("boolean");
    for node in [root, Node(2)] {
        let error = Err(Misread { node });
        assert_eq!(value.items(node), error);
        assert_eq!(value.some(node), Err(Misread { node }));
        assert_eq!(value.tag(node), Err(Misread { node }));
        assert_eq!(value.payload(node), Err(Misread { node }));
        assert_eq!(value.uint(node), Err(Misread { node }));
        assert_eq!(value.boolean(node), Err(Misread { node }));
        assert_eq!(value.content(node), Err(Misread { node }));
        assert_eq!(value.expose(node), Err(Misread { node }));
    }
    for node in [boolean, Node(2)] {
        assert_eq!(value.field(node, FieldIx::new(0)), Err(Misread { node }));
    }
    assert_eq!(value.span(Node(2)), Err(Misread { node: Node(2) }));
    let mut absent = Items { next: 2, left: 1 };
    assert_eq!(value.next_item(&mut absent), None);
}

#[test]
fn take_moves_the_secret_buffer_and_empties_the_index() {
    let schema = parse(b"family f 1\nrecord R { n: u8 }").expect("schema");
    let wire: Box<[u8]> = b"a".as_slice().into();
    let pointer = wire.as_ptr();
    let mut value = Value::with_room(2);
    let root = value.decode(&schema, DeclIx::new(0), wire).expect("record");
    let bytes = value.take();
    assert_eq!(bytes.expose(), b"a");
    assert_eq!(bytes.expose().as_ptr(), pointer, "take moves the owned buffer");
    assert_eq!(value.bytes(), b"");
    assert_eq!(value.span(root), Err(Misread { node: root }));
}

#[test]
fn debug_redacts_secret_and_text_content() {
    let schema = parse(b"family f 1\nrecord R { key: secret 3 words: text 3 }").expect("schema");
    let mut value = Value::with_room(3);
    value.decode(&schema, DeclIx::new(0), b"\0\0\0\x03xyz\0\0\0\x03abc".as_slice().into()).expect("record");
    let debug = format!("{value:?}");
    assert!(!debug.contains("120, 121, 122"), "secret bytes are redacted");
    assert!(!debug.contains("97, 98, 99"), "text bytes are redacted");
}
