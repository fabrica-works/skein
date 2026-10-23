//! Black-box checks of matching, patch kinds, text, and wire compatibility.

#![cfg(test)]

use skein_codec::schema::{self, Schema};
use skein_codec::value::{Refusal, Value};
use skein_codegen::diff::{self, Diff, DiffError, Kind, Rename, Version};
use skein_lib::Rng;

#[path = "support/generated.rs"]
mod generated;
#[expect(dead_code, reason = "The QA oracle uses render and ROOM from the shared generic harness.")]
#[path = "support/generic.rs"]
mod generic;

fn read(text: &str) -> Schema {
    schema::parse(text.as_bytes()).expect("valid QA schema")
}

fn compare(kept: &str, current: &str, renames: &[Rename]) -> Diff {
    diff::diff(&read(kept), &read(current), renames).expect("valid QA comparison")
}

fn bodies(kept: &str, current: &str, renames: &[Rename]) -> Diff {
    compare(&format!("family qa 1 {kept}"), &format!("family qa 1 {current}"), renames)
}

fn field_diff(kept: &str, current: &str) -> Diff {
    bodies(&format!("record R {{ f: {kept} }}"), &format!("record R {{ f: {current} }}"), &[])
}

fn lines(diff: &Diff) -> Vec<String> {
    diff.changes.iter().map(|change| format!("{} {change}", change.kind)).collect()
}

fn declaration_rename(old: &str, new: &str) -> Rename {
    Rename::Declaration { old: old.into(), new: new.into() }
}

fn part_rename(decl: &str, old: &str, new: &str) -> Rename {
    Rename::Part { decl: decl.into(), old: old.into(), new: new.into() }
}

fn decoded(schema: &Schema, root: &str, bytes: &[u8]) -> Result<String, Refusal> {
    let decl = schema.find(root.as_bytes()).expect("QA root");
    let mut value = Value::with_room(generic::ROOM);
    let node = value.decode(schema, decl, bytes.into())?;
    Ok(generic::render(schema, &value, node, decl))
}

#[test]
fn nested_widenings_preserve_wire_width_and_spell_types() {
    let cases = [
        ("text 2", "bytes 3"),
        ("bytes 2", "secret 3"),
        ("secret 2", "bytes 3"),
        ("list 2 option text 2", "list 3 option bytes 3"),
        ("option bytes 2 holds other A", "option bytes 3"),
        ("list 2 bytes 2", "list 3 bytes 3 holds other A"),
    ];
    for (old, new) in cases {
        let diff = field_diff(old, new);
        assert_eq!(lines(&diff), [format!("none widen R.f {old} {new}")], "{old} to {new}");
        assert_eq!(diff.kind(), Some(Kind::NoneNeeded), "{old} to {new}");
        assert_eq!(diff.version(), Version::Kept, "{old} to {new}");
    }
}

#[test]
fn integer_widenings_need_derived_byte_conversion() {
    let cases =
        [("u8", "u16"), ("u8", "u64"), ("u16", "u32"), ("u32", "u64"), ("option list 2 u8", "option list 3 u16")];
    for (old, new) in cases {
        let diff = field_diff(old, new);
        assert_eq!(lines(&diff), [format!("derived widen R.f {old} {new}")], "{old} to {new}");
        assert_eq!(diff.version(), Version::Unmoved, "{old} to {new}");
    }
}

#[test]
fn options_inserted_at_multiple_levels_are_wraps() {
    let cases = [
        ("u8", "option option u16"),
        ("option u8", "option option u8"),
        ("list 2 option u8", "option list 3 option option u16"),
        ("option list 2 u8", "option list 3 option u8"),
        ("bytes 2", "option bytes 3 holds other A"),
    ];
    for (old, new) in cases {
        let diff = field_diff(old, new);
        assert_eq!(lines(&diff), [format!("derived wrap R.f {old} {new}")], "{old} to {new}");
    }
}

#[test]
fn long_type_chains_compare_and_display_every_level() {
    const LEVELS: usize = 4096; // Deep, within MAX_TEXT and the u32 encoded ceiling.
    let kept = format!("{}u8", "option ".repeat(LEVELS));
    let current = format!("{}u16", "option ".repeat(LEVELS));
    assert_eq!(lines(&field_diff(&kept, &current)), [format!("derived widen R.f {kept} {current}")]);
}

#[test]
fn narrowing_and_semantic_kind_changes_need_written_conversion() {
    let cases = [
        ("u16", "u8"),
        ("u64", "duration"),
        ("u8", "bool"),
        ("duration", "u64"),
        ("fixed 2", "fixed 3"),
        ("bytes 2", "text 3"),
        ("option u8", "u8"),
        ("option u8", "list 2 option u8"),
        ("list 3 u8", "option list 2 u16"),
        ("bytes 2 holds other A", "bytes 3 holds other B"),
        ("bytes 2 holds other A", "bytes 3 holds elsewhere A"),
    ];
    for (old, new) in cases {
        let diff = field_diff(old, new);
        assert_eq!(lines(&diff), [format!("written retype R.f {old} {new}")], "{old} to {new}");
        assert_eq!(diff.written(), ["R"], "{old} to {new}");
    }
}

#[test]
fn text_to_secret_is_a_widening() {
    let diff = field_diff("text 2", "secret 3");
    assert_eq!(lines(&diff), ["none widen R.f text 2 secret 3"]);
}

#[test]
fn changed_types_display_held_names_from_their_own_texts() {
    let diff = bodies(
        "record A {} record R { f: option list 2 bytes 8 pieces A }",
        "record B {} record R { f: option list 3 bytes 9 holds other New }",
        &[declaration_rename("A", "B")],
    );
    assert_eq!(
        lines(&diff),
        ["none rename A B", "written retype R.f option list 2 bytes 8 pieces A option list 3 bytes 9 holds other New",]
    );
}

#[test]
fn matched_pieces_and_payload_renames_have_no_retype() {
    let diff = bodies(
        "record A { n: u8 } record R { f: option A p: bytes 8 pieces A } enum E { a: A }",
        "record B { n: u8 } record R { f: option B p: bytes 8 pieces B } enum E { a: boxed B }",
        &[declaration_rename("A", "B")],
    );
    assert_eq!(lines(&diff), ["none rename A B"]);
    assert_eq!(diff.version(), Version::Kept);
}

#[test]
fn identical_layouts_do_not_make_distinct_named_types_equal() {
    const PREFIX: &str = "record A { n: u8 } record B { n: u8 }";
    let cases = [("option A", "option B"), ("bytes 8 pieces A", "bytes 8 pieces B")];
    for (old, new) in cases {
        let kept = read(&format!("family qa 1 {PREFIX} record R {{ f: {old} }}"));
        let current = read(&format!("family qa 1 {PREFIX} record R {{ f: {new} }}"));
        let diff = diff::diff(&kept, &current, &[]).expect("same family");
        assert_eq!(kept.fingerprints().family(), current.fingerprints().family());
        assert_eq!(lines(&diff), [format!("written retype R.f {old} {new}")]);
    }
}

#[test]
fn equal_payload_fingerprints_do_not_hide_a_changed_record() {
    const PREFIX: &str = "record A { n: u8 } record B { n: u8 }";
    let kept = read(&format!("family qa 1 {PREFIX} enum E {{ a: A }}"));
    let current = read(&format!("family qa 1 {PREFIX} enum E {{ a: B }}"));
    let diff = diff::diff(&kept, &current, &[]).expect("same family");
    assert_eq!(kept.fingerprints().family(), current.fingerprints().family());
    assert_eq!(lines(&diff), ["written payload E.a A B"]);
}

#[test]
fn changed_leaf_is_listed_once_through_all_reference_kinds() {
    const REFS: &str = "record R { f: option list 2 Leaf } record P { f: bytes 8 pieces Leaf }
                        enum E { a: Leaf }";
    let diff = bodies(&format!("record Leaf {{ n: u8 }} {REFS}"), &format!("record Leaf {{ n: u16 }} {REFS}"), &[]);
    assert_eq!(lines(&diff), ["derived widen Leaf.n u8 u16"]);
}

#[test]
fn declaration_reordering_moves_only_the_family_fingerprint() {
    let kept = read("family qa 1 record A { n: u8 } record B { n: u16 }");
    let current = read("family qa 1 record B { n: u16 } record A { n: u8 }");
    let diff = diff::diff(&kept, &current, &[]).expect("same family");
    assert_eq!(lines(&diff), Vec::<String>::new());
    assert_ne!(kept.fingerprints().family(), current.fingerprints().family());
    for name in [b"A", b"B"] {
        assert_eq!(
            kept.fingerprints().declaration(kept.find(name).expect("kept type")),
            current.fingerprints().declaration(current.find(name).expect("current type")),
        );
    }
}

#[test]
fn explicit_rename_swaps_take_precedence_over_equal_names() {
    let diff = bodies(
        "record A { a: u8 b: u16 } record B { n: u32 }",
        "record B { b: u8 a: u16 } record A { n: u32 }",
        &[
            part_rename("B", "a", "b"),
            declaration_rename("B", "A"),
            part_rename("B", "b", "a"),
            declaration_rename("A", "B"),
        ],
    );
    assert_eq!(lines(&diff), ["none rename A B", "none rename B.a b", "none rename B.b a", "none rename B A"]);
    assert_eq!(diff.proposed, Vec::<Rename>::new());
}

#[test]
fn changes_follow_current_declarations_then_kept_drops() {
    let diff = bodies(
        "record Gone {} record A { x: u8 y: u16 z: u32 } enum E { a, b, c } record Last {}",
        "enum E { c, fresh, a } record B { y: u32 renamed: u8 extra: option u8 } record Added {}",
        &[declaration_rename("A", "B"), part_rename("B", "x", "renamed")],
    );
    assert_eq!(
        lines(&diff),
        [
            "derived add E.fresh",
            "written drop E.b",
            "derived order E c a",
            "none rename A B",
            "derived widen B.y u16 u32",
            "none rename B.x renamed",
            "derived add B.extra option u8 none",
            "derived drop B.z",
            "derived order B y renamed",
            "none add Added",
            "none drop Gone",
            "none drop Last",
        ]
    );
    assert_eq!(diff.written(), ["E"]);
}

#[test]
fn reshape_does_not_compare_parts_or_propose_them() {
    let diff = bodies("record R { f: u8 }", "enum R { f }", &[]);
    assert_eq!(lines(&diff), ["written reshape R"]);
    assert_eq!(diff.proposed, Vec::<Rename>::new());
}

#[test]
fn inserted_variants_need_derived_tag_conversion() {
    let cases = ["enum E { a, c, b }", "enum E { c, a, b }"];
    for current in cases {
        assert_eq!(lines(&bodies("enum E { a, b }", current, &[])), ["derived add E.c"], "{current}");
    }
}

#[test]
fn removing_payload_and_adding_payload_need_written_sites() {
    let diff = bodies("record P { n: u8 } enum E { a: P, b }", "record P { n: u8 } enum E { a, b: P }", &[]);
    assert_eq!(lines(&diff), ["written payload E.a P -", "written payload E.b - P"]);
    assert_eq!(diff.written(), ["E"]);
}

#[test]
fn versioned_marker_changes_precede_current_fields() {
    let cases = [
        ("record R { a: u8 }", "versioned record R { a: u16 }", "derived versioned R"),
        ("versioned record R { a: u8 }", "record R { a: u16 }", "derived unversioned R"),
    ];
    for (old, new, marker) in cases {
        assert_eq!(lines(&bodies(old, new, &[])), [marker, "derived widen R.a u8 u16"]);
    }
}

#[test]
fn added_fields_ask_for_values_except_outer_options() {
    let diff = bodies("record R {}", "record R { a: option list 2 u8 b: list 2 option u8 c: fixed 0 d: bytes 0 }", &[]);
    assert_eq!(
        lines(&diff),
        [
            "derived add R.a option list 2 u8 none",
            "derived add R.b list 2 option u8 ?",
            "derived add R.c fixed 0 ?",
            "derived add R.d bytes 0 ?",
        ]
    );
}

#[test]
fn proposals_pair_positionally_without_searching_ahead() {
    let diff = bodies("record R { a: u8 b: u16 }", "record R { y: u16 x: u8 }", &[]);
    assert_eq!(diff.proposed, Vec::<Rename>::new());
}

#[test]
fn positional_proposals_do_not_confirm_themselves() {
    let diff = bodies("record R { a: u8 b: u16 }", "record R { x: u8 y: u16 }", &[]);
    assert_eq!(diff.proposed, [part_rename("R", "a", "x"), part_rename("R", "b", "y")]);
    assert_eq!(
        lines(&diff),
        ["derived add R.x u8 ?", "derived add R.y u16 ?", "derived drop R.a", "derived drop R.b",]
    );
    assert_eq!(diff.kind(), Some(Kind::Derived));
}

#[test]
fn declaration_proposals_leave_their_parts_unmatched() {
    let diff = bodies("record A { a: u8 }", "record B { b: u8 }", &[]);
    assert_eq!(diff.proposed, [declaration_rename("A", "B")]);
    assert_eq!(lines(&diff), ["none add B", "none drop A"]);
}

#[test]
fn confirmed_declaration_rename_exposes_its_part_proposals() {
    let diff = bodies("record A { a: u8 }", "record B { b: u8 }", &[declaration_rename("A", "B")]);
    assert_eq!(diff.proposed, [part_rename("B", "a", "b")]);
    assert_eq!(lines(&diff), ["none rename A B", "derived add B.b u8 ?", "derived drop B.a"]);
}

#[test]
fn invalid_renames_report_the_exact_input_position() {
    const KEPT: &str = "family qa 1 record A { f: u8 g: u8 } record B {} enum E { a, b }";
    const CURRENT: &str = "family qa 1 record X { h: u8 i: u8 } record B {} enum E { c, d }";
    let cases = [
        (vec![declaration_rename("Missing", "X")], 0),
        (vec![declaration_rename("A", "Missing")], 0),
        (vec![part_rename("X", "f", "h")], 0),
        (vec![declaration_rename("A", "X"), part_rename("A", "f", "h")], 1),
        (vec![declaration_rename("A", "X"), part_rename("X", "missing", "h")], 1),
        (vec![declaration_rename("A", "X"), part_rename("X", "f", "missing")], 1),
        (vec![declaration_rename("A", "X"), declaration_rename("A", "B")], 1),
        (vec![declaration_rename("A", "X"), declaration_rename("B", "X")], 1),
        (vec![part_rename("E", "a", "c"), part_rename("E", "a", "d")], 1),
        (vec![part_rename("E", "a", "c"), part_rename("E", "b", "c")], 1),
    ];
    let kept = read(KEPT);
    let current = read(CURRENT);
    for (renames, position) in cases {
        assert_eq!(diff::diff(&kept, &current, &renames), Err(DiffError::Rename(position)), "{renames:?}");
    }
}

#[test]
fn different_families_are_refused_before_matching() {
    assert_eq!(diff::diff(&read("family a 1"), &read("family b 1"), &[]), Err(DiffError::Family));
}

fn verdicts(cases: &[(u16, u16, &str)], expected: Version) {
    for (old, new, ty) in cases {
        let diff = compare(
            &format!("family qa {old} record R {{ f: u8 }}"),
            &format!("family qa {new} record R {{ f: {ty} }}"),
            &[],
        );
        assert_eq!(diff.version(), expected, "{old} to {new}, u8 to {ty}");
    }
}

#[test]
fn no_patch_and_equal_headers_keep_the_version() {
    verdicts(&[(1, 1, "u8"), (65535, 65535, "u8")], Version::Kept);
}

#[test]
fn no_patch_makes_the_next_version_needless() {
    verdicts(&[(1, 2, "u8")], Version::Needless);
}

#[test]
fn a_patch_with_equal_headers_is_unmoved() {
    verdicts(&[(1, 1, "u16"), (65535, 65535, "u16")], Version::Unmoved);
}

#[test]
fn a_patch_with_the_next_version_is_moved() {
    verdicts(&[(0, 1, "u16"), (1, 2, "u16")], Version::Moved);
}

#[test]
fn skipped_and_overflowed_versions_are_wrong() {
    verdicts(&[(1, 3, "u8"), (1, 3, "u16"), (65535, 0, "u8"), (65535, 0, "u16")], Version::Wrong);
}

#[test]
fn header_move_changes_versioned_fingerprints_without_record_lines() {
    let kept = read("family qa 1 versioned record R { f: u8 }");
    let current = read("family qa 2 versioned record R { f: u8 }");
    let diff = diff::diff(&kept, &current, &[]).expect("same family");
    assert_eq!(diff.to_string(), "family qa 1 to 2\n");
    assert_eq!(diff.version(), Version::Needless);
    assert_ne!(kept.fingerprints().family(), current.fingerprints().family());
}

#[test]
fn display_includes_kinds_and_proposed_rename_lines() {
    let diff = compare("family qa 1 record R { a: u8 }", "family qa 2 record R { b: u8 }", &[]);
    assert_eq!(diff.to_string(), "family qa 1 to 2\nderived add R.b u8 ?\nderived drop R.a\nproposed rename R.a b\n");
}

#[test]
fn none_needed_bound_changes_preserve_generated_draws() {
    // Sample Field has text 4 and bytes 4. Both widen without changing bytes.
    const CURRENT: &str = "family sample 1 record Field { name: bytes 8 value: secret 8 }";
    const DRAWS: u64 = 64; // Tiny bounds: both empty and full draws, over fixed replay seeds.
    let family = generated::families().into_iter().find(|family| family.name == "sample").expect("sample family");
    let root = family.roots.iter().find(|root| root.name == "Field").expect("Field generated decoder");
    let kept = read(family.text);
    let current = read(CURRENT);
    let drawn_schema = skein_codegen::parse(family.text).expect("generated draw schema");
    let diff = diff::diff(&kept, &current, &[]).expect("same family");
    assert_eq!(diff.kind(), Some(Kind::NoneNeeded));
    for seed in 0..DRAWS {
        let bytes = generated::draw(&drawn_schema, root.name, &mut Rng::new(seed));
        let generated::Decoded::Value { bytes: encoded, .. } = (root.decode)(&bytes) else {
            unreachable!("a generated draw decodes with its generated decoder");
        };
        assert_eq!(encoded, bytes, "seed {seed}");
        let decl = current.find(b"Field").expect("current Field");
        let mut value = Value::with_room(generic::ROOM);
        let node =
            value.decode(&current, decl, bytes.clone().into_boxed_slice()).expect("none needed accepts kept bytes");
        let schema::Shape::Record { first, .. } = current.declaration(decl).shape else {
            unreachable!("Field is a record");
        };
        let name = value.field(node, first).expect("name field");
        let next = schema::FieldIx::new(first.index().checked_add(1).expect("value field index"));
        let secret = value.field(node, next).expect("value field");
        let mut old = Value::with_room(generic::ROOM);
        let old_node =
            old.decode(&kept, kept.find(b"Field").expect("kept Field"), bytes.into_boxed_slice()).expect("kept draw");
        let old_name = old.field(old_node, first).expect("kept name");
        let old_value = old.field(old_node, next).expect("kept value");
        assert_eq!(value.content(name).expect("bytes"), old.content(old_name).expect("text"), "seed {seed}");
        assert_eq!(value.expose(secret).expect("secret"), old.content(old_value).expect("bytes"), "seed {seed}");
    }
}

#[test]
fn adding_unused_declaration_preserves_every_generated_root() {
    const DRAWS: u64 = 8; // Every fixture root, at eight replayable seeds.
    for family in generated::families() {
        let current_text = format!("{}\nrecord QaUnused {{ n: u8 }}\n", family.text);
        let kept = read(family.text);
        let current = read(&current_text);
        let diff = diff::diff(&kept, &current, &[]).expect("same family");
        assert_eq!(lines(&diff), ["none add QaUnused"], "{}", family.name);
        assert_ne!(kept.fingerprints().family(), current.fingerprints().family(), "{}", family.name);
        let drawn_schema = skein_codegen::parse(family.text).expect("generated draw schema");
        for root in family.roots {
            for seed in 0..DRAWS {
                let bytes = generated::draw(&drawn_schema, root.name, &mut Rng::new(seed));
                match (root.decode)(&bytes) {
                    generated::Decoded::Value { debug, bytes: encoded } => {
                        assert_eq!(encoded, bytes, "{} {}, seed {seed}", family.name, root.name);
                        assert_eq!(
                            decoded(&current, root.name, &bytes),
                            Ok(debug),
                            "{} {}, seed {seed}",
                            family.name,
                            root.name
                        );
                    }
                    generated::Decoded::Problem { path, reason } => {
                        unreachable!("a generated draw decodes: {path} {reason:?}");
                    }
                }
            }
        }
    }
}

#[test]
fn enum_append_preserves_every_old_tag_and_meaning() {
    let kept = read("family qa 1 enum E { a, b, c }");
    let current = read("family qa 1 enum E { a, b, c, d }");
    let diff = diff::diff(&kept, &current, &[]).expect("same family");
    assert_eq!(lines(&diff), ["none add E.d"]);
    for (bytes, expected) in [([0], "A"), ([1], "B"), ([2], "C")] {
        assert_eq!(decoded(&current, "E", &bytes), Ok(expected.into()));
    }
}

#[test]
fn derived_order_can_decode_bytes_with_a_different_meaning() {
    let kept = read("family qa 1 record R { a: u8 b: u8 }");
    let current = read("family qa 1 record R { b: u8 a: u8 }");
    let diff = diff::diff(&kept, &current, &[]).expect("same family");
    assert_eq!(kept.fingerprints().family(), current.fingerprints().family());
    assert_eq!(lines(&diff), ["derived order R b a"]);
    assert_eq!(decoded(&kept, "R", &[7, 9]), Ok("R { a: 7, b: 9 }".into()));
    assert_eq!(decoded(&current, "R", &[7, 9]), Ok("R { b: 7, a: 9 }".into()));
}

#[test]
fn written_byte_to_text_retype_rejects_non_utf8_kept_value() {
    let kept = read("family qa 1 record R { f: bytes 1 }");
    let current = read("family qa 1 record R { f: text 1 }");
    let diff = diff::diff(&kept, &current, &[]).expect("same family");
    assert_eq!(lines(&diff), ["written retype R.f bytes 1 text 1"]);
    assert_eq!(decoded(&kept, "R", &[0, 0, 0, 1, 255]), Ok("R { f: [255] }".into()));
    assert!(decoded(&current, "R", &[0, 0, 0, 1, 255]).is_err(), "non-UTF-8 bytes cannot be text");
}
