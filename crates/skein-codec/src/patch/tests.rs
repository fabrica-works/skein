//! Public planning admission and lookup contracts (codec.md, 11.6).

use super::*;

// Two adjacent kept versions, one patch and one identity, with small text fixtures.
const CAPACITY: Capacity = Capacity { versions: 3, patches: 2, text: 4096, bytes: 64, nodes: 16 };
const OLD: &[u8] = b"family f 1\nrecord A {} record Gone {}";
const NEW: &[u8] = b"family f 2\nrecord B {} record Added {}";
// Ten bytes in the schema header, seventeen in the patch header.
const SCHEMA_TEXT: u32 = 10;
const HEADER: &[u8] = b"patch 1\nowner o 0";
const HEADER_TEXT: u32 = 17;
const ONE_VERSION: u32 = 1;
const ONE_PLAN: u32 = 1;
const PATCH: &[u8] = b"patch 1\nowner owner 7\nfamily f 1 to 2\nrename A B\nadd Added\ndrop Gone\n";

fn kept(capacity: Capacity) -> Patcher {
    let mut patcher = Patcher::new(capacity);
    patcher.keep(OLD).expect("old schema");
    patcher.keep(NEW).expect("new schema");
    patcher
}

#[test]
fn lookups_use_each_schemas_own_declaration_indices() {
    let mut patcher = kept(CAPACITY);
    let plan = patcher.plan(PATCH).expect("valid patch").expect("family block");
    assert_eq!(plan.index(), 0);
    assert_eq!(patcher.versions(plan), Versions { from: 1, to: 2 });
    assert_eq!(patcher.counterpart(plan, DeclIx::new(0)), Some(DeclIx::new(0)));
    assert_eq!(patcher.counterpart(plan, DeclIx::new(1)), None);
    assert_eq!(patcher.source(plan, DeclIx::new(0)), Some(DeclIx::new(0)));
    assert_eq!(patcher.source(plan, DeclIx::new(1)), None);
    assert_eq!(patcher.plan_from(1), Some(plan));
    assert_eq!(patcher.plan_from(2), None);
    assert_eq!(patcher.schema(2).expect("kept").version(), 2);
    assert!(patcher.schema(3).is_none());
}

#[test]
fn keep_refuses_family_duplicate_and_bad_schema() {
    let rows: &[(&str, &[u8], KeepError)] = &[
        ("family", b"family other 2", KeepError::Family),
        ("twice", OLD, KeepError::Twice),
        (
            "schema",
            b"!",
            KeepError::Schema(SchemaError { line: 1, at: Span { start: 0, end: 1 }, wrong: schema::Wrong::Character }),
        ),
    ];
    for &(name, text, wrong) in rows {
        let mut patcher = Patcher::new(CAPACITY);
        patcher.keep(OLD).expect("first kept");
        assert_eq!(patcher.keep(text), Err(wrong), "{name}");
    }
}

#[test]
fn keep_admits_at_text_and_version_capacities() {
    let text = b"family f 1";
    let mut patcher = Patcher::new(Capacity { versions: ONE_VERSION, text: SCHEMA_TEXT, ..CAPACITY });
    assert_eq!(patcher.keep(text), Ok(1));
}

#[test]
fn keep_refuses_past_text_and_version_capacities() {
    let rows = [
        ("text", Capacity { text: SCHEMA_TEXT.checked_sub(1).expect("nonempty header"), ..CAPACITY }, false),
        ("versions", Capacity { versions: ONE_VERSION, ..CAPACITY }, true),
    ];
    for (name, capacity, keep_first) in rows {
        let mut patcher = Patcher::new(capacity);
        if keep_first {
            patcher.keep(b"family f 1").expect("at capacity");
        }
        assert_eq!(patcher.keep(b"family f 2"), Err(KeepError::Room), "{name}");
    }
}

#[test]
fn plans_and_identities_share_the_plan_capacity() {
    let mut patcher = kept(Capacity { patches: ONE_PLAN, ..CAPACITY });
    patcher.plan(PATCH).expect("at capacity");
    assert_eq!(patcher.identity(2), Err(PatchWrong::Room));
}

#[test]
fn identity_is_separate_from_the_patch_from_a_version() {
    let mut patcher = kept(CAPACITY);
    let identity = patcher.identity(1).expect("identity");
    assert_eq!(patcher.plan_from(1), None);
    let plan = patcher.plan(PATCH).expect("patch beside identity").expect("own block");
    assert_eq!(patcher.plan_from(1), Some(plan));
    assert_eq!(patcher.versions(identity), Versions { from: 1, to: 1 });
    assert_eq!(patcher.identity(1), Err(PatchWrong::Twice));
    assert_eq!(patcher.identity(3), Err(PatchWrong::Unkept));
}

#[test]
fn second_patch_from_one_version_is_refused() {
    let mut patcher = kept(CAPACITY);
    patcher.plan(PATCH).expect("first plan");
    assert_eq!(
        patcher.plan(PATCH),
        Err(PatchError { line: 3, at: Span { start: 22, end: 28 }, wrong: PatchWrong::Twice })
    );
}

#[test]
fn a_full_plan_table_refuses_the_family_token() {
    let mut patcher = kept(Capacity { patches: 0, ..CAPACITY });
    assert_eq!(
        patcher.plan(PATCH),
        Err(PatchError { line: 3, at: Span { start: 22, end: 28 }, wrong: PatchWrong::Room })
    );
}

#[test]
fn marks_alone_and_other_families_return_no_plan() {
    let rows: &[(&str, &[u8])] = &[
        ("marks", b"patch 1\nowner owner 7\nmark fresh\n"),
        ("other family", b"patch 1\nowner owner 7\nfamily other 1 to 2\nadd Absent\n"),
    ];
    for &(name, text) in rows {
        assert_eq!(kept(CAPACITY).plan(text), Ok(None), "{name}");
    }
}

#[test]
fn patch_text_is_admitted_at_its_capacity() {
    let mut patcher = Patcher::new(Capacity { text: HEADER_TEXT, ..CAPACITY });
    assert_eq!(patcher.plan(HEADER), Ok(None));
}

#[test]
fn patch_text_is_refused_past_its_capacity() {
    let mut patcher = Patcher::new(Capacity { text: HEADER_TEXT.checked_sub(1).expect("nonempty header"), ..CAPACITY });
    assert_eq!(
        patcher.plan(HEADER),
        Err(PatchError { line: 1, at: Span { start: 0, end: 0 }, wrong: PatchWrong::Long })
    );
}

#[test]
fn site_and_refusal_handles_keep_their_numbers() {
    let site = Site::new(7, 3);
    assert_eq!(site.patch(), 7);
    assert_eq!(site.index(), 3);
    assert_eq!(RefusalIx::new(4).index(), 4);
}
