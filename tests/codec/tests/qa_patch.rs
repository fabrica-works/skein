//! Black-box QA of codec.md, section 11.

#![cfg(test)]

use skein_codec::patch::{
    self, Capacity, Halt, Missing, Op, PatchWrong, Patcher, PlanIx, RefusalIx, Site, Step, Target,
};
use skein_codec::schema::{DeclIx, FieldIx};
use skein_codec::value::{Refusal, Value};
use skein_codec_tests::patches;
use skein_lib::Rng;

#[path = "support/generic.rs"]
mod generic;
mod support;

// Three kept versions and three plans cover a two-pass chain and an identity.
const CAPACITY: Capacity = Capacity { versions: 3, patches: 3, text: 8192, bytes: 4096, nodes: generic::ROOM };
const ROOT: DeclIx = DeclIx::new(0);
const FIRST: FieldIx = FieldIx::new(0);
const SECOND: FieldIx = FieldIx::new(1);
const EMPTY: &str = "record R {}";
const BYTE: &str = "record R { x: u8 }";
const WRITTEN: &str = "written R.x\n";
const HELD: &str = "record R { x: bytes 4 holds other Thing }";
const HEADER: &str = "patch 1\nowner qa 7\n";

fn machine(capacity: Capacity, bodies: &[&str]) -> Patcher {
    let mut patcher = Patcher::new(capacity);
    for (offset, body) in bodies.iter().enumerate() {
        let version = offset.checked_add(1).expect("small version");
        patcher.keep(format!("family qa {version}\n{body}\n").as_bytes()).expect("valid QA schema");
    }
    patcher
}

fn text(lines: &str) -> String {
    format!("{HEADER}family qa 1 to 2\n{lines}")
}

fn fixture(old: &str, new: &str, lines: &str) -> (Patcher, PlanIx) {
    let mut patcher = machine(CAPACITY, &[old, new]);
    let plan = patcher.plan(text(lines).as_bytes()).expect("valid QA patch").expect("QA family block");
    (patcher, plan)
}

fn start(patcher: &mut Patcher, plan: PlanIx, name: &[u8], bytes: &[u8]) {
    let version = patcher.versions(plan).from;
    let root = patcher.schema(version).expect("kept schema").find(name).expect("root declared");
    patcher.start(plan, root, bytes.into()).expect("valid old record");
}

fn output(patcher: &mut Patcher, plan: PlanIx, name: &[u8], expected: &[u8]) {
    let bytes = patcher.finish().expect("completed record").to_vec();
    let schema = patcher.schema(patcher.versions(plan).to).expect("kept new schema");
    let root = schema.find(name).expect("new root");
    let mut value = Value::with_room(CAPACITY.nodes);
    let decoded = value.decode(schema, root, bytes.clone().into_boxed_slice());
    if let Err(Refusal::Problem(problem)) = decoded {
        assert_eq!(generic::path_name(schema, problem.path), "valid new record", "{problem:?}");
    }
    let node = decoded.expect("output decodes by the new schema");
    assert_eq!(bytes, expected, "{}", generic::render(schema, &value, node, root));
}

fn written(patcher: &mut Patcher) -> (Site, skein_codec::value::Node) {
    let step = patcher.next().expect("written stop");
    assert!(matches!(step, Step::Written { .. }), "expected written stop, got {step:?}");
    let Step::Written { site, at } = step else { unreachable!("checked written stop") };
    (site, at)
}

#[test]
fn header_reads_levels_marks_without_reading_block_grammar() {
    // codec.md 11.1: header reads its lines alone, including a header-only patch.
    for tail in ["", "family elsewhere 1 to\nadd\n"] {
        let patch = format!("patch 1\nowner qa 4294967295 after smith 0 skein 9\nmark fresh\nmark freshish\n{tail}");
        let header = patch::header(patch.as_bytes()).expect("header independent of body grammar");
        assert_eq!(header.level(), u32::MAX);
        assert_eq!(header.after(patch.as_bytes(), b"smith"), Some(0));
        assert_eq!(header.after(patch.as_bytes(), b"skein"), Some(9));
        assert_eq!(header.after(patch.as_bytes(), b"missing"), None);
        assert!(header.marked(patch.as_bytes(), b"fresh"), "exact mark found");
        assert!(!header.marked(patch.as_bytes(), b"fres"), "mark prefixes are not marks");
        let span = header.owner();
        assert_eq!(
            patch.as_bytes().get(usize::try_from(span.start).unwrap()..usize::try_from(span.end).unwrap()),
            Some(b"qa".as_slice())
        );
    }
}

#[test]
fn foreign_blocks_are_checked_for_grammar() {
    // codec.md 11.6: plan reads the whole text, even after its own family.
    for tail in ["family other 1 to 2\nadd R.x u8\n", "family other 1 to 2\nunknown R\n"] {
        let mut patcher = machine(CAPACITY, &[EMPTY, EMPTY]);
        let patch = format!("{}{tail}", text(""));
        let error = patcher.plan(patch.as_bytes()).expect_err("foreign grammar must be checked");
        assert!(matches!(error.wrong, PatchWrong::Expected(_)), "{error:?}");
        assert_eq!(error.line, 5);
    }
}

#[test]
fn foreign_blocks_need_no_kept_layout_or_semantic_match() {
    // codec.md 11.1: other families are read for grammar alone.
    let mut patcher = machine(CAPACITY, &[EMPTY, EMPTY]);
    let patch =
        format!("{HEADER}family other 45 to 46\nretype Absent.x u8 text 1\nwritten Absent.x\nfamily qa 1 to 2\n");
    let plan = patcher.plan(patch.as_bytes()).expect("foreign layouts are not ours").expect("our plan");
    start(&mut patcher, plan, b"R", &[]);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[]);
}

#[test]
fn missing_changes_name_the_family_line_and_new_field() {
    // codec.md 11.1, 11.3: a missing line names the family's line and target.
    let mut patcher = machine(CAPACITY, &[EMPTY, BYTE]);
    let patch = format!("{HEADER}# spacer\n\nfamily qa 1 to 2\n");
    let error = patcher.plan(patch.as_bytes()).expect_err("added field needs a line");
    assert_eq!(error.line, 5);
    assert_eq!(error.wrong, PatchWrong::Missing(Missing { op: Op::Add, target: Target::Field(FIRST) }));
    assert_eq!(
        patch.as_bytes().get(usize::try_from(error.at.start).unwrap()..usize::try_from(error.at.end).unwrap()),
        Some(b"family".as_slice())
    );
}

#[test]
fn refused_plan_leaves_the_only_plan_slot_available() {
    // codec.md 11.3: only a planned conversion occupies a plan slot.
    let capacity = Capacity { patches: 1, ..CAPACITY };
    let mut patcher = machine(capacity, &[EMPTY, BYTE]);
    let error = patcher.plan(text("add R.x u8 ?\n").as_bytes()).expect_err("no invented value");
    assert_eq!(error.wrong, PatchWrong::Asked);
    let plan = patcher.plan(text("add R.x u8 255\n").as_bytes()).expect("retry after refusal").expect("plan");
    start(&mut patcher, plan, b"R", &[]);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[255]);
}

#[test]
fn implicit_bound_widening_and_appended_variant_preserve_bytes() {
    // codec.md 11.3: non-integer widening and variants appended at the end need no line.
    let (mut patcher, plan) =
        fixture("enum E { a } record R { x: bytes 1 e: E }", "enum E { a b } record R { x: bytes 2 e: E }", "");
    let bytes = [0, 0, 0, 1, 255, 0];
    start(&mut patcher, plan, b"R", &bytes);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &bytes);
}

#[test]
fn added_literal_values_encode_at_their_bounds() {
    // codec.md 11.2: literal bounds count bytes; nested some/none and empty containers are values.
    let rows: &[(&str, &str, &[u8])] = &[
        ("u8", "255", &[255]),
        ("u16", "65535", &[255, 255]),
        ("u32", "4294967295", &[255, 255, 255, 255]),
        ("u64", "18446744073709551615", &[255; 8]),
        ("duration", "18446744073709551615", &[255; 8]),
        ("bool", "false", &[0]),
        ("fixed 2", "0x00ff", &[0, 255]),
        ("bytes 2", "0x00ff", &[0, 0, 0, 2, 0, 255]),
        ("bytes 0", "0x", &[0, 0, 0, 0]),
        ("text 3", "\"é#\"", &[0, 0, 0, 3, 0xc3, 0xa9, b'#']),
        ("list 0 u8", "[]", &[0, 0, 0, 0]),
        ("option option u8", "some none", &[1, 0]),
        ("option option u8", "some some 255", &[1, 1, 255]),
    ];
    for (ty, literal, expected) in rows {
        let new = format!("record R {{ x: {ty} }}");
        let lines = format!("add R.x {ty} {literal}\n");
        let (mut patcher, plan) = fixture(EMPTY, &new, &lines);
        start(&mut patcher, plan, b"R", &[]);
        assert_eq!(patcher.next(), Ok(Step::Done));
        output(&mut patcher, plan, b"R", expected);
    }
}

#[test]
fn invalid_literals_are_refused_when_planned() {
    // codec.md 11.2: a literal past its width/bound, malformed hex, secret or held bytes is Value.
    for (ty, literal) in [
        ("u8", "256"),
        ("u64", "18446744073709551616"),
        ("bool", "2"),
        ("fixed 2", "0xff"),
        ("fixed 1", "0xFF"),
        ("bytes 1", "0x0011"),
        ("bytes 2", "0x0"),
        ("text 2", "\"é#\""),
        ("secret 1", "0x00"),
        ("bytes 1 holds other Thing", "0x"),
    ] {
        let new = format!("record R {{ x: {ty} }}");
        let mut patcher = machine(CAPACITY, &[EMPTY, &new]);
        let patch = text(&format!("add R.x {ty} {literal}\n"));
        let error = patcher.plan(patch.as_bytes()).expect_err("invalid literal");
        assert_eq!(error.wrong, PatchWrong::Value, "{ty} {literal}: {error:?}");
        assert_eq!(error.line, 4);
    }
}

#[test]
fn order_lists_only_matched_parts_using_their_new_names() {
    // codec.md 10.2, 11.3: order omits additions and uses confirmed current names.
    let (mut patcher, plan) = fixture(
        "record R { a: u8 b: u8 gone: u8 }",
        "record R { b: u8 fresh: u8 renamed: u8 }",
        "rename R.a renamed\nadd R.fresh u8 99\ndrop R.gone\norder R b renamed\n",
    );
    start(&mut patcher, plan, b"R", &[11, 22, 33]);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[22, 99, 11]);
}

#[test]
fn nested_option_wrap_keeps_absence_at_outermost_option() {
    // codec.md 10.3, 11.4: an existing option maps to the outermost new option.
    let (mut patcher, plan) = fixture(
        "record R { x: list 2 option u8 }",
        "record R { x: list 3 option option u16 }",
        "wrap R.x list 2 option u8 list 3 option option u16\n",
    );
    start(&mut patcher, plan, b"R", &[0, 0, 0, 2, 0, 1, 255]);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[0, 0, 0, 2, 0, 1, 1, 0, 255]);
}

#[test]
fn empty_versioned_records_in_lists_get_the_new_version() {
    // codec.md 11.4: nested versions are rewritten even for records with no fields.
    let body = "versioned record E {} versioned record R { x: list 3 E }";
    let (mut patcher, plan) = fixture(body, body, "");
    start(&mut patcher, plan, b"R", &[0, 1, 0, 0, 0, 3, 0, 1, 0, 1, 0, 1]);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[0, 2, 0, 0, 0, 3, 0, 2, 0, 2, 0, 2]);
}

#[test]
fn pieces_refuse_transitive_version_changes() {
    // codec.md 11.3: pieces cannot contain a conversion whose bytes must change.
    let body = "versioned record E {} record R { x: bytes 8 pieces E }";
    let mut patcher = machine(CAPACITY, &[body, body]);
    let error = patcher.plan(text("").as_bytes()).expect_err("pieces cannot rewrite nested versions");
    assert_eq!(error.wrong, PatchWrong::Pieces);
}

#[test]
fn identity_refuses_pieces_that_reach_held_fields() {
    // codec.md 11.3: an identity must still reach held fields, which pieces cannot open.
    let body = "record E { h: bytes 4 holds other Thing } record R { x: bytes 8 pieces E }";
    let mut patcher = machine(CAPACITY, &[body]);
    assert_eq!(patcher.identity(1), Err(PatchWrong::Pieces));
}

#[test]
fn written_fields_walk_in_new_order_with_old_parent_links() {
    // codec.md 11.4, 11.5: new preorder controls stops; at is the old containing record.
    let old = "record E { x: u8 } record R { a: E b: E }";
    let new = "record E { x: u8 } record R { b: E a: E }";
    let (mut patcher, plan) = fixture(old, new, "written E.x\norder R b a\n");
    start(&mut patcher, plan, b"R", &[10, 20]);
    for expected in [20, 10] {
        let (site, at) = written(&mut patcher);
        assert_eq!(site, Site::new(7, 0));
        let (old, build) = patcher.parts();
        assert_eq!(old.parent(at), Ok(Some(old.root())));
        let field = old.field(at, FIRST).unwrap();
        assert_eq!(old.uint(field), Ok(expected));
        build.carry(old, field).unwrap();
    }
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[20, 10]);
}

#[test]
fn sites_and_refusals_keep_indexes_across_foreign_blocks() {
    // codec.md 11.1, 11.5: site and refusal indexes are global, not per family.
    let mut patcher = machine(CAPACITY, &[BYTE, BYTE]);
    let patch = format!(
        "{HEADER}family other 1 to 2\nwritten X\nrefuses elsewhere \"other reason\"\nfamily qa 1 to 2\nwritten R.x\nrefuses ours \"our # reason\"\n"
    );
    let plan = patcher.plan(patch.as_bytes()).unwrap().unwrap();
    start(&mut patcher, plan, b"R", &[5]);
    let (site, _) = written(&mut patcher);
    assert_eq!(site, Site::new(7, 1));
    let reason = RefusalIx::new(1);
    assert_eq!(patcher.refusal(reason), Some(b"our # reason".as_slice()));
    assert_eq!(patcher.parts().1.refuse(reason), Halt::Refused(reason));
    assert_eq!(patcher.parts().1.put_uint(9), Err(Halt::Order));
    assert_eq!(patcher.next(), Err(Halt::Order));
}

#[test]
fn a_foreign_sites_reason_is_undeclared_at_our_site() {
    // codec.md 11.5: a refusal must belong to the current site.
    let (mut patcher, plan) = fixture(
        "record R { x: u8 y: u8 }",
        "record R { x: u8 y: u8 }",
        "written R.x\nrefuses first \"first\"\nwritten R.y\nrefuses second \"second\"\n",
    );
    start(&mut patcher, plan, b"R", &[5, 6]);
    written(&mut patcher);
    let reason = RefusalIx::new(1);
    assert_eq!(patcher.parts().1.refuse(reason), Halt::Undeclared(reason));
    assert_eq!(patcher.parts().1.put_uint(9), Err(Halt::Order));
    assert_eq!(patcher.next(), Err(Halt::Order));
}

#[test]
fn a_fresh_start_recovers_from_a_declared_refusal() {
    // codec.md 11.4: a halt ends the record; the next start begins afresh.
    let (mut patcher, plan) = fixture(BYTE, BYTE, "written R.x\nrefuses no \"no\"\n");
    start(&mut patcher, plan, b"R", &[5]);
    written(&mut patcher);
    let reason = RefusalIx::new(0);
    assert_eq!(patcher.parts().1.refuse(reason), Halt::Refused(reason));
    start(&mut patcher, plan, b"R", &[6]);
    written(&mut patcher);
    patcher.parts().1.put_uint(9).unwrap();
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[9]);
}

#[test]
fn unanswered_written_stop_halts_instead_of_skipping() {
    // codec.md 11.5: next requires exactly one value per stop.
    let (mut patcher, plan) = fixture(BYTE, BYTE, WRITTEN);
    start(&mut patcher, plan, b"R", &[5]);
    written(&mut patcher);
    assert_eq!(patcher.next(), Err(Halt::Order));
    assert_eq!(patcher.finish(), Err(Halt::Order));
}

#[test]
fn unclosed_written_value_is_not_a_completed_answer() {
    // codec.md 11.5: an opened option must be closed before next.
    let (mut patcher, plan) = fixture(BYTE, "record R { x: option u8 }", "wrap R.x u8 option u8\nwritten R.x\n");
    start(&mut patcher, plan, b"R", &[5]);
    written(&mut patcher);
    let (_, build) = patcher.parts();
    build.open().unwrap();
    build.put_uint(5).unwrap();
    assert_eq!(patcher.next(), Err(Halt::Order));
}

#[test]
fn extra_written_value_cannot_fill_the_following_field() {
    // codec.md 11.5: one value per stop, even when the following field has the same type.
    let body = "record R { x: u8 y: u8 }";
    let (mut patcher, plan) = fixture(body, body, WRITTEN);
    start(&mut patcher, plan, b"R", &[5, 6]);
    written(&mut patcher);
    let (_, build) = patcher.parts();
    build.put_uint(9).unwrap();
    assert_eq!(build.put_uint(10), Err(Halt::Order));
    assert_eq!(patcher.next(), Err(Halt::Order));
}

#[test]
fn builder_type_error_is_terminal_until_next_start() {
    // codec.md 11.4: every Halt ends the record, including a wrong builder operation.
    let (mut patcher, plan) = fixture(BYTE, BYTE, WRITTEN);
    start(&mut patcher, plan, b"R", &[5]);
    written(&mut patcher);
    let error = patcher.parts().1.put_bool(true).expect_err("u8 is not bool");
    assert!(matches!(error, Halt::Misbuilt(_)), "{error:?}");
    assert_eq!(patcher.parts().1.put_uint(9), Err(Halt::Order));
    assert_eq!(patcher.next(), Err(Halt::Order));
    start(&mut patcher, plan, b"R", &[6]);
    written(&mut patcher);
    patcher.parts().1.put_uint(9).unwrap();
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[9]);
}

#[test]
fn whole_site_can_carry_its_own_derived_conversion() {
    // codec.md 11.5: carry(old, at) at a declaration's site does not revisit the site.
    let (mut patcher, plan) =
        fixture(BYTE, "versioned record R { x: u16 }", "versioned R\nwiden R.x u8 u16\nwritten R\n");
    start(&mut patcher, plan, b"R", &[255]);
    let (_, at) = written(&mut patcher);
    let (old, build) = patcher.parts();
    build.carry(old, at).unwrap();
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[0, 2, 0, 255]);
}

#[test]
fn carry_refuses_a_descendant_with_a_written_site() {
    // codec.md 11.5, 13: carry is immediate and cannot run a descendant's site.
    let body = "record E { x: u8 } record R { e: E }";
    let (mut patcher, plan) = fixture(body, body, "written E.x\nwritten R\n");
    start(&mut patcher, plan, b"R", &[5]);
    let (_, at) = written(&mut patcher);
    let (old, build) = patcher.parts();
    let child = old.field(at, SECOND).unwrap();
    build.open().unwrap();
    let error = build.carry(old, child).expect_err("carried child reaches a site");
    assert!(matches!(error, Halt::Misbuilt(_)), "{error:?}");
}

#[test]
fn identity_reaches_held_fields_and_carries_their_bytes() {
    // codec.md 11.3, 11.4: identity stops at held fields and preserves their opaque content.
    let mut patcher = machine(CAPACITY, &[HELD]);
    let plan = patcher.identity(1).unwrap();
    assert_eq!(patcher.plan_from(1), None);
    start(&mut patcher, plan, b"R", &[0, 0, 0, 4, 255, 0, 1, 254]);
    let step = patcher.next().unwrap();
    assert!(matches!(step, Step::Held { .. }), "{step:?}");
    let Step::Held { at, family, name } = step else { unreachable!("held stop") };
    assert_eq!(patcher.spelled(family), b"other");
    assert_eq!(patcher.spelled(name), b"Thing");
    let (old, build) = patcher.parts();
    build.carry(old, at).unwrap();
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[0, 0, 0, 4, 255, 0, 1, 254]);
}

#[test]
fn held_stop_accepts_replacement_content_at_its_bound() {
    // codec.md 11.4: the caller puts patched held bytes with put_content.
    let (mut patcher, plan) = fixture(HELD, HELD, "");
    start(&mut patcher, plan, b"R", &[0, 0, 0, 0]);
    assert!(matches!(patcher.next(), Ok(Step::Held { .. })), "held field stops");
    patcher.parts().1.put_content(&[9, 8, 7, 6]).unwrap();
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[0, 0, 0, 4, 9, 8, 7, 6]);
}

#[test]
fn content_of_cannot_bypass_a_held_stop() {
    // codec.md 11.5: put_content_of never copies a held field; carry is its stop's operation.
    let (mut patcher, plan) = fixture(HELD, HELD, "");
    start(&mut patcher, plan, b"R", &[0, 0, 0, 1, 9]);
    let step = patcher.next().unwrap();
    assert!(matches!(step, Step::Held { .. }), "{step:?}");
    let Step::Held { at, .. } = step else { unreachable!("held stop") };
    let (old, build) = patcher.parts();
    let error = build.put_content_of(old, at).expect_err("held content must not bypass its stop");
    assert!(matches!(error, Halt::Misbuilt(_)), "{error:?}");
}

#[test]
fn next_pass_uses_each_plans_own_declaration_indexes() {
    // codec.md 11.6: counterpart/source and next_pass cross each version's index space.
    let mut patcher = machine(
        CAPACITY,
        &[
            "versioned record R { x: u8 }",
            "record Added {} versioned record S { x: u16 }",
            "versioned record T { x: u32 }",
        ],
    );
    let first = patcher.plan(text("rename R S\nadd Added\nwiden S.x u8 u16\n").as_bytes()).unwrap().unwrap();
    let second = patcher
        .plan(format!("{HEADER}family qa 2 to 3\nrename S T\ndrop Added\nwiden T.x u16 u32\n").as_bytes())
        .unwrap()
        .unwrap();
    assert_eq!(patcher.counterpart(first, ROOT), Some(DeclIx::new(1)));
    assert_eq!(patcher.source(second, ROOT), Some(DeclIx::new(1)));
    assert_eq!(patcher.plan_from(2), Some(second));
    start(&mut patcher, first, b"R", &[0, 1, 255]);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, first, b"S", &[0, 2, 0, 255]);
    patcher.next_pass(second, DeclIx::new(1)).unwrap();
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, second, b"T", &[0, 3, 0, 0, 0, 255]);
}

#[test]
fn exact_output_byte_capacity_holds() {
    // codec.md 11.7: the output fits at bytes, including a newly added version.
    const OUTPUT_BYTES: u32 = 3; // u16 version + u8 x.
    let capacity = Capacity { bytes: OUTPUT_BYTES, ..CAPACITY };
    let mut patcher = machine(capacity, &[BYTE, "versioned record R { x: u8 }"]);
    let plan = patcher.plan(text("versioned R\n").as_bytes()).unwrap().unwrap();
    start(&mut patcher, plan, b"R", &[255]);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[0, 2, 255]);
}

#[test]
fn output_one_byte_past_capacity_halts_with_room() {
    // codec.md 11.7: output room is checked independently of the smaller old record.
    const OUTPUT_ROOM: u32 = 2; // The output needs u16 version + u8 x = 3.
    let mut patcher = machine(Capacity { bytes: OUTPUT_ROOM, ..CAPACITY }, &[BYTE, "versioned record R { x: u8 }"]);
    let plan = patcher.plan(text("versioned R\n").as_bytes()).unwrap().unwrap();
    start(&mut patcher, plan, b"R", &[255]);
    assert_eq!(patcher.next(), Err(Halt::Room));
}

#[test]
fn zero_width_list_items_count_toward_old_node_capacity() {
    // codec.md 11.7, 8.2: every node is counted, even a zero-byte record in a list.
    const EXACT_NODES: u32 = 5; // R, its list, and three empty E records.
    let body = "record E {} record R { x: list 3 E }";
    let mut patcher = machine(Capacity { nodes: EXACT_NODES, ..CAPACITY }, &[body]);
    let plan = patcher.identity(1).unwrap();
    start(&mut patcher, plan, b"R", &[0, 0, 0, 3]);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[0, 0, 0, 3]);
}

#[test]
fn zero_width_list_one_node_past_room_is_refused() {
    // codec.md 11.7: old nodes one past capacity are Decode(Room).
    const SHORT_NODES: u32 = 4; // R + list + only two of three empty E records.
    let body = "record E {} record R { x: list 3 E }";
    let mut patcher = machine(Capacity { nodes: SHORT_NODES, ..CAPACITY }, &[body]);
    let plan = patcher.identity(1).unwrap();
    let root = patcher.schema(1).unwrap().find(b"R").unwrap();
    assert_eq!(patcher.start(plan, root, Box::from([0, 0, 0, 3])), Err(Halt::Decode(Refusal::Room)));
}

#[test]
fn existing_story_goldens_pass_through_written_dispatch() {
    // codec.md 11.8: old goldens, written functions and generic new decoder agree byte for byte.
    let mut patcher = Patcher::new(CAPACITY);
    patcher.keep(include_bytes!("../schema/every-v1.schema")).unwrap();
    patcher.keep(include_bytes!("../schema/every-v2.schema")).unwrap();
    let plan = patcher.plan(patches::P0001.as_bytes()).unwrap().unwrap();
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut goldens = std::fs::read_dir(base.join("patches/golden/0001"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    goldens.sort();
    let mut context = patches::Context::default();
    for path in goldens {
        let file = path.file_name().unwrap().to_str().unwrap();
        let root = generic::golden_root(patcher.schema(1).unwrap(), file);
        let new_root = patcher.counterpart(plan, root).unwrap();
        let new_name =
            patcher.schema(2).unwrap().spelled(patcher.schema(2).unwrap().declaration(new_root).name).to_vec();
        let old = std::fs::read(base.join("golden/v1").join(file)).unwrap();
        patcher.start(plan, root, old.into_boxed_slice()).unwrap();
        while let step @ (Step::Written { .. } | Step::Held { .. }) = patcher.next().unwrap() {
            match step {
                Step::Written { site, at } => {
                    let (old, build) = patcher.parts();
                    patches::written(site, old, build, at, &mut context).unwrap();
                }
                Step::Held { at, .. } => {
                    let (old, build) = patcher.parts();
                    build.carry(old, at).unwrap();
                }
                Step::Done => unreachable!("while selects stops"),
            }
        }
        output(&mut patcher, plan, &new_name, &std::fs::read(path).unwrap());
    }
}

#[test]
fn seeded_identity_preserves_all_generated_families() {
    // codec.md 11.3, 11.8: identity preserves complete wire values and still visits held fields.
    const DRAWS: usize = 8; // Eight deterministic edge-biased draws per generated root.
    let mut rng = Rng::new(0x5141_5041_5443_4801);
    for family in support::generated::families() {
        let schema = skein_codegen::parse(family.text).expect(family.name);
        let mut patcher = Patcher::new(CAPACITY);
        let version = patcher.keep(family.text.as_bytes()).unwrap();
        let plan = patcher.identity(version).unwrap();
        for root in family.roots {
            for _ in 0..DRAWS {
                let bytes = support::generated::draw(&schema, root.name, &mut rng);
                start(&mut patcher, plan, root.name.as_bytes(), &bytes);
                while let Step::Held { at, .. } = patcher.next().unwrap() {
                    let (old, build) = patcher.parts();
                    build.carry(old, at).unwrap();
                }
                output(&mut patcher, plan, root.name.as_bytes(), &bytes);
                let decoded = (root.decode)(&bytes);
                match decoded {
                    support::generated::Decoded::Value { debug, bytes: encoded } => {
                        assert_eq!(encoded, bytes, "{} {}: {debug}", family.name, root.name);
                    }
                    support::generated::Decoded::Problem { path, reason } => {
                        unreachable!("{} {}: {path} {reason:?}", family.name, root.name);
                    }
                }
            }
        }
    }
}

#[test]
fn carry_of_a_part_with_a_site_in_reach_names_the_carried_part() {
    // codec.md 11.5: a carried part whose type has a site in reach halts at its own path.
    let (mut patcher, plan) = fixture(
        "record E { x: u8 } record R { e: E }",
        "record E { x: bool } record R { e: E }",
        "retype E.x u8 bool\nwritten E.x\nwritten R\n",
    );
    start(&mut patcher, plan, b"R", &[5]);
    let (_, at) = written(&mut patcher);
    let (old, build) = patcher.parts();
    let child = old.field(at, SECOND).unwrap();
    build.open().unwrap();
    assert_eq!(
        build.carry(old, child),
        Err(Halt::Misbuilt(skein_codec::value::Path {
            decl: DeclIx::new(1),
            part: skein_codec::value::Part::Field(SECOND)
        }))
    );
}

#[test]
fn exact_new_node_capacity_holds() {
    // codec.md 11.7: new nodes fit exactly at capacity, independently of old nodes.
    const NEW_NODES: u32 = 2; // R and added x.
    let mut patcher = machine(Capacity { nodes: NEW_NODES, ..CAPACITY }, &[EMPTY, BYTE]);
    let plan = patcher.plan(text("add R.x u8 255\n").as_bytes()).unwrap().unwrap();
    start(&mut patcher, plan, b"R", &[]);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[255]);
}

#[test]
fn added_field_one_new_node_past_capacity_halts() {
    // codec.md 11.7: old or new nodes past capacity halt with Room.
    const OLD_NODES: u32 = 1; // Empty R fits, but the new R and x need two nodes.
    let mut patcher = machine(Capacity { nodes: OLD_NODES, ..CAPACITY }, &[EMPTY, BYTE]);
    let plan = patcher.plan(text("add R.x u8 255\n").as_bytes()).unwrap().unwrap();
    start(&mut patcher, plan, b"R", &[]);
    assert_eq!(patcher.next(), Err(Halt::Room));
}

#[test]
fn written_list_builder_patches_count_and_versions() {
    // codec.md 11.5: lists get their count at close; opening versioned records puts the version.
    let body = "versioned record E { x: u8 } record R { items: list 2 E }";
    let (mut patcher, plan) = fixture(body, body, "written R.items\n");
    start(&mut patcher, plan, b"R", &[0, 0, 0, 0]);
    written(&mut patcher);
    let (_, build) = patcher.parts();
    build.open().unwrap();
    for value in [8, 9] {
        build.open().unwrap();
        build.put_uint(value).unwrap();
        build.close().unwrap();
    }
    build.close().unwrap();
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[0, 0, 0, 2, 0, 2, 8, 0, 2, 9]);
}

#[test]
fn written_list_builder_refuses_an_item_past_bound() {
    // codec.md 11.5: builder counts are checked as the new decoder checks them.
    let body = "record R { items: list 1 u8 }";
    let (mut patcher, plan) = fixture(body, body, "written R.items\n");
    start(&mut patcher, plan, b"R", &[0, 0, 0, 0]);
    written(&mut patcher);
    let (_, build) = patcher.parts();
    build.open().unwrap();
    build.put_uint(8).unwrap();
    let error = build.put_uint(9).and_then(|()| build.close()).expect_err("two items exceed list 1");
    assert!(matches!(error, Halt::Built(_) | Halt::Misbuilt(_)), "{error:?}");
}

#[test]
fn written_content_is_checked_by_the_new_text_type() {
    // codec.md 11.5: put_content_of validates UTF-8 and the new bound.
    for bytes in [b"abc".as_slice(), &[0xff]] {
        let (mut patcher, plan) =
            fixture("record R { x: bytes 3 }", "record R { x: text 2 }", "retype R.x bytes 3 text 2\nwritten R.x\n");
        let mut wire = u32::try_from(bytes.len()).unwrap().to_be_bytes().to_vec();
        wire.extend_from_slice(bytes);
        start(&mut patcher, plan, b"R", &wire);
        let (_, at) = written(&mut patcher);
        let (old, build) = patcher.parts();
        let field = old.field(at, FIRST).unwrap();
        let error = build.put_content_of(old, field).expect_err("new text rejects content");
        assert!(matches!(error, Halt::Built(_)), "{error:?}");
    }
}

#[test]
fn dropped_fields_do_not_run_their_reachable_sites() {
    // codec.md 11.4: the new fields drive the walk; old dropped fields are not visited.
    let (mut patcher, plan) = fixture(
        "record E { x: u8 } record R { gone: E keep: u8 }",
        "record E { x: u8 } record R { keep: u8 }",
        "written E.x\ndrop R.gone\n",
    );
    start(&mut patcher, plan, b"R", &[5, 6]);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[6]);
}

#[test]
fn unversioning_removes_nested_versions_before_reordered_fields() {
    // codec.md 11.4: nested version removal and new field order compose.
    let (mut patcher, plan) = fixture(
        "versioned record E { x: u8 } record R { a: E b: E }",
        "record E { x: u8 } record R { b: E a: E }",
        "unversioned E\norder R b a\n",
    );
    start(&mut patcher, plan, b"R", &[0, 1, 8, 0, 1, 9]);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[9, 8]);
}

#[test]
fn held_annotation_added_alone_does_not_stop_the_old_bytes() {
    // codec.md 11.4: Held requires the same held family and type in both texts.
    let (mut patcher, plan) = fixture("record R { x: bytes 4 }", HELD, "");
    start(&mut patcher, plan, b"R", &[0, 0, 0, 1, 9]);
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[0, 0, 0, 1, 9]);
}

#[test]
fn invalid_old_record_is_refused_before_any_written_site() {
    // codec.md 11.4, 8.2: start validates the entire old record before the walk.
    let body = "record R { x: bool }";
    let (mut patcher, plan) = fixture(body, body, "written R.x\n");
    let error = patcher.start(plan, ROOT, Box::from([2])).expect_err("invalid old bool");
    assert_eq!(
        error,
        Halt::Decode(Refusal::Problem(skein_codec::value::Problem {
            path: skein_codec::value::Path { decl: ROOT, part: skein_codec::value::Part::Field(FIRST) },
            reason: skein_codec::Reason::Bool,
        }))
    );
    start(&mut patcher, plan, b"R", &[1]);
    written(&mut patcher);
    patcher.parts().1.put_bool(false).unwrap();
    assert_eq!(patcher.next(), Ok(Step::Done));
    output(&mut patcher, plan, b"R", &[0]);
}
