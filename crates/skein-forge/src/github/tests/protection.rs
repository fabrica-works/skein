//! Invented documents exercise the protection plan through its own decoders.
use super::fixtures::{decode, limits};
use crate::{
    api,
    github::{
        document::{self, Document, Kind, Row},
        header::Head,
        plans::{
            Body, Heard,
            plan::{Bounds, Plan, Step},
            tables::{self, Tables},
        },
        request::Request,
    },
};
use alloc::boxed::Box;
const RULES: &[u8] = br#"[
 {"ruleset_id":11,"type":"pull_request","parameters":{"required_approving_review_count":2,"dismiss_stale_reviews_on_push":true,"require_code_owner_review":true}},
 {"ruleset_id":11,"type":"required_status_checks","parameters":{"required_status_checks":[{"context":"build"},{"context":"lint"}]}},
 {"ruleset_id":11,"type":"required_linear_history"},
 {"ruleset_id":11,"type":"required_signatures"},
 {"ruleset_id":11,"type":"merge_queue"},
 {"ruleset_id":999,"type":"creation","parameters":{"unread":{"anything":[1,2]}}}
]"#;
const CLASSIC: &[u8] = br#"{"required_status_checks":{"contexts":["lint","classic"]},"required_pull_request_reviews":{"required_approving_review_count":3,"dismiss_stale_reviews":false,"require_code_owner_reviews":true},"required_signatures":{"enabled":false},"required_linear_history":{"enabled":true}}"#;
fn bounds() -> Bounds {
    Bounds { inner: 4, page_size: 2, rows: 8, answer_bytes: 4096, body_bytes: 1024 }
}
fn request_branch() -> Box<[u8]> {
    bytes(b"main")
}
fn run(
    rules: &[u8],
    classic: Option<&[u8]>,
    bypass: &[u8],
    paged: bool,
    cap: u32,
) -> (Result<api::Answer, api::Error>, u32) {
    run_bypasses(rules, classic, &[(11, bypass), (12, bypass)], paged, cap)
}
fn run_bypasses(
    rules: &[u8],
    classic: Option<&[u8]>,
    bypasses: &[(u64, &[u8])],
    paged: bool,
    cap: u32,
) -> (Result<api::Answer, api::Error>, u32) {
    let mut tables = Tables::new(1, 1);
    let key = api::Repository { forge: 1, repository: 2 };
    tables.keep_repository(
        key,
        tables::Repository {
            owner: bytes(b"acme"),
            name: bytes(b"widgets"),
            owner_id: 1,
            organization: true,
            merge: None,
            html_url: bytes(b"https://example.invalid/acme/widgets"),
        },
    );
    let bounds = Bounds { inner: cap, ..bounds() };
    let mut plan = Plan::new(key, api::Op::Read(api::Read::Protection { branch: request_branch() }));
    let mut step = plan.begin(bounds, b"example-app", &mut tables).unwrap();
    for count in 0_u32..16 {
        let Step::Send(exchange) = step else {
            let Step::Done(result) = step else { unreachable!() };
            return (result, count);
        };
        let (kind, wire, status, next) = if let Request::RulesForBranch { page, .. } = exchange.request {
            (Kind::RuleRows, if paged && page == 1 { b"[]".as_slice() } else { rules }, 200, paged && page == 1)
        } else if let Request::Ruleset { id, .. } = exchange.request {
            assert_ne!(id, 999, "a skipped rule must not cause a ruleset read");
            let wire = bypass_for(bypasses, id);
            (Kind::Ruleset, wire, 200, false)
        } else if let Request::ClassicProtection { .. } = exchange.request {
            match classic {
                Some(wire) => (Kind::ClassicProtection, wire, 200, false),
                None => (Kind::Error, br#"{"message":"Branch not protected"}"#.as_slice(), 404, false),
            }
        } else {
            panic!("protection exchange")
        };
        let mut head = Head::new();
        head.next = next;
        assert_eq!(plan.heard(Heard::Head { status, head }, bounds, b"example-app", &mut tables), Ok(None));
        let (rows, document) = decode(kind, wire, 1, &limits());
        let mut ended = None;
        for row in rows.into_boxed() {
            let verdict = plan.heard(Heard::Row(row), bounds, b"example-app", &mut tables).unwrap();
            if verdict.is_some() {
                ended = verdict;
                break;
            }
        }
        step = match ended {
            Some(step) => step,
            None => plan
                .heard(Heard::Done(Body::Document(document.unwrap())), bounds, b"example-app", &mut tables)
                .unwrap()
                .unwrap(),
        };
    }
    panic!("bounded protection plan")
}
fn protection(answer: Result<api::Answer, api::Error>) -> api::Protection {
    let api::Answer::Protection(Some(value)) = answer.unwrap() else { panic!("protected branch") };
    value
}
#[test]
fn sources_join_contexts_counts_booleans_and_least_bypass() {
    for (wire, bypass) in [
        (br#"{"current_user_can_bypass":"never"}"#.as_slice(), api::Bypass::Never),
        (br#"{"current_user_can_bypass":"pull_requests_only"}"#.as_slice(), api::Bypass::PullsOnly),
        (br#"{"current_user_can_bypass":"always"}"#.as_slice(), api::Bypass::Always),
    ] {
        for paged in [false, true] {
            let (answer, cost) = run(RULES, None, wire, paged, 4);
            let value = protection(answer);
            assert_eq!(value.branch.as_ref(), b"main");
            assert_eq!(value.contexts.as_ref(), &[bytes(b"build"), bytes(b"lint")]);
            assert_eq!(
                (value.approvals, value.dismiss_stale, value.code_owners, value.signatures, value.linear, value.queue),
                (2, true, true, true, true, true)
            );
            assert_eq!(value.bypass, bypass);
            assert_eq!(cost, if paged { 4 } else { 3 });
        }
        let (answer, cost) = run(RULES, Some(CLASSIC), wire, false, 4);
        let value = protection(answer);
        assert_eq!(value.contexts.as_ref(), &[bytes(b"build"), bytes(b"lint"), bytes(b"classic")]);
        assert_eq!(value.approvals, 3);
        assert!(value.dismiss_stale && value.code_owners && value.queue && value.signatures && value.linear);
        assert_eq!(value.bypass, api::Bypass::Never);
        assert_eq!(cost, 3);
    }
    let (answer, cost) = run(b"[]", Some(CLASSIC), b"{}", false, 4);
    let value = protection(answer);
    assert_eq!(value.contexts.as_ref(), &[bytes(b"lint"), bytes(b"classic")]);
    assert_eq!(value.approvals, 3);
    assert_eq!(value.bypass, api::Bypass::Never);
    assert!(!value.queue && !value.signatures && !value.dismiss_stale);
    assert_eq!(cost, 2);
    assert_eq!(run(b"[]", Some(b"{}"), b"{}", false, 4), (Ok(api::Answer::Protection(None)), 2));
    assert_eq!(run(b"[]", None, b"{}", false, 4), (Ok(api::Answer::Protection(None)), 2));
    assert_eq!(
        run(br#"[{"ruleset_id":999,"type":"creation"}]"#, None, b"{}", false, 0),
        (Ok(api::Answer::Protection(None)), 2)
    );
}
#[test]
fn distinct_rulesets_are_bounded_and_bypass_takes_the_least() {
    let rules = br#"[{"ruleset_id":11,"type":"merge_queue"},{"ruleset_id":12,"type":"required_linear_history"}]"#;
    assert_eq!(run(rules, None, br#"{"current_user_can_bypass":"always"}"#, false, 1), (Err(api::Error::TooLarge), 1));
    let (answer, cost) = run(rules, None, br#"{"current_user_can_bypass":"pull_requests_only"}"#, false, 2);
    assert_eq!(protection(answer).bypass, api::Bypass::PullsOnly);
    assert_eq!(cost, 4);
    for bypasses in [
        [
            (11, br#"{"current_user_can_bypass":"always"}"#.as_slice()),
            (12, br#"{"current_user_can_bypass":"never"}"#.as_slice()),
        ],
        [
            (11, br#"{"current_user_can_bypass":"never"}"#.as_slice()),
            (12, br#"{"current_user_can_bypass":"always"}"#.as_slice()),
        ],
    ] {
        let (answer, cost) = run_bypasses(rules, None, &bypasses, false, 2);
        assert_eq!(protection(answer).bypass, api::Bypass::Never);
        assert_eq!(cost, 4);
    }
}
#[test]
fn decoders_refuse_bad_kept_fields_and_bound_contexts() {
    for wire in [
        br#"[{"ruleset_id":0,"type":"merge_queue"}]"#.as_slice(),
        br#"[{"ruleset_id":1,"type":"pull_request","parameters":{}}]"#,
        br#"[{"ruleset_id":1,"type":"required_status_checks","parameters":{}}]"#,
        br#"[{"ruleset_id":1,"type":"pull_request","parameters":{"required_approving_review_count":4294967296}}]"#,
    ] {
        decode(Kind::RuleRows, wire, 1, &limits()).1.unwrap_err();
    }
    for wire in [b"{}".as_slice(), br#"{"current_user_can_bypass":"unknown"}"#] {
        decode(Kind::Ruleset, wire, 1, &limits()).1.unwrap_err();
    }
    for wire in [br#"{"required_status_checks":{}}"#.as_slice(), br#"{"required_signatures":{"enabled":"true"}}"#] {
        decode(Kind::ClassicProtection, wire, 1, &limits()).1.unwrap_err();
    }
    assert_eq!(
        decode(Kind::ClassicProtection, br#"{"required_status_checks":null}"#, 1, &limits()).1,
        Ok(Document::ClassicProtection(None))
    );
    let tight = crate::github::limits::Limits { inner: 1, ..limits() };
    decode(Kind::ClassicProtection, CLASSIC, 1, &tight).1.unwrap_err();
    let (rows, done) = decode(Kind::RuleRows, br#"[{"ruleset_id":1,"type":"unknown"}]"#, 7, &limits());
    assert_eq!(done, Ok(Document::Rows));
    assert_eq!(rows.as_slice(), &[Row::Rule(document::Rule { ruleset: 1, kind: document::RuleKind::Skipped })]);
}

fn bytes(value: &[u8]) -> Box<[u8]> {
    Box::from(value)
}

fn bypass_for<'a>(bypasses: &[(u64, &'a [u8])], id: u64) -> &'a [u8] {
    for (expected, wire) in bypasses {
        if *expected == id {
            return wire;
        }
    }
    panic!("invented ruleset has a bypass document")
}
