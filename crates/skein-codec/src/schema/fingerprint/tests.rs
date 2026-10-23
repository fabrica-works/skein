//! Encoding goldens and both sides of each layout element.

use super::{Fingerprint, Schema};
use crate::schema::parse;
use skein_lib::sha256::digest;

fn schema(text: &str) -> Schema {
    parse(text.as_bytes()).expect("valid fingerprint fixture")
}

fn selected(schema: &Schema, name: Option<&str>) -> Fingerprint {
    let fingerprints = schema.fingerprints();
    match name {
        Some(name) => fingerprints.declaration(schema.find(name.as_bytes()).expect("fixture declaration")),
        None => fingerprints.family(),
    }
}

#[test]
fn canonical_bytes_pin_records_enums_and_family() {
    let schema = schema("family f 3\nversioned record A { x: u8 }\nenum E { a, b: A }");
    let fingerprints = schema.fingerprints();
    let record = [
        1, // encoding version
        1, 1, 0, 3, // record, versioned, family version
        0, 0, 0, 1, // one field
        1, // u8
    ];
    let record_digest = digest(&record);
    let enumeration = [
        1, 2, // encoding version, enumeration
        0, 0, 0, 2, // two variants
        0, 1, // no payload, record payload
        0xa2, 0x92, 0x9d, 0x21, 0x16, 0x0b, 0xd7, 0x93, // A fingerprint bytes 0..8
        0xb0, 0x95, 0xb0, 0xe0, 0xc3, 0xe0, 0x00, 0xf4, // A fingerprint bytes 8..16
        0xba, 0x47, 0x2e, 0x95, 0x7f, 0x72, 0x5b, 0x62, // A fingerprint bytes 16..24
        0x4e, 0x53, 0xe9, 0x72, 0x8e, 0xf9, 0xb2, 0x03, // A fingerprint bytes 24..32
    ];
    let enum_digest = digest(&enumeration);
    let family = [
        1, 3, // encoding version, family
        0, 0, 0, 2, // two declarations
        0xa2, 0x92, 0x9d, 0x21, 0x16, 0x0b, 0xd7, 0x93, // A fingerprint bytes 0..8
        0xb0, 0x95, 0xb0, 0xe0, 0xc3, 0xe0, 0x00, 0xf4, // A fingerprint bytes 8..16
        0xba, 0x47, 0x2e, 0x95, 0x7f, 0x72, 0x5b, 0x62, // A fingerprint bytes 16..24
        0x4e, 0x53, 0xe9, 0x72, 0x8e, 0xf9, 0xb2, 0x03, // A fingerprint bytes 24..32
        0xe3, 0xea, 0x56, 0xe2, 0x3b, 0xb9, 0xf3, 0xe0, // E fingerprint bytes 0..8
        0xfe, 0x49, 0x1a, 0xdd, 0x14, 0x9e, 0x10, 0x26, // E fingerprint bytes 8..16
        0xbe, 0x33, 0xb6, 0xff, 0x32, 0x5e, 0x62, 0xab, // E fingerprint bytes 16..24
        0x5b, 0x1e, 0x22, 0xae, 0x4b, 0x0e, 0x1a, 0x49, // E fingerprint bytes 24..32
    ];
    assert_eq!(selected(&schema, Some("A")).digest(), record_digest);
    assert_eq!(selected(&schema, Some("E")).digest(), enum_digest);
    assert_eq!(fingerprints.family().digest(), digest(&family));
}

#[test]
fn names_comments_layout_and_boxing_preserve_every_fingerprint() {
    let base = schema("family f 3\nrecord A { x: u8 } enum E { a, b: A } record R { x: A, y: bytes 8 pieces A }");
    let cases = [
        (
            "declaration and references",
            "family f 3\nrecord B { x: u8 } enum E { a, b: B } record R { x: B, y: bytes 8 pieces B }",
        ),
        ("field", "family f 3\nrecord A { z: u8 } enum E { a, b: A } record R { x: A, y: bytes 8 pieces A }"),
        ("variant", "family f 3\nrecord A { x: u8 } enum E { z, b: A } record R { x: A, y: bytes 8 pieces A }"),
        (
            "comment",
            "family f 3\n# a comment\nrecord A { x: u8 } enum E { a, b: A } record R { x: A, y: bytes 8 pieces A }",
        ),
        ("layout", "family f 3\nrecord A {\nx: u8,\n}\nenum E {\na\nb: A\n}\nrecord R {\nx: A\ny: bytes 8 pieces A\n}"),
        ("boxed", "family f 3\nrecord A { x: u8 } enum E { a, b: boxed A } record R { x: A, y: bytes 8 pieces A }"),
        (
            "family name",
            "family renamed 3\nrecord A { x: u8 } enum E { a, b: A } record R { x: A, y: bytes 8 pieces A }",
        ),
    ];
    let expected = base.fingerprints();
    for (label, text) in cases {
        let other = schema(text).fingerprints();
        assert_eq!(expected.family(), other.family(), "{label}");
        for at in 0..base.declarations() {
            let at = crate::schema::DeclIx::new(at);
            assert_eq!(expected.declaration(at), other.declaration(at), "{label}");
        }
    }
}

/// Each row names the selected declaration (or family) and whether it moves.
struct Pair {
    label: &'static str,
    left: &'static str,
    right: &'static str,
    declaration: Option<&'static str>,
    moved: bool,
}

const LAYOUT_PAIRS: &[Pair] = &[
    Pair {
        label: "shape",
        left: "family f 1\nrecord R {}",
        right: "family f 1\nenum R { a }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "versioned",
        left: "family f 1\nrecord R {}",
        right: "family f 1\nversioned record R {}",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "versioned version",
        left: "family f 1\nversioned record R {}",
        right: "family f 2\nversioned record R {}",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "unversioned version",
        left: "family f 1\nrecord R {}",
        right: "family f 2\nrecord R {}",
        declaration: Some("R"),
        moved: false,
    },
    Pair {
        label: "unversioned family version",
        left: "family f 1\nrecord R {}",
        right: "family f 2\nrecord R {}",
        declaration: None,
        moved: false,
    },
    Pair {
        label: "field order",
        left: "family f 1\nrecord R { x: u8, y: u16 }",
        right: "family f 1\nrecord R { y: u16, x: u8 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "field count",
        left: "family f 1\nrecord R { x: u8 }",
        right: "family f 1\nrecord R { x: u8, y: u8 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "u8 to u16",
        left: "family f 1\nrecord R { x: u8 }",
        right: "family f 1\nrecord R { x: u16 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "u16 to u32",
        left: "family f 1\nrecord R { x: u16 }",
        right: "family f 1\nrecord R { x: u32 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "u32 to u64",
        left: "family f 1\nrecord R { x: u32 }",
        right: "family f 1\nrecord R { x: u64 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "u64 to bool",
        left: "family f 1\nrecord R { x: u64 }",
        right: "family f 1\nrecord R { x: bool }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "bool to duration",
        left: "family f 1\nrecord R { x: bool }",
        right: "family f 1\nrecord R { x: duration }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "fixed bound",
        left: "family f 1\nrecord R { x: fixed 1 }",
        right: "family f 1\nrecord R { x: fixed 2 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "secret bound",
        left: "family f 1\nrecord R { x: secret 1 }",
        right: "family f 1\nrecord R { x: secret 2 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "text bound",
        left: "family f 1\nrecord R { x: text 1 }",
        right: "family f 1\nrecord R { x: text 2 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "bytes bound",
        left: "family f 1\nrecord R { x: bytes 1 }",
        right: "family f 1\nrecord R { x: bytes 2 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "list bound",
        left: "family f 1\nrecord R { x: list 1 u8 }",
        right: "family f 1\nrecord R { x: list 2 u8 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "wrapper order",
        left: "family f 1\nrecord R { x: list 2 option u8 }",
        right: "family f 1\nrecord R { x: option list 2 u8 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "text bytes",
        left: "family f 1\nrecord R { x: text 2 }",
        right: "family f 1\nrecord R { x: bytes 2 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "bytes secret",
        left: "family f 1\nrecord R { x: bytes 2 }",
        right: "family f 1\nrecord R { x: secret 2 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "secret text",
        left: "family f 1\nrecord R { x: secret 2 }",
        right: "family f 1\nrecord R { x: text 2 }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "opaque holds",
        left: "family f 1\nrecord R { x: bytes 8 }",
        right: "family f 1\nrecord R { x: bytes 8 holds other A }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "holds family",
        left: "family f 1\nrecord R { x: bytes 8 holds other A }",
        right: "family f 1\nrecord R { x: bytes 8 holds another A }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "holds type",
        left: "family f 1\nrecord R { x: bytes 8 holds other A }",
        right: "family f 1\nrecord R { x: bytes 8 holds other B }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "opaque pieces",
        left: "family f 1\nrecord A { a: u8 } record B { b: u16 } record R { x: bytes 8 }",
        right: "family f 1\nrecord A { a: u8 } record B { b: u16 } record R { x: bytes 8 pieces A }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "holds pieces",
        left: "family f 1\nrecord A { a: u8 } record B { b: u16 } record R { x: bytes 8 holds other A }",
        right: "family f 1\nrecord A { a: u8 } record B { b: u16 } record R { x: bytes 8 pieces A }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "pieces target",
        left: "family f 1\nrecord A { a: u8 } record B { b: u16 } record R { x: bytes 8 pieces A }",
        right: "family f 1\nrecord A { a: u8 } record B { b: u16 } record R { x: bytes 8 pieces B }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "variant count",
        left: "family f 1\nenum R { a }",
        right: "family f 1\nenum R { a, b }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "payload presence",
        left: "family f 1\nrecord A {} enum R { a }",
        right: "family f 1\nrecord A {} enum R { a: A }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "payload record",
        left: "family f 1\nrecord A { a: u8 } record B { b: u16 } enum R { a: A }",
        right: "family f 1\nrecord A { a: u8 } record B { b: u16 } enum R { a: B }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "payload layout",
        left: "family f 1\nrecord A { x: u8 } enum R { a: A }",
        right: "family f 1\nrecord A { x: u16 } enum R { a: A }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "variant order",
        left: "family f 1\nrecord A { a: u8 } record B { b: u16 } enum R { a: A, b: B }",
        right: "family f 1\nrecord A { a: u8 } record B { b: u16 } enum R { b: B, a: A }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "named layout",
        left: "family f 1\nrecord A { x: u8 } record R { x: A }",
        right: "family f 1\nrecord A { x: u16 } record R { x: A }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "named rename",
        left: "family f 1\nrecord A { x: u8 } record R { x: A }",
        right: "family f 1\nrecord B { x: u8 } record R { x: B }",
        declaration: Some("R"),
        moved: false,
    },
    Pair {
        label: "pieces layout",
        left: "family f 1\nrecord A { x: u8 } record R { x: bytes 8 pieces A }",
        right: "family f 1\nrecord A { x: u16 } record R { x: bytes 8 pieces A }",
        declaration: Some("R"),
        moved: true,
    },
    Pair {
        label: "unreferenced R",
        left: "family f 1\nrecord A { x: u8 } record R {}",
        right: "family f 1\nrecord A { x: u16 } record R {}",
        declaration: Some("R"),
        moved: false,
    },
    Pair {
        label: "unreferenced None",
        left: "family f 1\nrecord A { x: u8 } record R {}",
        right: "family f 1\nrecord A { x: u16 } record R {}",
        declaration: None,
        moved: true,
    },
    Pair {
        label: "declaration order A",
        left: "family f 1\nrecord A { x: u8 } record B { x: u16 }",
        right: "family f 1\nrecord B { x: u16 } record A { x: u8 }",
        declaration: Some("A"),
        moved: false,
    },
    Pair {
        label: "declaration order B",
        left: "family f 1\nrecord A { x: u8 } record B { x: u16 }",
        right: "family f 1\nrecord B { x: u16 } record A { x: u8 }",
        declaration: Some("B"),
        moved: false,
    },
    Pair {
        label: "declaration order None",
        left: "family f 1\nrecord A { x: u8 } record B { x: u16 }",
        right: "family f 1\nrecord B { x: u16 } record A { x: u8 }",
        declaration: None,
        moved: true,
    },
    Pair {
        label: "declaration added",
        left: "family f 1\nrecord R {}",
        right: "family f 1\nrecord R {} record A {}",
        declaration: None,
        moved: true,
    },
];

#[test]
fn layout_elements_move_the_selected_fingerprints_from_both_sides() {
    for case in LAYOUT_PAIRS {
        let left = selected(&schema(case.left), case.declaration);
        let right = selected(&schema(case.right), case.declaration);
        assert_eq!(left != right, case.moved, "{} forward", case.label);
        assert_eq!(right != left, case.moved, "{} reverse", case.label);
    }
}
