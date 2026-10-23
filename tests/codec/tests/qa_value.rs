//! Black-box generic values and the generated oracle (codec.md, section 8).

#![cfg(test)]

#[path = "support/generated.rs"]
mod generated;
#[path = "support/generic.rs"]
mod generic;

use core::fmt::Write;
use generated::{Decoded, Root};
use skein_codec::Reason;
use skein_codec::schema::{self, DeclIx, FieldIx, Schema, Span, VariantIx};
use skein_codec::value::{Misread, Node, Refusal, Value};
use skein_lib::Rng;

fn decode(text: &str, name: &[u8], bytes: &[u8], capacity: u32) -> (Schema, Value, Node) {
    let schema = schema::parse(text.as_bytes()).expect("QA schema");
    let decl = schema.find(name).expect("root declaration");
    let mut value = Value::with_room(capacity);
    let root = value.decode(&schema, decl, bytes.into()).expect("canonical value fits room");
    (schema, value, root)
}

fn problem(text: &str, name: &[u8], bytes: &[u8], path: &str, reason: Reason) {
    let schema = schema::parse(text.as_bytes()).expect("QA schema");
    let decl = schema.find(name).expect("root declaration");
    let mut value = Value::with_room(generic::ROOM);
    let refusal = value.decode(&schema, decl, bytes.into()).expect_err("malformed bytes");
    let Refusal::Problem(problem) = refusal else { unreachable!("room fits fixture") };
    assert_eq!(
        (generic::path_name(&schema, problem.path), problem.reason),
        (path.to_owned(), reason),
        "{text}; bytes={bytes:02x?}"
    );
}

#[test]
fn version_checks_precede_field_checks() {
    let text = "family qa 1 versioned record V { flag: bool }";
    for (bytes, path, reason) in [
        (b"".as_slice(), "VVersion", Reason::Short),
        (b"\x00", "VVersion", Reason::Short),
        (b"\x00\x02\x02", "VVersion", Reason::Version),
        (b"\x00\x01\x02", "VFlag", Reason::Bool),
    ] {
        problem(text, b"V", bytes, path, reason);
    }
}

#[test]
fn root_trailing_bytes_name_record_tail_or_enum_tag() {
    for (text, name, bytes, path) in [
        ("family qa 1 record R {}", b"R".as_slice(), b"\x00".as_slice(), "RTail"),
        ("family qa 1 enum E { a }", b"E", b"\x00\x01", "ETag"),
    ] {
        problem(text, name, bytes, path, Reason::Trailing);
    }
}

#[test]
fn length_bounds_precede_short_content_and_invalid_utf8() {
    for ty in ["bytes 2", "secret 2", "text 2"] {
        let text = format!("family qa 1 record R {{ data: {ty} }}");
        for (bytes, reason) in [
            (b"\x00\x00\x00".as_slice(), Reason::Short),
            (b"\x00\x00\x00\x03\xff", Reason::Bound),
            (b"\x00\x00\x00\x02\xff", Reason::Short),
        ] {
            problem(&text, b"R", bytes, "RData", reason);
        }
    }
}

#[test]
fn length_fields_accept_empty_and_exact_ceiling_content() {
    for ty in ["bytes 2", "secret 2", "text 2"] {
        let text = format!("family qa 1 record R {{ data: {ty} }}");
        for bytes in [b"\x00\x00\x00\x00".as_slice(), b"\x00\x00\x00\x02xy"] {
            let (_, value, _) = decode(&text, b"R", bytes, generic::ROOM);
            assert_eq!(value.bytes(), bytes);
        }
    }
}

#[test]
fn integer_widths_and_duration_read_big_endian_maxima() {
    // u8 + u16 + u32 + u64 + duration + fixed 0 = 23 bytes.
    const WIDTH: usize = 23;
    let text = "family qa 1 record R { a: u8 b: u16 c: u32 d: u64 e: duration f: fixed 0 }";
    let (_, value, root) = decode(text, b"R", &[0xff; WIDTH], generic::ROOM);
    for (index, maximum) in [(0, 255), (1, 65_535), (2, 4_294_967_295), (3, u64::MAX), (4, u64::MAX)] {
        assert_eq!(value.uint(value.field(root, FieldIx::new(index)).expect("scalar field")), Ok(maximum));
    }
    let fixed = value.field(root, FieldIx::new(5)).expect("zero width fixed");
    assert_eq!(value.content(fixed), Ok(b"".as_slice()));
    assert_eq!(value.span(fixed), Ok(Span { start: 23, end: 23 }));
}

#[test]
fn enum_tags_use_their_own_variant_table_offset() {
    let text = "family qa 1 enum Before { x y } enum E { a b }";
    let (_, value, root) = decode(text, b"E", b"\x01", generic::ROOM);
    assert_eq!(value.tag(root), Ok(VariantIx::new(3)));
    assert_eq!(value.payload(root), Ok(None));
}

#[test]
fn enumeration_tag_255_is_a_valid_last_variant() {
    // codec.md, 7.2: enums may use every u8 tag.
    const TAGS: u32 = 256;
    let mut variants = String::new();
    for index in 0..TAGS {
        write!(variants, "v{index} ").expect("String write");
    }
    let text = format!("family qa 1 enum E {{ {variants} }}");
    let (_, value, root) = decode(&text, b"E", b"\xff", generic::ROOM);
    assert_eq!(value.tag(root), Ok(VariantIx::new(255)));
}

#[test]
fn text_refuses_noncanonical_unicode_encodings() {
    let text = "family qa 1 record R { data: text 4 }";
    for bytes in [
        b"\x00\x00\x00\x02\xc0\x80".as_slice(),
        b"\x00\x00\x00\x03\xed\xa0\x80",
        b"\x00\x00\x00\x04\xf4\x90\x80\x80",
        b"\x00\x00\x00\x01\x80",
        b"\x00\x00\x00\x02\xe2\x82",
    ] {
        problem(text, b"R", bytes, "RData", Reason::Utf8);
    }
}

#[test]
fn text_accepts_multibyte_content_at_its_byte_bound() {
    let bytes = b"\x00\x00\x00\x04\xf4\x8f\xbf\xbf";
    let (_, value, root) = decode("family qa 1 record R { data: text 4 }", b"R", bytes, generic::ROOM);
    let field = value.field(root, FieldIx::new(0)).expect("text field");
    assert_eq!(value.content(field), Ok(b"\xf4\x8f\xbf\xbf".as_slice()));
    assert_eq!(value.bytes(), bytes);
}

#[test]
fn list_prechecks_precede_invalid_item_bytes() {
    for (text, bytes, reason) in [
        ("family qa 1 record R { xs: list 2 bool }", b"\x00\x00\x00\x03\x02".as_slice(), Reason::Bound),
        ("family qa 1 record R { xs: list 2 bool }", b"\x00\x00\x00\x02\x02", Reason::Short),
        ("family qa 1 record R { xs: list 2 option bool }", b"\x00\x00\x00\x02\x02", Reason::Short),
        ("family qa 1 record R { xs: list 2 option bool }", b"\x00\x00\x00\x01\x02", Reason::Tag),
        ("family qa 1 record R { xs: list 2 option bool }", b"\x00\x00\x00\x01\x01", Reason::Short),
    ] {
        problem(text, b"R", bytes, "RXs", reason);
    }
}

#[test]
fn named_items_report_their_own_field_paths() {
    let text = "family qa 1 record Inner { flag: bool } record R { xs: list 2 option Inner }";
    problem(text, b"R", b"\x00\x00\x00\x01\x01\x02", "InnerFlag", Reason::Bool);
}

#[test]
fn nested_versions_and_enum_tags_name_their_declarations() {
    let text = "family qa 1 versioned record V { flag: bool } enum E { v: V } record R { x: option E }";
    for (bytes, path, reason) in [
        (b"\x01\xff".as_slice(), "ETag", Reason::Tag),
        (b"\x01\x00\x00", "VVersion", Reason::Short),
        (b"\x01\x00\x00\x02", "VVersion", Reason::Version),
        (b"\x01\x00\x00\x01\x02", "VFlag", Reason::Bool),
    ] {
        problem(text, b"R", bytes, path, reason);
    }
}

const ZERO_WIDTH: &str = "family qa 1 record Blank {} record R { xs: list 3 Blank tail: option u8 }";
const ZERO_BYTES: &[u8] = b"\x00\x00\x00\x03\x00";
// R + list + three Blank entries + absent option (codec.md, 8).
const ZERO_NODES: u32 = 6;
// ZERO_WIDTH's list bound is three empty records.
const ZERO_ITEMS: u32 = 3;

#[test]
fn zero_byte_items_consume_nodes_at_the_exact_room() {
    let (_, value, root) = decode(ZERO_WIDTH, b"R", ZERO_BYTES, ZERO_NODES);
    let list = value.field(root, FieldIx::new(0)).expect("list field");
    let mut items = value.items(list).expect("list");
    for _ in 0..ZERO_ITEMS {
        let child = value.next_item(&mut items).expect("zero byte item");
        assert_eq!(value.span(child), Ok(Span { start: 4, end: 4 }));
    }
    assert_eq!(value.next_item(&mut items), None);
    assert_eq!(value.next_item(&mut items), None);
    assert_eq!(value.span(root), Ok(Span { start: 0, end: 5 }));
}

#[test]
fn zero_byte_items_refuse_one_node_below_room() {
    let schema = schema::parse(ZERO_WIDTH.as_bytes()).expect("zero width schema");
    let mut value = Value::with_room(ZERO_NODES.checked_sub(1).expect("one node below"));
    assert_eq!(value.decode(&schema, schema.find(b"R").expect("root"), ZERO_BYTES.into()), Err(Refusal::Room));
}

#[test]
fn room_is_checked_before_entering_a_nodes_bytes() {
    let schema = schema::parse(b"family qa 1 record R { flag: bool }").expect("bool schema");
    let root = schema.find(b"R").expect("root");
    for room in [0, 1] {
        let mut value = Value::with_room(room);
        assert_eq!(value.decode(&schema, root, b"\x02".as_slice().into()), Err(Refusal::Room));
    }
}

#[test]
fn held_bytes_are_opaque_even_when_their_contents_are_invalid() {
    let text = "family qa 1 record Item { flag: bool } record R { x: bytes 2 holds other Item y: bytes 2 pieces Item }";
    let bytes = b"\x00\x00\x00\x02\xff\xff\x00\x00\x00\x02\xff\xff";
    let (_, value, root) = decode(text, b"R", bytes, generic::ROOM);
    for index in [1, 2] {
        let node = value.field(root, FieldIx::new(index)).expect("held field");
        assert_eq!(value.content(node), Ok(b"\xff\xff".as_slice()));
    }
}

const TREE: &str =
    "family qa 1 record Atom { n: u16 } enum E { no yes: Atom } record R { xs: list 2 option E after: u8 }";
// Two options: Some(E::Yes(Atom { n: 0x1234 })) and Some(E::No), then after=9.
const TREE_BYTES: &[u8] = b"\x00\x00\x00\x02\x01\x01\x12\x34\x01\x00\x09";

#[test]
fn item_iteration_skips_option_and_payload_subtrees() {
    let (_, value, root) = decode(TREE, b"R", TREE_BYTES, generic::ROOM);
    let list = value.field(root, FieldIx::new(1)).expect("xs");
    let mut items = value.items(list).expect("list");
    let first = value.next_item(&mut items).expect("first option");
    let first_enum = value.some(first).expect("option").expect("present");
    let payload = value.payload(first_enum).expect("enum").expect("Atom");
    let number = value.field(payload, FieldIx::new(0)).expect("n");
    assert_eq!(value.uint(number), Ok(0x1234));
    assert_eq!(value.tag(first_enum), Ok(VariantIx::new(1)));
    assert_eq!(value.span(first), Ok(Span { start: 4, end: 8 }));
    let second = value.next_item(&mut items).expect("second option");
    let second_enum = value.some(second).expect("option").expect("present");
    assert_eq!(value.tag(second_enum), Ok(VariantIx::new(0)));
    assert_eq!(value.payload(second_enum), Ok(None));
    assert_eq!(value.span(second), Ok(Span { start: 8, end: 10 }));
    assert_eq!(value.next_item(&mut items), None);
    assert_eq!(value.uint(value.field(root, FieldIx::new(2)).expect("after")), Ok(9));
}

#[test]
fn foreign_fields_are_not_read_from_a_record() {
    let (_, value, root) = decode(TREE, b"R", TREE_BYTES, generic::ROOM);
    assert_eq!(value.field(root, FieldIx::new(0)), Err(Misread { node: root }));
    assert_eq!(value.field(root, FieldIx::new(u32::MAX)), Err(Misread { node: root }));
}

#[test]
fn scalar_accessors_reject_the_wrong_node_kinds() {
    let text = "family qa 1 record R { n: u8 b: bool raw: bytes 1 key: secret 1 }";
    let bytes = b"\x07\x01\x00\x00\x00\x01x\x00\x00\x00\x01y";
    let (_, value, root) = decode(text, b"R", bytes, generic::ROOM);
    let number = value.field(root, FieldIx::new(0)).expect("n");
    let boolean = value.field(root, FieldIx::new(1)).expect("b");
    let raw = value.field(root, FieldIx::new(2)).expect("raw");
    let secret = value.field(root, FieldIx::new(3)).expect("key");
    assert_eq!(value.uint(number), Ok(7));
    assert_eq!(value.boolean(boolean), Ok(true));
    assert_eq!(value.content(raw), Ok(b"x".as_slice()));
    assert_eq!(value.expose(secret), Ok(b"y".as_slice()));
    assert_eq!(value.boolean(number), Err(Misread { node: number }));
    assert_eq!(value.uint(boolean), Err(Misread { node: boolean }));
    assert_eq!(value.content(secret), Err(Misread { node: secret }));
    assert_eq!(value.expose(raw), Err(Misread { node: raw }));
    assert_eq!(value.some(number), Err(Misread { node: number }));
    assert_eq!(value.tag(number), Err(Misread { node: number }));
    assert_eq!(value.payload(number), Err(Misread { node: number }));
    assert_eq!(value.items(number).expect_err("integer is not a list"), Misread { node: number });
    assert_eq!(value.field(number, FieldIx::new(0)), Err(Misread { node: number }));
}

#[test]
fn take_returns_the_original_allocation_and_empties_the_value() {
    let schema = schema::parse(b"family qa 1 record R { x: u8 }").expect("schema");
    let mut value = Value::with_room(generic::ROOM);
    let bytes: Box<[u8]> = b"\x07".as_slice().into();
    let pointer = bytes.as_ptr();
    let root = value.decode(&schema, DeclIx::new(0), bytes).expect("decode");
    let secret = value.take();
    assert_eq!(secret.expose(), b"\x07");
    assert_eq!(secret.expose().as_ptr(), pointer);
    assert_eq!(value.bytes(), b"");
    assert_eq!(value.span(root), Err(Misread { node: root }));
    assert_eq!(value.take().expose(), b"");
}

#[test]
fn failed_decode_drops_bytes_and_clears_partial_nodes() {
    let schema = schema::parse(b"family qa 1 record R { x: bool y: u8 }").expect("schema");
    let mut value = Value::with_room(generic::ROOM);
    let root = value.decode(&schema, DeclIx::new(0), b"\x01\x07".as_slice().into()).expect("initial value");
    let _refusal = value.decode(&schema, DeclIx::new(0), b"\x01".as_slice().into()).expect_err("short second field");
    assert_eq!(value.bytes(), b"");
    assert_eq!(value.span(root), Err(Misread { node: root }));
    let root = value.decode(&schema, DeclIx::new(0), b"\x00\x08".as_slice().into()).expect("reuse after failure");
    assert_eq!(value.uint(value.field(root, FieldIx::new(1)).expect("y")), Ok(8));
}

#[test]
fn debug_contains_neither_secret_nor_public_field_contents() {
    let text = "family qa 1 record R { text: text 8 key: secret 8 }";
    let (_, value, _) = decode(text, b"R", b"\x00\x00\x00\x08VISIBLE!\x00\x00\x00\x08HIDDEN!!", generic::ROOM);
    let debug = format!("{value:?}");
    for content in ["VISIBLE!", "HIDDEN!!", "86, 73, 83, 73, 66, 76, 69, 33", "72, 73, 68, 68, 69, 78, 33, 33"] {
        assert!(!debug.contains(content), "Debug must show lengths and index only: {debug}");
    }
}

fn compare(schema: &Schema, root: &Root, value: &mut Value, bytes: &[u8]) {
    let decl = schema.find(root.name.as_bytes()).expect("root");
    match ((root.decode)(bytes), value.decode(schema, decl, bytes.into())) {
        (Decoded::Value { debug, bytes: encoded }, Ok(node)) => {
            assert_eq!(value.bytes(), encoded, "{}; input={bytes:02x?}", root.name);
            assert_eq!(generic::render(schema, value, node, decl), debug, "{}; input={bytes:02x?}", root.name);
        }
        (Decoded::Problem { path, reason }, Err(Refusal::Problem(problem))) => {
            assert_eq!(
                (generic::path_name(schema, problem.path), problem.reason),
                (path, reason),
                "{}; input={bytes:02x?}",
                root.name
            );
        }
        (expected, decoded) => {
            unreachable!("{}; input={bytes:02x?}; generated={expected:?}; generic={decoded:?}", root.name);
        }
    }
}

#[test]
fn all_fixture_goldens_match_generated_trees_and_bytes() {
    for family in generated::families() {
        let schema = schema::parse(family.text.as_bytes()).expect("runtime schema");
        let codegen = skein_codegen::parse(family.text).expect("generator schema");
        let mut value = Value::with_room(generic::ROOM);
        for golden in skein_codegen::generate(&codegen).goldens {
            let decl = generic::golden_root(&schema, &golden.name);
            let name = schema.spelled(schema.declaration(decl).name);
            let root = family.roots.iter().find(|root| root.name.as_bytes() == name).expect("generated root");
            compare(&schema, root, &mut value, &golden.bytes);
        }
    }
}

#[test]
fn seeded_draws_cuts_and_boundary_mutations_match_generated_decoders() {
    // Fixed seeds replay; 8 draws per root keep the entire binary below 30 s.
    const SEEDS: core::ops::Range<u64> = 0..8;
    for family in generated::families() {
        let schema = schema::parse(family.text.as_bytes()).expect("runtime schema");
        let codegen = skein_codegen::parse(family.text).expect("generator schema");
        let mut value = Value::with_room(generic::ROOM);
        for root in family.roots {
            for seed in SEEDS {
                let mut rng = Rng::new(seed);
                let bytes = generated::draw(&codegen, root.name, &mut rng);
                compare(&schema, root, &mut value, &bytes);
                for cut in 0..bytes.len() {
                    compare(&schema, root, &mut value, bytes.get(..cut).expect("prefix"));
                }
                for index in 0..bytes.len() {
                    for replacement in [0, 1, 2, 0x7f, 0x80, 0xff] {
                        let mut changed = bytes.clone();
                        *changed.get_mut(index).expect("chosen byte") = replacement;
                        compare(&schema, root, &mut value, &changed);
                    }
                }
                let mut grown = bytes;
                grown.push(0xff);
                compare(&schema, root, &mut value, &grown);
            }
        }
    }
}

#[test]
fn every_possible_single_byte_matches_all_generated_roots() {
    for family in generated::families() {
        let schema = schema::parse(family.text.as_bytes()).expect("runtime schema");
        let mut value = Value::with_room(generic::ROOM);
        for root in family.roots {
            for byte in u8::MIN..=u8::MAX {
                compare(&schema, root, &mut value, &[byte]);
            }
        }
        assert_eq!(schema.spelled(schema.family()), family.name.as_bytes());
    }
}
