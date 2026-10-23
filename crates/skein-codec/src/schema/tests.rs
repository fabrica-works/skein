//! Schema refusals and their nearest passing neighbours (codec.md, 8.3).

#![expect(clippy::disallowed_types, reason = "ceiling fixtures are assembled with ordinary Rust test containers")]
#![expect(clippy::disallowed_macros, reason = "numbered declarations and variants are formatted test input")]

use super::{
    DeclIx, Expected, FieldIx, Held, MAX_DECLARATIONS, MAX_TEXT, SchemaError, Span, Type, TypeIx, Wrong, parse,
};

use core::fmt::Write;

type RefusalCase = (&'static [u8], u32, Span, Wrong, &'static [u8]);

fn refusal(source: &[u8], line: u32, at: Span, wrong: Wrong) {
    assert_eq!(parse(source).expect_err("schema is refused"), SchemaError { line, at, wrong }, "{source:?}");
}

#[test]
fn grammar_refusals_name_the_token_and_line() {
    let cases: &[RefusalCase] = &[
        (b"\n?", 2, Span { start: 1, end: 2 }, Wrong::Character, b"\nfamily f 1"),
        (b"family f\n", 1, Span { start: 9, end: 9 }, Wrong::End, b"family f 1\n"),
        (b"record f 1", 1, Span { start: 0, end: 6 }, Wrong::Expected(Expected::Family), b"family f 1"),
        (
            b"family f 1\nrecord R :",
            2,
            Span { start: 20, end: 21 },
            Wrong::Expected(Expected::Open),
            b"family f 1\nrecord R {}",
        ),
        (
            b"family f 1\nrecord R { a u8 }",
            2,
            Span { start: 24, end: 26 },
            Wrong::Expected(Expected::Colon),
            b"family f 1\nrecord R { a: u8 }",
        ),
        (
            b"family f 1\nversioned enum R {}",
            2,
            Span { start: 21, end: 25 },
            Wrong::Expected(Expected::Record),
            b"family f 1\nversioned record R {}",
        ),
        (
            b"family f 1\nrecord R { a: }",
            2,
            Span { start: 25, end: 26 },
            Wrong::Expected(Expected::Type),
            b"family f 1\nrecord R { a: u8 }",
        ),
        (
            b"family f 1\nR {}",
            2,
            Span { start: 11, end: 12 },
            Wrong::Expected(Expected::Declaration),
            b"family f 1\nrecord R {}",
        ),
        (b"family f 1\nrecord 1R {}", 2, Span { start: 18, end: 20 }, Wrong::Name, b"family f 1\nrecord R1 {}"),
        (
            b"family f 1\nrecord R { a: fixed 4294967296 }",
            2,
            Span { start: 31, end: 41 },
            Wrong::Bound,
            b"family f 1\nrecord R { a: fixed 4294967295 }",
        ),
        (b"family {\n1", 2, Span { start: 7, end: 8 }, Wrong::Family, b"family f\n1"),
        (b"family f 65536", 1, Span { start: 9, end: 14 }, Wrong::Version, b"family f 65535"),
    ];
    for &(source, line, at, wrong, twin) in cases {
        refusal(source, line, at, wrong);
        parse(twin).expect("nearest twin passes");
    }
}

#[test]
fn layout_refusals_name_the_member_or_declaration() {
    let cases: &[RefusalCase] = &[
        (
            b"family f 1\nrecord R { a: Missing }",
            2,
            Span { start: 25, end: 32 },
            Wrong::Undeclared,
            b"family f 1\nrecord Missing {}\nrecord R { a: Missing }",
        ),
        (
            b"family f 1\nenum E { x }\nenum F { x: E }",
            3,
            Span { start: 33, end: 34 },
            Wrong::NotRecord,
            b"family f 1\nrecord E {}\nenum F { x: E }",
        ),
        (b"family f 1\nenum E {}", 2, Span { start: 16, end: 17 }, Wrong::NoVariants, b"family f 1\nenum E { x }"),
        (
            b"family f 1\nrecord R {}\nrecord R {}",
            3,
            Span { start: 30, end: 31 },
            Wrong::DuplicateType,
            b"family f 1\nrecord R {}\nrecord S {}",
        ),
        (
            b"family f 1\nrecord R { x: u8 x: u16 }",
            2,
            Span { start: 28, end: 29 },
            Wrong::DuplicateField,
            b"family f 1\nrecord R { x: u8 y: u16 }",
        ),
        (
            b"family f 1\nenum E { x, x }",
            2,
            Span { start: 23, end: 24 },
            Wrong::DuplicateVariant,
            b"family f 1\nenum E { x, y }",
        ),
        (
            b"family f 1\nrecord R { a: list 4294967295 list 4294967295 u64 }",
            2,
            Span { start: 22, end: 23 },
            Wrong::Overflow,
            b"family f 1\nrecord R { a: list 1 list 1 u64 }",
        ),
        (
            b"family f 1\nrecord R { a: bytes 4294967295 }",
            2,
            Span { start: 18, end: 19 },
            Wrong::Large,
            b"family f 1\nrecord R { a: bytes 4294967291 }",
        ),
    ];
    for &(source, line, at, wrong, twin) in cases {
        refusal(source, line, at, wrong);
        parse(twin).expect("nearest twin passes");
    }
}

#[test]
fn spans_choose_the_second_duplicate_and_eof() {
    let source = b"family f 1\nrecord R { x: u8\n x: u16 }";
    assert_eq!(
        parse(source).expect_err("duplicate field"),
        SchemaError { line: 3, at: Span { start: 29, end: 30 }, wrong: Wrong::DuplicateField }
    );
    assert_eq!(
        parse(b"family\nf\n").expect_err("missing version"),
        SchemaError { line: 2, at: Span { start: 9, end: 9 }, wrong: Wrong::End }
    );
}

#[test]
fn character_pass_precedes_grammar_and_comments_hold_any_bytes() {
    refusal(b"bogus\n#\xff\xfe\n\xff", 3, Span { start: 10, end: 11 }, Wrong::Character);
    parse(b"#\xff\xfe\nfamily f 1\t\r\x0c#\xff").expect("comments are opaque bytes");
    refusal(b"family f 1\x0b", 1, Span { start: 10, end: 11 }, Wrong::Character);
    parse(b"family f 1\nrecord _R { _a: u8 }").expect("Rust casing stays in codegen");
}

#[test]
fn held_records_and_pieces_keep_their_names() {
    let schema = parse(b"family f 1\nrecord Item {}\nrecord R { a: bytes 8 holds other-family Value b: bytes 9 pieces Item c: bytes 4 }").expect("held fields");
    let record = schema.field(FieldIx::new(0));
    let Type::Bytes { bound, held: Held::Record { family, name } } = schema.ty(record.ty) else {
        unreachable!("held record");
    };
    assert_eq!(bound, 8);
    assert_eq!(schema.spelled(family), b"other-family");
    assert_eq!(schema.spelled(name), b"Value");
    assert_eq!(
        schema.ty(schema.field(FieldIx::new(1)).ty),
        Type::Bytes { bound: 9, held: Held::Pieces(DeclIx::new(0)) }
    );
    assert_eq!(schema.ty(schema.field(FieldIx::new(2)).ty), Type::Bytes { bound: 4, held: Held::Opaque });
}

#[test]
fn holds_and_pieces_can_name_the_next_field() {
    for source in [
        &b"family f 1 record R { a: bytes 4 holds: bool }"[..],
        &b"family f 1 record R { a: bytes 4 pieces: bool }"[..],
    ] {
        let schema = parse(source).expect("clause word followed by colon is a field");
        assert_eq!(schema.ty(TypeIx::new(0)), Type::Bytes { bound: 4, held: Held::Opaque });
        assert_eq!(schema.ty(TypeIx::new(1)), Type::Bool);
    }
}

#[test]
fn pieces_require_an_earlier_type_and_holds_checks_words() {
    for source in [
        &b"family f 1\nrecord R { a: bytes 8 pieces Missing }"[..],
        &b"family f 1\nrecord R { a: bytes 8 pieces Missing }\nrecord Missing {}"[..],
    ] {
        refusal(source, 2, Span { start: 40, end: 47 }, Wrong::Undeclared);
    }
    refusal(b"family f 1 record R { a: bytes 8 holds { Value }", 1, Span { start: 39, end: 40 }, Wrong::Family);
    refusal(b"family f 1 record R { a: bytes 8 holds f 1Bad }", 1, Span { start: 41, end: 45 }, Wrong::Name);
    refusal(b"family f 1 record R { a: bytes 8 pieces 1Bad }", 1, Span { start: 40, end: 44 }, Wrong::Name);
}

#[test]
fn chains_are_outermost_first_and_sizes_are_hand_counted() {
    let schema = parse(b"family f 1\nversioned record A { a: u16 b: bytes 3 }\nenum E { a: A, empty }\nrecord R { chain: list 2 option list 3 A e: E fixed: fixed 5 }").expect("small schema");
    assert_eq!(schema.most_bytes(DeclIx::new(0)), 11); // version 2 + integer 2 + length 4 + bytes 3
    assert_eq!(schema.most_bytes(DeclIx::new(1)), 12); // tag + A
    assert_eq!(schema.most_bytes(DeclIx::new(2)), 97); // 4 + 2*(1 + 4 + 3*11) + 12 + 5
    assert_eq!(schema.fewest_bytes(TypeIx::new(2)), 4);
    assert_eq!(schema.fewest_bytes(TypeIx::new(3)), 1);
    assert_eq!(schema.fewest_bytes(TypeIx::new(4)), 4);
    assert_eq!(schema.fewest_bytes(TypeIx::new(5)), 8); // A without byte contents
    assert_eq!(schema.fewest_bytes(TypeIx::new(6)), 1); // enum's empty variant
    assert_eq!(schema.ty(TypeIx::new(2)), Type::List { bound: 2, item: TypeIx::new(3) });
    assert_eq!(schema.ty(TypeIx::new(3)), Type::Option(TypeIx::new(4)));
    assert_eq!(schema.ty(TypeIx::new(4)), Type::List { bound: 3, item: TypeIx::new(5) });
    for (name, index) in [(b"A".as_slice(), 0), (b"E".as_slice(), 1), (b"R".as_slice(), 2)] {
        assert_eq!(schema.find(name), Some(DeclIx::new(index)));
    }
    assert_eq!(schema.find(b"Missing"), None);
    assert_eq!(schema.text(), b"family f 1\nversioned record A { a: u16 b: bytes 3 }\nenum E { a: A, empty }\nrecord R { chain: list 2 option list 3 A e: E fixed: fixed 5 }");
    assert_eq!(schema.spelled(schema.family()), b"f");
    assert_eq!(schema.version(), 1);
    assert_eq!(schema.declarations(), 3);
}

#[test]
fn text_at_its_ceiling_passes_and_one_more_fails() {
    let mut source = Vec::from(b"family f 1 #".as_slice());
    source.resize(usize::try_from(MAX_TEXT).expect("text ceiling fits"), b'x');
    parse(&source).expect("text at ceiling");
    source.push(b'?');
    assert_eq!(
        parse(&source).expect_err("past ceiling"),
        SchemaError { line: 1, at: Span { start: 0, end: 0 }, wrong: Wrong::Long }
    );
}

#[test]
fn declarations_at_the_ceiling_pass_and_one_more_fails() {
    let mut source = String::from("family f 1\n");
    for index in 0..MAX_DECLARATIONS {
        writeln!(&mut source, "record R{index} {{}}").expect("schema fixture");
    }
    assert_eq!(parse(source.as_bytes()).expect("declarations at ceiling").declarations(), MAX_DECLARATIONS);
    let start = u32::try_from(source.len()).expect("text within ceiling");
    source.push_str("versioned record Extra {}");
    assert_eq!(
        parse(source.as_bytes()).expect_err("past ceiling"),
        SchemaError {
            line: MAX_DECLARATIONS.checked_add(2).expect("line fits"),
            at: Span { start, end: start.checked_add(9).expect("token fits") },
            wrong: Wrong::Many,
        }
    );
}

#[test]
fn variants_at_the_tag_ceiling_pass_and_one_more_fails() {
    let mut source = String::from("family f 1\nenum E {\n");
    for index in 0_u16..256 {
        writeln!(&mut source, "v{index},").expect("variant fixture");
    }
    let at = source.len();
    source.push('}');
    parse(source.as_bytes()).expect("256 variants fit a tag");
    source.insert_str(at, "extra,");
    refusal(source.as_bytes(), 2, Span { start: 16, end: 17 }, Wrong::ManyVariants);
}

#[test]
fn every_scalar_and_length_has_its_minimum_width() {
    let schema = parse(b"family f 1 record R { a: u8 b: u16 c: u32 d: u64 e: bool f: duration g: fixed 7 h: bytes 9 i: secret 9 j: text 9 }").expect("scalar widths");
    for (at, expected) in [1, 2, 4, 8, 1, 8, 7, 4, 4, 4].into_iter().enumerate() {
        assert_eq!(schema.fewest_bytes(TypeIx::new(u32::try_from(at).expect("ten types"))), expected);
    }
    assert_eq!(schema.most_bytes(DeclIx::new(0)), 70); // fixed widths 31 + three lengths with 9 bytes each
}

#[test]
fn deeply_nested_options_are_read_without_recursion() {
    // Four thousand presence bytes followed by one integer byte.
    const DEPTH: u32 = 4_000;
    let mut source = String::from("family f 1 record R { a: ");
    for _ in 0..DEPTH {
        source.push_str("option ");
    }
    source.push_str("u8 }");
    let schema = parse(source.as_bytes()).expect("deep type chain");
    assert_eq!(schema.most_bytes(DeclIx::new(0)), 4_001);
    assert_eq!(schema.ty(TypeIx::new(DEPTH)), Type::U8);
}

#[test]
fn the_grammar_pass_precedes_layout() {
    refusal(b"family f 1 enum E {} garbage", 1, Span { start: 21, end: 28 }, Wrong::Expected(Expected::Declaration));
}
