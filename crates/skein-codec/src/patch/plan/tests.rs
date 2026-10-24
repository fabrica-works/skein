//! Layout changes, typed values and retained run tables (codec.md, 11.2–11.3).

#![expect(clippy::disallowed_types, reason = "small schema and patch fixtures use ordinary Rust test containers")]
#![expect(clippy::disallowed_macros, reason = "table rows assemble schema and patch fixtures")]

use super::*;
use crate::patch::{Capacity, Patcher};

// Fixtures fit in 4 KiB, hold two adjacent versions and one plan.
const CAPACITY: Capacity = Capacity { versions: 3, patches: 2, text: 4096, bytes: 64, nodes: 16 };
const HEADER: &str = "patch 1\nowner o 7\nfamily f 1 to 2\n";

fn kept(old: &str, new: &str) -> Patcher {
    let mut patcher = Patcher::new(CAPACITY);
    patcher.keep(format!("family f 1\n{old}").as_bytes()).expect("old schema");
    patcher.keep(format!("family f 2\n{new}").as_bytes()).expect("new schema");
    patcher
}

fn planned(old: &str, new: &str, lines: &str) -> Plan {
    let mut patcher = kept(old, new);
    patcher.plan(format!("{HEADER}{lines}").as_bytes()).expect("valid patch").expect("family block");
    let tables = core::mem::replace(&mut patcher.build.tables.plans, List::with_capacity(0));
    tables.into_boxed().into_iter().next().expect("one plan")
}

fn refused(old: &str, new: &str, lines: &str) -> PatchError {
    kept(old, new).plan(format!("{HEADER}{lines}").as_bytes()).expect_err("patch refused")
}

fn missing(op: Op, target: Target) -> PatchWrong {
    PatchWrong::Missing(Missing { op, target })
}
const D: Target = Target::Declaration(DeclIx::new(0));
const F: Target = Target::Field(FieldIx::new(0));
const V: Target = Target::Variant(VariantIx::new(0));

struct ChangeRow {
    name: &'static str,
    old: &'static str,
    new: &'static str,
    lines: &'static str,
    without: &'static str,
    op: Op,
    target: Target,
}

const CHANGES: &[ChangeRow] = &[
    ChangeRow {
        name: "add declaration",
        old: "",
        new: "record D {}",
        lines: "add D\n",
        without: "",
        op: Op::Add,
        target: D,
    },
    ChangeRow {
        name: "drop declaration",
        old: "record D {}",
        new: "",
        lines: "drop D\n",
        without: "",
        op: Op::Drop,
        target: D,
    },
    ChangeRow {
        name: "reshape",
        old: "record D {}",
        new: "enum D { a }",
        lines: "reshape D\nwritten D\n",
        without: "written D\n",
        op: Op::Reshape,
        target: D,
    },
    ChangeRow {
        name: "versioned",
        old: "record D {}",
        new: "versioned record D {}",
        lines: "versioned D\n",
        without: "",
        op: Op::Versioned,
        target: D,
    },
    ChangeRow {
        name: "unversioned",
        old: "versioned record D {}",
        new: "record D {}",
        lines: "unversioned D\n",
        without: "",
        op: Op::Unversioned,
        target: D,
    },
    ChangeRow {
        name: "add field",
        old: "record D {}",
        new: "record D { f: u8 }",
        lines: "add D.f u8 1\n",
        without: "",
        op: Op::Add,
        target: F,
    },
    ChangeRow {
        name: "drop field",
        old: "record D { f: u8 }",
        new: "record D {}",
        lines: "drop D.f\n",
        without: "",
        op: Op::Drop,
        target: F,
    },
    ChangeRow {
        name: "widen",
        old: "record D { f: u8 }",
        new: "record D { f: u16 }",
        lines: "widen D.f u8 u16\n",
        without: "",
        op: Op::Widen,
        target: F,
    },
    ChangeRow {
        name: "wrap inner",
        old: "record D { f: list 8 text 32 }",
        new: "record D { f: list 8 option text 32 }",
        lines: "wrap D.f list 8 text 32 list 8 option text 32\n",
        without: "",
        op: Op::Wrap,
        target: F,
    },
    ChangeRow {
        name: "retype",
        old: "record D { f: u8 }",
        new: "record D { f: bool }",
        lines: "retype D.f u8 bool\nwritten D\n",
        without: "written D\n",
        op: Op::Retype,
        target: F,
    },
    ChangeRow {
        name: "order fields",
        old: "record D { a: u8 b: u8 }",
        new: "record D { b: u8 a: u8 }",
        lines: "order D b a\n",
        without: "",
        op: Op::Order,
        target: D,
    },
    ChangeRow {
        name: "order variants",
        old: "enum D { a b }",
        new: "enum D { b a }",
        lines: "order D b a\n",
        without: "",
        op: Op::Order,
        target: D,
    },
    ChangeRow {
        name: "insert variant",
        old: "enum D { a }",
        new: "enum D { b a }",
        lines: "add D.b\n",
        without: "",
        op: Op::Add,
        target: V,
    },
    ChangeRow {
        name: "drop variant",
        old: "enum D { a b }",
        new: "enum D { b }",
        lines: "drop D.a\nwritten D\n",
        without: "written D\n",
        op: Op::Drop,
        target: V,
    },
    ChangeRow {
        name: "payload",
        old: "record R {} enum D { a }",
        new: "record R {} enum D { a: R }",
        lines: "payload D.a - R\nwritten D\n",
        without: "written D\n",
        op: Op::Payload,
        target: V,
    },
];

#[test]
fn each_layout_change_accepts_its_line() {
    for row in CHANGES {
        planned(row.old, row.new, row.lines);
    }
}

#[test]
fn each_layout_change_requires_its_line_in_diff_order() {
    for row in CHANGES {
        let error = refused(row.old, row.new, row.without);
        assert_eq!(
            error,
            super::super::error(3, Span { start: 18, end: 24 }, missing(row.op, row.target)),
            "{}",
            row.name
        );
    }
}

#[test]
fn a_line_requires_the_change_it_names() {
    let rows = [
        ("add declaration", "record D {}", "record D {}", "add D\n"),
        ("drop declaration", "record D {}", "record D {}", "drop D\n"),
        ("reshape", "record D {}", "record D {}", "reshape D\n"),
        ("versioned", "versioned record D {}", "versioned record D {}", "versioned D\n"),
        ("unversioned", "record D {}", "record D {}", "unversioned D\n"),
        ("add field", "record D { f: u8 }", "record D { f: u8 }", "add D.f u8 1\n"),
        ("drop field", "record D { f: u8 }", "record D { f: u8 }", "drop D.f\n"),
        ("widen", "record D { f: u8 }", "record D { f: u8 }", "widen D.f u8 u8\n"),
        ("wrap", "record D { f: u8 }", "record D { f: u16 }", "wrap D.f u8 u16\n"),
        ("retype", "record D { f: u8 }", "record D { f: u16 }", "retype D.f u8 u16\n"),
        ("order", "record D { a: u8 b: u8 }", "record D { b: u8 a: u8 }", "order D a b\n"),
        ("add variant", "enum D { a }", "enum D { a }", "add D.a\n"),
        ("drop variant", "enum D { a }", "enum D { a }", "drop D.a\n"),
        ("payload", "enum D { a }", "enum D { a }", "payload D.a - -\n"),
    ];
    for (name, old, new, lines) in rows {
        assert_eq!(refused(old, new, lines).wrong, PatchWrong::Mismatch, "{name}");
    }
}

#[test]
fn implicit_bound_widen_and_appended_variant_need_no_line() {
    let rows = [
        ("bound", "record D { f: text 8 }", "record D { f: text 9 }", "widen D.f text 8 text 9\n"),
        ("variant", "enum D { a }", "enum D { a b }", "add D.b\n"),
    ];
    for (_, old, new, line) in rows {
        let explicit = planned(old, new, line);
        let implicit = planned(old, new, "");
        assert_eq!(
            explicit.declarations.get(0).expect("conversion").copies,
            implicit.declarations.get(0).expect("conversion").copies
        );
    }
}

#[test]
fn written_changes_require_a_site_after_their_line() {
    let rows = [
        ("reshape", "record D {}", "enum D { a }", "reshape D\n", D),
        ("retype", "record D { f: u8 }", "record D { f: bool }", "retype D.f u8 bool\n", F),
        ("drop variant", "enum D { a b }", "enum D { b }", "drop D.a\n", D),
        (
            "payload",
            "record R {} enum D { a }",
            "record R {} enum D { a: R }",
            "payload D.a - R\n",
            Target::Declaration(DeclIx::new(1)),
        ),
    ];
    for (name, old, new, line, target) in rows {
        assert_eq!(refused(old, new, line).wrong, missing(Op::Written, target), "{name}");
    }
}

#[test]
fn a_retype_can_use_a_field_site() {
    let plan = planned("record D { f: u8 }", "record D { f: bool }", "retype D.f u8 bool\nwritten D.f\n");
    assert_eq!(plan.sources.as_slice(), [Source::Site(0)]);
    assert_eq!(plan.fields.as_slice(), [false]);
}

#[test]
fn confirmed_renames_match_declarations_fields_and_variants_first() {
    let plan = planned(
        "record A { a: u8 b: u16 } record B {} enum E { x y }",
        "record A {} record B { a: u16 b: u16 } enum E { x y }",
        "rename A B\nrename B A\nrename B.a b\nrename B.b a\nwiden B.b u8 u16\norder B a b\nrename E.x y\nrename E.y x\norder E x y\n",
    );
    assert_eq!(plan.olds.as_slice(), [Some(DeclIx::new(1)), Some(DeclIx::new(0)), Some(DeclIx::new(2))]);
    assert_eq!(plan.sources.as_slice(), [Source::Old(FieldIx::new(1)), Source::Old(FieldIx::new(0))]);
    assert_eq!(plan.variants.as_slice(), [Some(VariantIx::new(1)), Some(VariantIx::new(0))]);
}

#[test]
fn a_rename_then_widen_is_one_operation_of_each_kind() {
    planned("record D { a: u8 }", "record D { b: u16 }", "rename D.a b\nwiden D.b u8 u16\n");
}

#[test]
fn duplicate_operations_and_renames_are_refused() {
    let rows = [
        ("operation", "record D { f: u8 }", "record D { f: u16 }", "widen D.f u8 u16\nwiden D.f u8 u16\n"),
        ("site", "record D {}", "record D {}", "written D\nwritten D\n"),
        ("declaration rename", "record D {}", "record E {}", "rename D E\nrename D E\n"),
        ("field rename", "record D { a: u8 }", "record D { b: u8 }", "rename D.a b\nrename D.a b\n"),
        ("variant rename", "enum D { a }", "enum D { b }", "rename D.a b\nrename D.a b\n"),
        ("order", "record D { a: u8 b: u8 }", "record D { b: u8 a: u8 }", "order D b a\norder D b a\n"),
    ];
    for (name, old, new, lines) in rows {
        assert_eq!(refused(old, new, lines).wrong, PatchWrong::Twice, "{name}");
    }
}

#[test]
fn unknown_names_are_checked_in_their_own_text() {
    let rows = [
        ("drop", "drop Missing\n"),
        ("old rename", "rename Missing D\n"),
        ("new rename", "rename D Missing\n"),
        ("new declaration", "written Missing\n"),
        ("old part", "drop D.absent\n"),
        ("new part", "written D.absent\n"),
    ];
    for (name, lines) in rows {
        assert_eq!(refused("record D { f: u8 }", "record D { f: u8 }", lines).wrong, PatchWrong::Unknown, "{name}");
    }
}

#[test]
fn mismatched_types_payloads_and_unmatched_renames_are_refused() {
    let rows = [
        ("old type", "record D { f: u8 }", "record D { f: u16 }", "widen D.f u16 u16\n"),
        ("new type", "record D { f: u8 }", "record D { f: u16 }", "widen D.f u8 u32\n"),
        ("added type", "record D {}", "record D { f: u16 }", "add D.f u8 1\n"),
        ("same rename", "record D {}", "record D {}", "rename D D\n"),
        ("unmatched part", "record A { a: u8 }", "record D { b: u8 }", "rename D.a b\n"),
        ("payload name", "record R {} enum D { a }", "record R {} enum D { a: R }", "payload D.a - Wrong\n"),
    ];
    for (name, old, new, lines) in rows {
        assert_eq!(refused(old, new, lines).wrong, PatchWrong::Mismatch, "{name}");
    }
}

#[test]
fn versions_must_be_kept_and_adjacent() {
    let rows = [
        ("unkept", "family f 1 to 3", PatchWrong::Unkept),
        ("same", "family f 1 to 1", PatchWrong::Version),
        ("backward", "family f 2 to 1", PatchWrong::Version),
    ];
    for (name, block, wrong) in rows {
        assert_eq!(
            kept("", "").plan(format!("patch 1\nowner o 7\n{block}\n").as_bytes()).expect_err(name),
            super::super::error(3, Span { start: 18, end: 24 }, wrong)
        );
    }
}

#[test]
fn site_targets_and_written_values_must_have_evidence() {
    let rows = [
        ("added declaration", "", "record D {}", "written D\n"),
        ("field of added", "", "record D { f: u8 }", "written D.f\n"),
        ("variant", "enum D { a }", "enum D { a }", "written D.a\n"),
        ("field after declaration", "record D { f: u8 }", "record D { f: u8 }", "written D\nwritten D.f\n"),
        ("declaration after field", "record D { f: u8 }", "record D { f: u8 }", "written D.f\nwritten D\n"),
        ("written without site", "record D {}", "record D { f: u8 }", "add D.f u8 written\n"),
        ("literal with site", "record D {}", "record D { f: u8 }", "written D.f\nadd D.f u8 1\n"),
    ];
    for (name, old, new, lines) in rows {
        assert_eq!(refused(old, new, lines).wrong, PatchWrong::Site, "{name}");
    }
}

#[test]
fn written_values_accept_field_or_declaration_sites_in_either_order() {
    for lines in [
        "add D.f u8 written\nwritten D.f\n",
        "written D.f\nadd D.f u8 written\n",
        "add D.f u8 written\nwritten D\n",
        "written D\nadd D.f u8 written\n",
    ] {
        planned("record D {}", "record D { f: u8 }", lines);
    }
}

#[test]
fn the_first_wrong_line_precedes_a_later_wrong_name() {
    let rows = [
        ("written value", "record D {}", "record D { f: u8 }", "add D.f u8 written\nwritten Missing\n"),
        ("field site", "record D {}", "record D { f: u8 }", "written D.f\nwritten Missing\nadd D.f u8 1\n"),
    ];
    for (name, old, new, lines) in rows {
        let error = refused(old, new, lines);
        assert_eq!(error.line, 4, "{name}");
        assert_eq!(error.wrong, PatchWrong::Site, "{name}");
    }
}

struct ValueRow {
    name: &'static str,
    ty: &'static str,
    value: &'static str,
    bytes: &'static [u8],
}
const PRELUDE: &str = "record R {} enum E { plain payload: R last none some written }";

fn value_plan(ty: &str, value: &str) -> Plan {
    planned(
        &format!("{PRELUDE} record D {{}}"),
        &format!("{PRELUDE} record D {{ f: {ty} }}"),
        &format!("add D.f {ty} {value}\n"),
    )
}

#[test]
fn added_values_are_encoded_once_in_codec_bytes() {
    let rows = [
        ValueRow { name: "u8 max", ty: "u8", value: "255", bytes: &[255] },
        ValueRow { name: "u16 max", ty: "u16", value: "65535", bytes: &[255, 255] },
        ValueRow { name: "u32 order", ty: "u32", value: "16909060", bytes: &[1, 2, 3, 4] },
        ValueRow { name: "u64 order", ty: "u64", value: "72623859790382856", bytes: &[1, 2, 3, 4, 5, 6, 7, 8] },
        ValueRow { name: "u32 max", ty: "u32", value: "4294967295", bytes: &[255, 255, 255, 255] },
        ValueRow { name: "u64 max", ty: "u64", value: "18446744073709551615", bytes: &[255; 8] },
        ValueRow { name: "duration max", ty: "duration", value: "18446744073709551615", bytes: &[255; 8] },
        ValueRow { name: "true", ty: "bool", value: "true", bytes: &[1] },
        ValueRow { name: "false", ty: "bool", value: "false", bytes: &[0] },
        ValueRow { name: "fixed", ty: "fixed 2", value: "0x00ff", bytes: &[0, 255] },
        ValueRow { name: "bytes at bound", ty: "bytes 2", value: "0x00ff", bytes: &[0, 0, 0, 2, 0, 255] },
        ValueRow { name: "bytes empty", ty: "bytes 0", value: "0x", bytes: &[0, 0, 0, 0] },
        ValueRow { name: "pieces empty", ty: "bytes 8 pieces R", value: "0x", bytes: &[0, 0, 0, 0] },
        ValueRow { name: "text at bound", ty: "text 2", value: "\"é\"", bytes: &[0, 0, 0, 2, 0xc3, 0xa9] },
        ValueRow { name: "hash text", ty: "text 1", value: "\"#\"", bytes: &[0, 0, 0, 1, b'#'] },
        ValueRow { name: "empty list", ty: "list 0 u8", value: "[]", bytes: &[0, 0, 0, 0] },
        ValueRow { name: "none", ty: "option u8", value: "none", bytes: &[0] },
        ValueRow { name: "some", ty: "option u16", value: "some 258", bytes: &[1, 1, 2] },
        ValueRow { name: "nested options", ty: "option option u8", value: "some some 255", bytes: &[1, 1, 255] },
        ValueRow { name: "some none", ty: "option option u8", value: "some none", bytes: &[1, 0] },
        ValueRow { name: "enumeration", ty: "E", value: "plain", bytes: &[0] },
        ValueRow { name: "variant tag", ty: "E", value: "last", bytes: &[2] },
    ];
    for row in rows {
        let plan = value_plan(row.ty, row.value);
        assert_eq!(plan.pool.as_slice(), row.bytes, "{}", row.name);
        assert_eq!(
            plan.sources.as_slice(),
            [Source::Value { start: 0, end: u32::try_from(row.bytes.len()).expect("fixture length") }],
            "{}",
            row.name
        );
    }
}

#[test]
fn added_values_refuse_the_other_side_of_each_bound() {
    let rows = [
        ("u8 overflow", "u8", "256"),
        ("u16 overflow", "u16", "65536"),
        ("u32 overflow", "u32", "4294967296"),
        ("u64 overflow", "u64", "18446744073709551616"),
        ("duration overflow", "duration", "18446744073709551616"),
        ("bool", "bool", "1"),
        ("fixed short", "fixed 2", "0x00"),
        ("fixed long", "fixed 2", "0x000000"),
        ("bytes bound", "bytes 1", "0x00ff"),
        ("odd hex", "bytes 2", "0xf"),
        ("uppercase hex", "bytes 2", "0xFF"),
        ("text bound", "text 1", "\"é\""),
        ("text unquoted", "text 2", "hi"),
        ("pieces not empty", "bytes 8 pieces R", "0x00"),
        ("held", "bytes 8 holds other R", "0x"),
        ("secret", "secret 8", "0x"),
        ("record", "R", "R"),
        ("variant record", "E", "payload"),
        ("variant absent", "E", "absent"),
        ("reserved none", "E", "none"),
        ("reserved some", "E", "some plain"),
        ("some nonoption", "u8", "some 1"),
        ("none nonoption", "u8", "none"),
        ("option bare", "option u8", "1"),
        ("list nonempty word", "list 1 u8", "1"),
    ];
    for (name, ty, value) in rows {
        let error = refused(
            &format!("{PRELUDE} record D {{}}"),
            &format!("{PRELUDE} record D {{ f: {ty} }}"),
            &format!("add D.f {ty} {value}\n"),
        );
        assert_eq!(error.wrong, PatchWrong::Value, "{name}");
        assert_eq!(error.line, 4, "{name}");
    }
}

#[test]
fn asked_values_are_refused_without_invention() {
    for ty in ["u8", "option u8", "E"] {
        assert_eq!(
            refused(
                &format!("{PRELUDE} record D {{}}"),
                &format!("{PRELUDE} record D {{ f: {ty} }}"),
                &format!("add D.f {ty} ?\n")
            )
            .wrong,
            PatchWrong::Asked
        );
    }
}

#[test]
fn comparison_walks_wrappers_and_clauses_level_by_level() {
    let rows = [
        ("equal", "option list 2 u8", "option list 2 u8", Comparison::Equal),
        ("integer", "option list 2 u8", "option list 3 u16", Comparison::Widen { integer: true }),
        ("bound", "list 2 text 8", "list 3 text 9", Comparison::Widen { integer: false }),
        ("inner wrap", "list 2 text 8", "list 3 option text 9", Comparison::Wrap),
        ("option outermost", "option u8", "option option u8", Comparison::Wrap),
        ("option removed", "option u8", "u8", Comparison::Retype),
        ("bound narrowed", "list 2 u8", "list 1 u8", Comparison::Retype),
        ("duration", "u64", "duration", Comparison::Retype),
        ("fixed", "fixed 2", "fixed 3", Comparison::Retype),
        ("bytes text", "bytes 2", "text 2", Comparison::Retype),
        ("text bytes", "text 2", "bytes 2", Comparison::Widen { integer: false }),
        ("text secret", "text 2", "secret 2", Comparison::Widen { integer: false }),
        ("secret bytes", "secret 2", "bytes 2", Comparison::Widen { integer: false }),
        ("add holds", "bytes 2", "bytes 2 holds other R", Comparison::Widen { integer: false }),
        ("drop holds", "bytes 2 holds other R", "bytes 2", Comparison::Widen { integer: false }),
        ("holds family", "bytes 2 holds other R", "bytes 2 holds third R", Comparison::Retype),
        ("holds name", "bytes 2 holds other R", "bytes 2 holds other S", Comparison::Retype),
        ("pieces same", "bytes 2 pieces R", "bytes 2 pieces R", Comparison::Equal),
        ("pieces different", "bytes 2 pieces R", "bytes 2 pieces S", Comparison::Retype),
        ("named same", "R", "R", Comparison::Equal),
        ("named different", "R", "S", Comparison::Retype),
    ];
    for (name, a, b, expected) in rows {
        let old =
            crate::schema::parse(format!("family f 1 record R {{}} record S {{}} record D {{ f: {a} }}").as_bytes())
                .expect(name);
        let new =
            crate::schema::parse(format!("family f 2 record R {{}} record S {{}} record D {{ f: {b} }}").as_bytes())
                .expect(name);
        let parsed = text::parse(HEADER.as_bytes()).expect("empty block");
        let mut errors = List::with_capacity(0);
        let matching =
            matching(&old, &new, HEADER.as_bytes(), &parsed, *parsed.blocks.get(0).expect("block"), &mut errors);
        assert_eq!(
            compare(&matching.declarations, &old, &new, old.field(FieldIx::new(0)).ty, new.field(FieldIx::new(0)).ty),
            expected,
            "{name}"
        );
    }
}

fn flags(plan: &Plan) -> Vec<(bool, bool)> {
    let mut flags = Vec::new();
    for conversion in &plan.declarations {
        flags.push((conversion.copies, conversion.stops));
    }
    flags
}

#[test]
fn copies_and_stops_follow_nested_declarations() {
    let old = "record Copy { a: text 8 } record Added {} record Moved { a: u8 b: u8 } enum Tags { a b } versioned record Version {} record Held { a: bytes 8 holds other R } record Parent { a: option list 2 Held } record Site {}";
    let new = "record Copy { a: text 9 } record Added { a: u8 } record Moved { b: u8 a: u8 } enum Tags { b a } versioned record Version {} record Held { a: bytes 8 holds other R } record Parent { a: option list 2 Held } record Site {}";
    let plan = planned(old, new, "add Added.a u8 1\norder Moved b a\norder Tags b a\nwritten Site\n");
    assert_eq!(
        flags(&plan),
        [
            (true, false),
            (false, false),
            (false, false),
            (false, false),
            (false, false),
            (false, true),
            (false, true),
            (false, true)
        ]
    );
}

#[test]
fn written_addition_disables_carry_of_its_record() {
    let plan = planned("record D { a: u8 }", "record D { a: u8 f: u8 }", "add D.f u8 written\nwritten D\n");
    assert!(!plan.declarations.get(0).expect("conversion").derived);
    assert_eq!(plan.sources.as_slice(), [Source::Old(FieldIx::new(0)), Source::Written]);
}

#[test]
fn reshape_and_payload_disable_their_old_parts_derived_flags() {
    let reshaped = planned("record D { a: u8 }", "enum D { b }", "reshape D\nwritten D\n");
    assert_eq!(reshaped.fields.as_slice(), [false]);
    assert!(!reshaped.declarations.get(0).expect("conversion").derived);
    let changed =
        planned("record R {} enum E { a b }", "record R {} enum E { a: R }", "payload E.a - R\ndrop E.b\nwritten E\n");
    assert_eq!(changed.variant_derived.as_slice(), [false, false]);
    assert_eq!(changed.variants.as_slice(), [Some(VariantIx::new(0)), None]);
}

#[test]
fn identity_copies_versioned_records_and_stops_at_held_fields() {
    let mut patcher =
        kept("versioned record Copy {} record Held { f: bytes 8 holds other R } record Nested { f: Held }", "");
    patcher.identity(1).expect("identity");
    let plan = patcher.build.tables.plans.get(0).expect("identity plan");
    assert_eq!(flags(plan), [(true, false), (false, true), (false, true)]);
    assert_eq!(plan.fields.as_slice(), [true, true]);
}

#[test]
fn pieces_require_their_items_conversion_to_copy() {
    let rows = [
        (
            "integer",
            "record R { a: u8 } record D { p: bytes 32 pieces R }",
            "record R { a: u16 } record D { p: bytes 32 pieces R }",
            "widen R.a u8 u16\n",
        ),
        (
            "held",
            "record R { a: bytes 8 holds other X } record D { p: bytes 32 pieces R }",
            "record R { a: bytes 8 holds other X } record D { p: bytes 32 pieces R }",
            "",
        ),
    ];
    for (name, old, new, lines) in rows {
        assert_eq!(refused(old, new, lines).wrong, PatchWrong::Pieces, "{name}");
    }
}

#[test]
fn copying_pieces_and_unreached_pieces_are_accepted() {
    planned(
        "record R { a: text 8 } record D { p: bytes 32 pieces R }",
        "record R { a: text 9 } record D { p: bytes 32 pieces R }",
        "",
    );
    planned(
        "record R { a: u8 } record D { p: bytes 32 pieces R }",
        "record R { a: u16 } record D {}",
        "widen R.a u8 u16\ndrop D.p\n",
    );
    planned(
        "record R { a: u8 } record D { p: bytes 32 pieces R }",
        "record R { a: u16 } record D { p: bytes 32 pieces R }",
        "widen R.a u8 u16\nwritten D\n",
    );
}

#[test]
fn identity_pieces_refuse_items_reaching_held_fields() {
    let mut patcher = kept("record R { a: bytes 8 holds other X } record D { p: bytes 32 pieces R }", "");
    assert_eq!(patcher.identity(1), Err(PatchWrong::Pieces));
}

#[test]
fn plan_retains_global_refusals_marks_and_its_own_sites() {
    const TEXT: &[u8] = b"patch 1\nowner o 7\nmark mark\nmark fresh\nfamily other 1 to 2\nwritten X\nrefuses a \"other\"\nfamily f 1 to 2\nwritten D\nrefuses b \"first#\"\nrefuses c \"second\"\n";
    let mut patcher = kept("record D {}", "record D {}");
    patcher.plan(TEXT).expect("valid patch");
    let plan = patcher.build.tables.plans.get(0).expect("plan");
    assert_eq!((plan.patch, plan.old, plan.new), (7, 0, 1));
    assert_eq!(plan.text.as_ref(), TEXT);
    let mut marks = Vec::new();
    for &span in &plan.marks {
        marks.push(spelled(&plan.text, span));
    }
    assert_eq!(marks, [b"mark".as_slice(), b"fresh".as_slice()]);
    let site = plan.sites.get(0).expect("own site");
    assert_eq!((site.index, site.target, site.refusals), (1, D, (1, 3)));
    let mut refusals = Vec::new();
    for r in &plan.refusals {
        refusals.push((spelled(&plan.text, r.name), spelled(&plan.text, r.text), r.site));
    }
    assert_eq!(
        refusals,
        [
            (b"a".as_slice(), b"other".as_slice(), 0),
            (b"b".as_slice(), b"first#".as_slice(), 1),
            (b"c".as_slice(), b"second".as_slice(), 1)
        ]
    );
}

#[test]
fn held_stops_require_the_same_other_family_in_both_texts() {
    let rows = [
        ("same family", "bytes 8 holds f R", "bytes 8 holds f R"),
        ("clause added", "bytes 8", "bytes 8 holds other R"),
        ("clause removed", "bytes 8 holds other R", "bytes 8"),
    ];
    for (name, a, b) in rows {
        let plan = planned(&format!("record D {{ f: {a} }}"), &format!("record D {{ f: {b} }}"), "");
        assert_eq!(flags(&plan), [(true, false)], "{name}");
    }
}

#[test]
fn invalid_utf8_values_name_the_quoted_token() {
    let text = b"patch 1\nowner o 7\nfamily f 1 to 2\nadd D.f text 1 \"\xff\"\n";
    assert_eq!(
        kept("record D {}", "record D { f: text 1 }").plan(text),
        Err(super::super::error(4, Span { start: 49, end: 52 }, PatchWrong::Value))
    );
}

#[test]
fn declaration_order_and_renamed_named_types_still_copy() {
    let plan = planned(
        "record A { f: u8 } record B {} record D { a: A }",
        "record B {} record Z { f: u8 } record D { a: Z }",
        "rename A Z\n",
    );
    assert_eq!(flags(&plan), [(true, false), (true, false), (true, false)]);
    assert_eq!(plan.olds.as_slice(), [Some(DeclIx::new(1)), Some(DeclIx::new(0)), Some(DeclIx::new(2))]);
}

#[test]
fn matched_sites_keep_derived_fields_and_variant_payloads() {
    let plan = planned(
        "record R { a: u8 } enum E { a: R b }",
        "record R { a: u8 } enum E { a: R b }",
        "written R.a\nwritten E\n",
    );
    assert_eq!(plan.fields.as_slice(), [true]);
    assert_eq!(plan.variant_derived.as_slice(), [true, true]);
    assert_eq!(plan.sources.as_slice(), [Source::Site(0)]);
}

#[test]
fn a_failed_plan_does_not_reserve_its_version() {
    let mut patcher = kept("record D { f: u8 }", "record D { f: u16 }");
    patcher.plan(HEADER.as_bytes()).expect_err("missing widening");
    let plan =
        patcher.plan(format!("{HEADER}widen D.f u8 u16\n").as_bytes()).expect("corrected patch").expect("family block");
    assert_eq!(plan.index(), 0);
}

#[test]
fn missing_changes_follow_new_then_old_text_order() {
    const OLD: &str = "record Gone {} record D { old: u8 a: u8 }";
    const NEW: &str = "record Added {} record D { a: u16 f: u8 }";
    let rows = [
        ("declaration first", "", Op::Add, Target::Declaration(DeclIx::new(0))),
        ("matched field", "add Added\n", Op::Widen, Target::Field(FieldIx::new(0))),
        ("added field", "add Added\nwiden D.a u8 u16\n", Op::Add, Target::Field(FieldIx::new(1))),
        ("dropped part", "add Added\nwiden D.a u8 u16\nadd D.f u8 0\n", Op::Drop, Target::Field(FieldIx::new(0))),
        (
            "dropped declaration",
            "add Added\nwiden D.a u8 u16\nadd D.f u8 0\ndrop D.old\n",
            Op::Drop,
            Target::Declaration(DeclIx::new(0)),
        ),
    ];
    for (name, lines, op, target) in rows {
        assert_eq!(refused(OLD, NEW, lines).wrong, missing(op, target), "{name}");
    }
}

#[test]
fn other_family_grammar_is_checked_after_the_own_block() {
    let text = b"patch 1\nowner o 7\nfamily f 1 to 2\nfamily other 1 to 2\nbogus\n";
    assert_eq!(
        kept("", "").plan(text),
        Err(super::super::error(5, Span { start: 54, end: 59 }, PatchWrong::Expected(crate::patch::Expected::Line)))
    );
}

#[test]
fn added_field_sources_share_one_value_pool() {
    let plan = planned("record D {}", "record D { a: u8 b: text 1 }", "add D.b text 1 \"x\"\nadd D.a u8 7\n");
    assert_eq!(plan.pool.as_slice(), [0, 0, 0, 1, b'x', 7]);
    assert_eq!(plan.sources.as_slice(), [Source::Value { start: 5, end: 6 }, Source::Value { start: 0, end: 5 }]);
}
