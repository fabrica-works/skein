//! Black-box schema checks against codec.md, sections 7.1-7.3.

#![cfg(test)]

use core::fmt::Write;

use skein_codec::schema::{self, DeclIx, Expected, FieldIx, Held, Shape, Type, VariantIx, Wrong};

fn refusal(text: &str, wrong: Wrong, line: u32, token: &str) {
    let error = schema::parse(text.as_bytes()).expect_err("schema must be refused");
    assert_eq!((error.wrong, error.line), (wrong, line), "{text}");
    let start = usize::try_from(error.at.start).expect("span start");
    let end = usize::try_from(error.at.end).expect("span end");
    assert_eq!(text.get(start..end), Some(token), "{text}");
}

#[test]
fn characters_are_checked_before_grammar_and_layout() {
    refusal("family qa 1\nrecord R { x: Missing }\nrecord S {\n@", Wrong::Character, 4, "@");
}

#[test]
fn grammar_is_checked_before_earlier_layout_faults() {
    refusal("family qa 1\nrecord R { x: Missing }\nrecord S { x u8 }", Wrong::Expected(Expected::Colon), 3, "u8");
}

#[test]
fn layout_is_checked_before_rust_names() {
    let text = "family qa 1\nrecord lower { x: Missing }";
    refusal(text, Wrong::Undeclared, 2, "Missing");
    assert_eq!(skein_codegen::parse(text).expect_err("layout before Rust names").line, 2);
}

#[test]
fn comments_hide_arbitrary_bytes_and_count_newlines() {
    let text = b"# \xff\x00@\nfamily qa 65535\r\n# \xfe\nversioned record R {\n x: u16,\n}\n";
    let parsed = schema::parse(text).expect("comments are opaque");
    assert_eq!(parsed.text(), text);
    assert_eq!(parsed.version(), u16::MAX);
    assert_eq!(parsed.declaration(DeclIx::new(0)).line, 4);
    assert_eq!(parsed.field(FieldIx::new(0)).line, 5);
    assert_eq!(parsed.most_bytes(DeclIx::new(0)), 4);
}

#[test]
fn declaration_field_and_variant_lines_name_their_names() {
    let text = b"family qa 1\nversioned\nrecord\n R {\n x\n : u8\n}\nenum\n E {\n a\n : boxed R\n b\n}\n";
    let parsed = schema::parse(text).expect("whitespace separates tokens");
    assert_eq!(parsed.declaration(DeclIx::new(0)).line, 4);
    assert_eq!(parsed.field(FieldIx::new(0)).line, 5);
    assert_eq!(parsed.declaration(DeclIx::new(1)).line, 9);
    assert_eq!(parsed.variant(VariantIx::new(0)).line, 10);
    assert_eq!(parsed.variant(VariantIx::new(1)).line, 12);
    assert_eq!(parsed.spelled(parsed.declaration(DeclIx::new(0)).name), b"R");
    assert_eq!(parsed.variant(VariantIx::new(0)).record, Some(DeclIx::new(0)));
    assert!(parsed.variant(VariantIx::new(0)).boxed, "boxed payload is recorded");
}

#[test]
fn held_words_before_colons_are_field_names() {
    let text = b"family qa 1 record Item {} record R { raw: bytes 9 holds: bytes 8 pieces: bytes 7 }";
    let parsed = schema::parse(text).expect("held words can name fields");
    for (index, name, bound) in [(0, b"raw".as_slice(), 9), (1, b"holds", 8), (2, b"pieces", 7)] {
        let field = parsed.field(FieldIx::new(index));
        assert_eq!(parsed.spelled(field.name), name);
        assert_eq!(parsed.ty(field.ty), Type::Bytes { bound, held: Held::Opaque });
    }
}

#[test]
fn held_clauses_keep_external_names_and_local_indexes() {
    let text = b"family qa 1 record Item {} enum E { empty } record R { a: bytes 4 holds other-family Unknown b: bytes 4 pieces E }";
    let parsed = schema::parse(text).expect("holds is external; pieces can name a declaration");
    let held = parsed.ty(parsed.field(FieldIx::new(0)).ty);
    let Type::Bytes { held: Held::Record { family, name }, .. } = held else {
        unreachable!("holds is a record annotation")
    };
    assert_eq!(parsed.spelled(family), b"other-family");
    assert_eq!(parsed.spelled(name), b"Unknown");
    assert_eq!(
        parsed.ty(parsed.field(FieldIx::new(1)).ty),
        Type::Bytes { bound: 4, held: Held::Pieces(DeclIx::new(1)) }
    );
    assert_eq!(parsed.most_bytes(DeclIx::new(2)), 16);
}

#[test]
fn references_must_precede_the_declaration_that_uses_them() {
    for text in [
        "family qa 1\nrecord R { x: option list 0 R }",
        "family qa 1\nrecord R { x: bytes 0 pieces Later }\nrecord Later {}",
        "family qa 1\nenum E { x: R }\nrecord R {}",
    ] {
        let error = schema::parse(text.as_bytes()).expect_err("forward reference");
        assert_eq!((error.wrong, error.line), (Wrong::Undeclared, 2), "{text}");
    }
}

#[test]
fn enum_payloads_must_be_records() {
    refusal("family qa 1\nenum E { empty }\nenum F { x: boxed E }", Wrong::NotRecord, 3, "x");
}

#[test]
fn duplicate_names_are_local_to_their_tables() {
    for (text, wrong, token) in [
        ("family qa 1\nrecord R { x: u8\n x: u16 }", Wrong::DuplicateField, "x"),
        ("family qa 1\nenum E { x\n x }", Wrong::DuplicateVariant, "x"),
        ("family qa 1\nrecord R {}\nenum R { x }", Wrong::DuplicateType, "R"),
    ] {
        refusal(text, wrong, 3, token);
    }
}

#[test]
fn field_layout_faults_precede_duplicate_declaration_names() {
    refusal("family qa 1\nrecord R {}\nrecord R { x: Missing }", Wrong::Undeclared, 3, "Missing");
}

const UNFINISHED: &str = "family qa 1\nrecord R {\n x:\n";

#[test]
fn end_of_text_uses_its_last_line() {
    // A final newline ends the last line and starts none (codec.md 7.1).
    refusal(UNFINISHED, Wrong::End, 3, "");
}

#[test]
fn codegen_end_of_text_uses_its_last_line() {
    assert_eq!(skein_codegen::parse(UNFINISHED).expect_err("unfinished schema").line, 3);
}

#[test]
fn numeric_words_refuse_one_past_their_widths() {
    for (text, wrong, token) in [
        ("family qa 65536", Wrong::Version, "65536"),
        ("family qa 1 record R { x: fixed 4294967296 }", Wrong::Bound, "4294967296"),
        ("family qa 1 record R { x: bytes -1 }", Wrong::Bound, "-1"),
    ] {
        refusal(text, wrong, 1, token);
    }
}

#[test]
fn encoded_worst_cases_accept_u32_maximum() {
    for (text, most) in [
        ("family qa 0 record R { x: fixed 4294967295 }", u32::MAX),
        ("family qa 65535 versioned record R { x: fixed 4294967293 }", u32::MAX),
        ("family qa 1 record R { x: bytes 4294967291 }", u32::MAX),
        ("family qa 1 record R { x: list 0 fixed 4294967295 }", 4),
    ] {
        let parsed = schema::parse(text.as_bytes()).expect("encoded maximum fits");
        assert_eq!(parsed.most_bytes(DeclIx::new(0)), most, "{text}");
    }
}

#[test]
fn encoded_worst_cases_refuse_one_past_u32() {
    for text in [
        "family qa 1\nrecord R { x: bytes 4294967292 }",
        "family qa 1\nversioned record R { x: fixed 4294967294 }",
        "family qa 1\nrecord R { x: fixed 4294967295 y: u8 }",
    ] {
        refusal(text, Wrong::Large, 2, "R");
    }
}

#[test]
fn nested_list_worst_case_refuses_u64_overflow() {
    refusal("family qa 1\nrecord R { x: list 4294967295 list 4294967295 fixed 4294967295 }", Wrong::Overflow, 2, "x");
}

fn enumeration(variants: u32) -> String {
    let mut text = String::from("family qa 1\nenum E {\n");
    for index in 0..variants {
        writeln!(text, " v{index}").expect("String write");
    }
    text.push('}');
    text
}

#[test]
fn enumerations_accept_all_256_wire_tags() {
    // codec.md, 7.2: one variant for each u8 tag.
    const TAGS: u32 = 256;
    let parsed = schema::parse(enumeration(TAGS).as_bytes()).expect("all tags fit");
    assert_eq!(parsed.declaration(DeclIx::new(0)).shape, Shape::Enum { first: VariantIx::new(0), count: TAGS });
    assert_eq!(parsed.spelled(parsed.variant(VariantIx::new(255)).name), b"v255");
}

#[test]
fn enumerations_refuse_empty_and_one_past_256() {
    for (count, wrong) in [(0, Wrong::NoVariants), (257, Wrong::ManyVariants)] {
        refusal(&enumeration(count), wrong, 2, "E");
    }
}

fn declarations(count: u32) -> String {
    let mut text = String::from("family qa 1\n");
    for index in 0..count {
        writeln!(text, "record R{index} {{}}").expect("String write");
    }
    text
}

#[test]
fn declaration_table_accepts_its_ceiling() {
    let parsed = schema::parse(declarations(schema::MAX_DECLARATIONS).as_bytes()).expect("declaration ceiling");
    assert_eq!(parsed.declarations(), schema::MAX_DECLARATIONS);
    assert_eq!(parsed.find(b"R4095"), Some(DeclIx::new(4095)));
}

#[test]
fn declaration_table_refuses_one_past_its_ceiling() {
    let count = schema::MAX_DECLARATIONS.checked_add(1).expect("one past ceiling");
    assert_eq!(schema::parse(declarations(count).as_bytes()).expect_err("too many declarations").wrong, Wrong::Many);
}

fn padded_text(extra: usize) -> Vec<u8> {
    let mut text = b"family qa 1\n#".to_vec();
    let limit = usize::try_from(schema::MAX_TEXT).expect("text ceiling");
    text.resize(limit.checked_add(extra).expect("one past ceiling"), b'@');
    text
}

#[test]
fn schema_text_accepts_its_byte_ceiling() {
    assert_eq!(schema::parse(&padded_text(0)).map(|parsed| parsed.declarations()), Ok(0));
}

#[test]
fn schema_text_refuses_one_past_its_byte_ceiling() {
    assert_eq!(schema::parse(&padded_text(1)).expect_err("text too long").wrong, Wrong::Long);
}

#[test]
fn minimum_and_maximum_sizes_follow_nested_types() {
    let text = b"family qa 1 record Empty {} versioned record V { n: u16 } enum E { a: V b } record R { a: list 2 option V b: E c: list 3 Empty d: fixed 0 }";
    let parsed = schema::parse(text).expect("nested sizes");
    // R: (4 + 2 * (1 + 4)) + (1 + 4) + 4 + 0 = 23.
    assert_eq!(parsed.most_bytes(DeclIx::new(3)), 23);
    for (index, minimum) in [(0, 2), (1, 4), (2, 1), (3, 4), (4, 0)] {
        assert_eq!(parsed.fewest_bytes(parsed.field(FieldIx::new(index)).ty), minimum, "field {index}");
    }
}

#[test]
fn runtime_names_are_independent_of_rust_names() {
    for text in [
        "family qa 1 record lower { Camel: u8 }",
        "family qa 1 record R { into_parts: u8 }",
        "family qa 1 record R { type: u8 }",
    ] {
        assert_eq!(schema::parse(text.as_bytes()).map(|parsed| parsed.declarations()), Ok(1), "{text}");
        drop(skein_codegen::parse(text).expect_err("Rust names checked by codegen only"));
    }
}

#[test]
fn held_annotations_do_not_change_generated_output() {
    let plain = "family qa 1 record Item {} record R { x: bytes 4 y: bytes 8 }";
    let held = "family qa 1 record Item {} record R { x: bytes 4 holds other-family Missing y: bytes 8 pieces Item }";
    let plain = skein_codegen::parse(plain).expect("plain schema");
    let held = skein_codegen::parse(held).expect("annotated schema");
    assert_eq!(skein_codegen::generate(&plain), skein_codegen::generate(&held));
}

#[test]
fn generated_name_collisions_are_checked_only_by_codegen() {
    for text in ["family qa 1 record R {} record RParts {}", "family qa 1 record R { a_b: bytes 1 a__b: bytes 1 }"] {
        drop(schema::parse(text.as_bytes()).expect("runtime table names are distinct"));
        drop(skein_codegen::parse(text).expect_err("generated Rust names collide"));
    }
}

#[test]
fn long_type_chains_preserve_their_outermost_first_order() {
    // 4,096 option tags and the u8 terminal each cost one byte (codec.md, 3).
    const DEPTH: u32 = 4096;
    let mut text = String::from("family qa 1 record R { x: ");
    for _ in 0..DEPTH {
        text.push_str("option ");
    }
    text.push_str("u8 }");
    let parsed = schema::parse(text.as_bytes()).expect("type chain is read without recursion");
    assert_eq!(parsed.most_bytes(DeclIx::new(0)), 4097);
    let mut ty = parsed.field(FieldIx::new(0)).ty;
    for _ in 0..DEPTH {
        assert_eq!(parsed.fewest_bytes(ty), 1);
        let Type::Option(item) = parsed.ty(ty) else { unreachable!("option chain") };
        ty = item;
    }
    assert_eq!(parsed.ty(ty), Type::U8);
}
