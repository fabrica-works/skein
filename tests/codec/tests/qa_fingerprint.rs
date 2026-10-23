//! Black-box checks of codec.md 9's canonical layout encoding.

#![cfg(test)]

use core::fmt::Write;
use skein_codec::schema::{self, DeclIx, MAX_DECLARATIONS, Schema};
use skein_lib::sha256::{Sha256, digest};

fn read(text: &str) -> Schema {
    schema::parse(text.as_bytes()).expect("valid QA schema")
}

fn declaration(schema: &Schema, name: &str) -> [u8; 32] {
    schema.fingerprints().declaration(schema.find(name.as_bytes()).expect("QA declaration")).digest()
}

fn field_schema(ty: &str) -> Schema {
    read(&format!("family qa 258 record R {{ f: {ty} }}"))
}

#[test]
fn each_type_has_its_literal_canonical_encoding() {
    // codec.md 9.1: version 1, record 1, unversioned 0, one field.
    const PREFIX: &[u8] = &[1, 1, 0, 0, 0, 0, 1];
    let cases: &[(&str, &[u8])] = &[
        ("u8", &[1]),
        ("u16", &[2]),
        ("u32", &[3]),
        ("u64", &[4]),
        ("bool", &[5]),
        ("duration", &[6]),
        ("fixed 258", &[7, 0, 0, 1, 2]),
        ("secret 258", &[8, 0, 0, 1, 2]),
        ("text 258", &[9, 0, 0, 1, 2]),
        ("bytes 258", &[10, 0, 0, 1, 2, 0]),
        ("list 258 u16", &[11, 0, 0, 1, 2, 2]),
        ("option u16", &[12, 2]),
        ("option list 258 option u16", &[12, 11, 0, 0, 1, 2, 12, 2]),
        ("bytes 258 holds ab cde", &[10, 0, 0, 1, 2, 1, 0, 0, 0, 2, b'a', b'b', 0, 0, 0, 3, b'c', b'd', b'e']),
        ("bytes 258 holds a bcde", &[10, 0, 0, 1, 2, 1, 0, 0, 0, 1, b'a', 0, 0, 0, 4, b'b', b'c', b'd', b'e']),
    ];
    for (ty, suffix) in cases {
        assert_eq!(declaration(&field_schema(ty), "R"), digest(&[PREFIX, suffix].concat()), "{ty}");
    }
}

#[test]
fn empty_and_versioned_records_have_literal_headers() {
    let cases: &[(&str, &[u8])] = &[
        ("family qa 258 record R {}", &[1, 1, 0, 0, 0, 0, 0]),
        ("family qa 258 versioned record R {}", &[1, 1, 1, 1, 2, 0, 0, 0, 0]),
        ("family qa 0 versioned record R {}", &[1, 1, 1, 0, 0, 0, 0, 0, 0]),
        ("family qa 65535 versioned record R {}", &[1, 1, 1, 255, 255, 0, 0, 0, 0]),
    ];
    for (text, canonical) in cases {
        assert_eq!(declaration(&read(text), "R"), digest(canonical), "{text}");
    }
}

#[test]
fn named_payload_and_pieces_embed_the_child_digest() {
    let schema = read(
        "family qa 258 record Leaf { n: u16 }
         record Named { f: option Leaf }
         record Pieces { f: bytes 258 pieces Leaf }
         enum E { empty, value: boxed Leaf }",
    );
    let leaf = digest(&[1, 1, 0, 0, 0, 0, 1, 2]);
    let named = digest(&[&[1, 1, 0, 0, 0, 0, 1, 12, 13][..], &leaf].concat());
    let pieces = digest(&[&[1, 1, 0, 0, 0, 0, 1, 10, 0, 0, 1, 2, 2][..], &leaf].concat());
    let enumeration = digest(&[&[1, 2, 0, 0, 0, 2, 0, 1][..], &leaf].concat());
    for (name, expected) in [("Leaf", leaf), ("Named", named), ("Pieces", pieces), ("E", enumeration)] {
        assert_eq!(declaration(&schema, name), expected, "{name}");
    }
    let canonical_family = [&[1, 3, 0, 0, 0, 4][..], &leaf, &named, &pieces, &enumeration].concat();
    assert_eq!(schema.fingerprints().family().digest(), digest(&canonical_family));
}

#[test]
fn empty_family_has_a_literal_canonical_encoding() {
    assert_eq!(read("family qa 9").fingerprints().family().digest(), digest(&[1, 3, 0, 0, 0, 0]));
}

#[test]
fn local_names_boxing_and_header_without_versioned_are_invisible() {
    let kept = read(
        "family qa 1 record A { x: u8 } enum E { a: A, b }
         record R { x: option A y: bytes 8 pieces A z: bytes 9 holds elsewhere A }",
    );
    let current = read(
        "# names, formatting, and Rust boxing are not layout
         family other-local-name 65535
         record B { renamed: u8, }
         enum F { renamed: boxed B, empty, }
         record S { first: option B, second: bytes 8 pieces B, third: bytes 9 holds elsewhere A, }",
    );
    for (old, new) in [("A", "B"), ("E", "F"), ("R", "S")] {
        assert_eq!(declaration(&kept, old), declaration(&current, new), "{old} to {new}");
    }
    assert_eq!(kept.fingerprints().family(), current.fingerprints().family());
}

#[test]
fn field_and_payload_order_are_part_of_layout() {
    let cases = [
        ("record R { a: u8 b: u16 }", "record R { b: u16 a: u8 }"),
        ("record A {} enum R { empty, a: A }", "record A {} enum R { a: A, empty }"),
        ("record R { a: option list 2 u8 }", "record R { a: list 2 option u8 }"),
        ("record R { a: bytes 8 holds ab c }", "record R { a: bytes 8 holds a bc }"),
        ("record R { a: bytes 8 }", "record R { a: bytes 8 holds qa R }"),
    ];
    for (old, new) in cases {
        let kept = read(&format!("family qa 1 {old}"));
        let current = read(&format!("family qa 1 {new}"));
        assert_ne!(declaration(&kept, "R"), declaration(&current, "R"), "{old} to {new}");
    }
}

#[test]
fn changed_leaf_reaches_every_kind_of_reference() {
    const REFS: &str = "record R { f: option list 2 Leaf }
                        record P { f: bytes 8 pieces Leaf }
                        enum E { a: Leaf }";
    let kept = read(&format!("family qa 1 record Leaf {{ x: u8 }} {REFS}"));
    let current = read(&format!("family qa 1 record Leaf {{ x: u16 }} {REFS}"));
    for name in ["Leaf", "R", "P", "E"] {
        assert_ne!(declaration(&kept, name), declaration(&current, name), "{name}");
    }
}

#[test]
fn family_order_changes_without_moving_declaration_layouts() {
    let kept = read("family qa 1 record A { n: u8 } record B { n: u16 }");
    let current = read("family qa 1 record B { n: u16 } record A { n: u8 }");
    for name in ["A", "B"] {
        assert_eq!(declaration(&kept, name), declaration(&current, name), "{name}");
    }
    assert_ne!(kept.fingerprints().family(), current.fingerprints().family());
}

#[test]
fn version_moves_only_versioned_records_and_their_referrers() {
    const BODY: &str = "record Plain { n: u8 } versioned record V { n: u8 }
                        record R { v: V } record P { v: bytes 8 pieces V }
                        enum E { v: V }";
    let kept = read(&format!("family qa 255 {BODY}"));
    let current = read(&format!("family qa 256 {BODY}"));
    assert_eq!(declaration(&kept, "Plain"), declaration(&current, "Plain"));
    for name in ["V", "R", "P", "E"] {
        assert_ne!(declaration(&kept, name), declaration(&current, name), "{name}");
    }
}

#[test]
fn full_declaration_table_includes_every_unused_root() {
    let mut text = String::from("family qa 1\n");
    for index in 0..MAX_DECLARATIONS {
        writeln!(text, "record R{index} {{}}").expect("String write");
    }
    let schema = read(&text);
    let fingerprints = schema.fingerprints();
    let empty = digest(&[1, 1, 0, 0, 0, 0, 0]);
    let mut family = Sha256::new();
    family.update(&[1, 3, 0, 0, 16, 0]); // 4096 declarations, codec.md 7.3.
    for index in 0..MAX_DECLARATIONS {
        assert_eq!(fingerprints.declaration(DeclIx::new(index)).digest(), empty, "R{index}");
        family.update(&empty);
    }
    assert_eq!(fingerprints.family().digest(), family.finish());
}

#[test]
fn all_256_variants_have_a_big_endian_count() {
    const VARIANTS: u32 = 256; // codec.md 7.2: all values of a one-byte tag.
    let mut text = String::from("family qa 1 enum E {");
    let mut canonical = vec![1, 2, 0, 0, 1, 0];
    for index in 0..VARIANTS {
        write!(text, "v{index},").expect("String write");
        canonical.push(0);
    }
    text.push('}');
    assert_eq!(declaration(&read(&text), "E"), digest(&canonical));
}

#[test]
fn long_option_chains_keep_their_canonical_level_order() {
    const LEVELS: usize = 4096; // A deep chain within MAX_TEXT and the u32 encoded ceiling.
    let ty = format!("{}u16", "option ".repeat(LEVELS));
    let mut canonical = vec![1, 1, 0, 0, 0, 0, 1];
    canonical.extend(core::iter::repeat_n(12, LEVELS));
    canonical.push(2);
    assert_eq!(declaration(&field_schema(&ty), "R"), digest(&canonical));
}

#[test]
fn full_named_chain_uses_each_preceding_declarations_digest() {
    let mut text = String::from("family qa 1 record R0 {}\n");
    for index in 1..MAX_DECLARATIONS {
        let prior = index.checked_sub(1).expect("chain starts at one");
        writeln!(text, "record R{index} {{ f: R{prior} }}").expect("String write");
    }
    let fingerprints = read(&text).fingerprints();
    let mut expected = digest(&[1, 1, 0, 0, 0, 0, 0]);
    assert_eq!(fingerprints.declaration(DeclIx::new(0)).digest(), expected);
    for index in 1..MAX_DECLARATIONS {
        expected = digest(&[&[1, 1, 0, 0, 0, 0, 1, 13][..], &expected].concat());
        assert_eq!(fingerprints.declaration(DeclIx::new(index)).digest(), expected, "R{index}");
    }
}
