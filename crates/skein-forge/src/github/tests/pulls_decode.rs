use super::fixtures::{self, bytes, decode, limits};
use crate::{
    api,
    github::{
        decode::Malformed,
        document::{self, Document, Kind, Row},
        server,
    },
};
use skein_lib::{Overflow, Writer};

const PULL: &str = r#"{"number":5,"state":"open","head":{"ref":"feature","sha":"0123456789abcdef0123456789abcdef01234567","repo":null},"base":{"ref":"main","sha":"abcdef0123456789abcdef0123456789abcdef01"},"mergeable":null}"#;
fn assert_decode(kind: Kind, body: &[u8], expected: Document, expected_rows: &[Row]) {
    for chunk in [1, 7] {
        let (rows, answer) = decode(kind, body, chunk, &limits());
        assert_eq!(answer, Ok(expected.clone()));
        assert_eq!(rows.as_slice(), expected_rows);
    }
}
#[test]
fn github_pull_document_round_trips() {
    for (closed, merged) in [(false, false), (true, false), (true, true)] {
        let mut pull = fixtures::pull(5);
        pull.closed = closed;
        pull.merged = merged;
        pull.merge_commit = Some(pull.base_commit);
        let mut out = Writer::new(4096);
        server::pull(&pull, &mut out).unwrap();
        assert_decode(Kind::Pull, &bytes(out), Document::Pull(pull.clone()), &[]);
        let mut small = Writer::new(1);
        assert_eq!(server::pull(&pull, &mut small), Err(Overflow));
        assert_eq!(small.written(), 0);
    }
    let mut pull = fixtures::pull(5);
    pull.mergeable = false;
    pull.reviewers = Box::from([]);
    assert_decode(Kind::Pull, PULL.as_bytes(), Document::Pull(pull.clone()), &[]);
    let reordered = PULL.replace(r#""head":{"ref":"feature","sha":"0123456789abcdef0123456789abcdef01234567","repo":null},"base":{"ref":"main","sha":"abcdef0123456789abcdef0123456789abcdef01"}"#,
        r#""base":{"sha":"abcdef0123456789abcdef0123456789abcdef01","ref":"main"},"head":{"sha":"0123456789abcdef0123456789abcdef01234567","ref":"feature"}"#);
    assert_decode(Kind::Pull, reordered.as_bytes(), Document::Pull(pull), &[]);
    let mut at_bound = fixtures::pull(5);
    at_bound.reviewers = Box::from([1, 2, 3, 4]);
    at_bound.head = Box::from([b'x'; 128]);
    at_bound.base = Box::from([b'y'; 128]);
    let mut out = Writer::new(4096);
    server::pull(&at_bound, &mut out).unwrap();
    assert_decode(Kind::Pull, &bytes(out), Document::Pull(at_bound), &[]);
}
#[test]
fn github_pull_document_malformed() {
    for (json, expected) in [
        (PULL.replace(r#""number":5,"#, ""), Malformed::Missing { field: b"number" }),
        (
            PULL.replace(r#""sha":"0123456789abcdef0123456789abcdef01234567","#, ""),
            Malformed::Missing { field: b"sha" },
        ),
        (PULL.replace("open", "draft"), Malformed::Value { field: b"state" }),
        (PULL.replace("0123456789abcdef0123456789abcdef01234567", "abc"), Malformed::Id { field: b"sha" }),
        (PULL.replace(r#""number":5"#, r#""number":-1"#), Malformed::Range { field: b"number" }),
        (PULL.replace(r#""number":5"#, r#""number":18446744073709551616"#), Malformed::Range { field: b"number" }),
        (
            PULL.replace(
                r#""mergeable":null"#,
                r#""requested_reviewers":[{"id":1},{"id":2},{"id":3},{"id":4},{"id":5}]"#,
            ),
            Malformed::Bound { field: b"requested_reviewers" },
        ),
        (PULL.replace("feature", &"x".repeat(129)), Malformed::TooLong { field: b"ref" }),
        (PULL.replace(r#""mergeable":null"#, r#""head":{}"#), Malformed::Shape { field: b"head" }),
    ] {
        for chunk in [1, 7] {
            assert_eq!(decode(Kind::Pull, json.as_bytes(), chunk, &limits()).1, Err(expected), "{json}");
        }
    }
}
#[test]
fn github_pull_rows_round_trip() {
    let first = fixtures::pull(5);
    let mut second = fixtures::pull(6);
    second.closed = true;
    second.reviewers = Box::from([]);
    let mut out = Writer::new(4096);
    server::pulls(&[first.clone(), second.clone()], &mut out).unwrap();
    assert_decode(Kind::PullRows, &bytes(out), Document::Rows, &[Row::Pull(first), Row::Pull(second)]);
    assert_decode(Kind::PullRows, b"[]", Document::Rows, &[]);
    let mut small = Writer::new(1);
    assert_eq!(server::pulls(&[fixtures::pull(5)], &mut small), Err(Overflow));
    assert_eq!(small.written(), 0);
}
#[test]
fn github_statuses_document_round_trips() {
    let contexts = [
        fixtures::context(api::Check::Pending),
        fixtures::context(api::Check::Passed),
        fixtures::context(api::Check::Failed),
    ];
    let combined = document::Combined { check: api::Check::Failed, total: 3 };
    let mut out = Writer::new(4096);
    server::combined(&combined, &contexts, &mut out).unwrap();
    assert_decode(Kind::CombinedStatus, &bytes(out), Document::Combined(combined), &contexts.map(Row::Context));
    let context = document::Context {
        updated: None,
        context: Box::from(*b"build"),
        check: api::Check::Failed,
        description: Box::from([]),
        url: Box::from([]),
    };
    assert_decode(Kind::CombinedStatus, br#"{"statuses":[{"context":"build","state":"error","description":null,"target_url":null}],"total_count":1,"state":"error"}"#,
        Document::Combined(document::Combined { check: api::Check::Failed, total: 1 }), &[Row::Context(context)]);
    assert_decode(
        Kind::CombinedStatus,
        br#"{"state":"pending","total_count":0}"#,
        Document::Combined(document::Combined { check: api::Check::Pending, total: 0 }),
        &[],
    );
    for (json, expected) in [
        (br#"{"state":"other","total_count":1}"#.as_slice(), Malformed::Value { field: b"state" }),
        (br#"{"state":"success"}"#, Malformed::Missing { field: b"total_count" }),
        (br#"{"state":"success","total_count":1,"statuses":[{"context":"build","state":"success","updated_at":"bad"}]}"#, Malformed::Time { field: b"updated_at" }),
    ] {
        for chunk in [1, 7] { assert_eq!(decode(Kind::CombinedStatus, json, chunk, &limits()).1, Err(expected)); }
    }
    let at_bound = document::Context {
        updated: None,
        context: Box::from([b'x'; 128]),
        check: api::Check::Passed,
        description: Box::from([b'y'; 128]),
        url: Box::from([b'z'; 128]),
    };
    let combined = document::Combined { check: api::Check::Passed, total: 1 };
    let mut out = Writer::new(4096);
    server::combined(&combined, core::slice::from_ref(&at_bound), &mut out).unwrap();
    assert_decode(Kind::CombinedStatus, &bytes(out), Document::Combined(combined), &[Row::Context(at_bound)]);
    for field in ["context", "description", "target_url"] {
        let json = alloc::fmt::format(format_args!(
            r#"{{"state":"success","total_count":1,"statuses":[{{"state":"success","context":"build","{field}":"{}"}}]}}"#,
            "x".repeat(129)
        ));
        let json = if field == "context" { json.replace(r#""context":"build","#, "") } else { json };
        let key: &'static [u8] = match field {
            "context" => b"context",
            "description" => b"description",
            "target_url" => b"target_url",
            _ => unreachable!(),
        };
        for chunk in [1, 7] {
            assert_eq!(
                decode(Kind::CombinedStatus, json.as_bytes(), chunk, &limits()).1,
                Err(Malformed::TooLong { field: key })
            );
        }
    }
    let mut small = Writer::new(1);
    assert_eq!(
        server::combined(&document::Combined { check: api::Check::Pending, total: 0 }, &[], &mut small),
        Err(Overflow)
    );
    assert_eq!(small.written(), 0);
}
#[test]
fn github_checks_runs_round_trip() {
    let runs = [
        document::Run::new(api::Check::Pending),
        document::Run::new(api::Check::Passed),
        document::Run::new(api::Check::Failed),
    ];
    let mut out = Writer::new(4096);
    server::runs(&runs, &mut out).unwrap();
    assert_decode(Kind::CheckRuns, &bytes(out), Document::Rows, &runs.map(Row::Run));
    for (status, conclusion, check) in [
        ("completed", "success", api::Check::Passed),
        ("completed", "neutral", api::Check::Passed),
        ("completed", "skipped", api::Check::Passed),
        ("completed", "failure", api::Check::Failed),
        ("completed", "timed_out", api::Check::Failed),
        ("completed", "cancelled", api::Check::Failed),
        ("completed", "action_required", api::Check::Failed),
        ("completed", "startup_failure", api::Check::Failed),
        ("completed", "stale", api::Check::Failed),
        ("queued", "other", api::Check::Pending),
        ("in_progress", "success", api::Check::Pending),
        ("waiting", "failure", api::Check::Pending),
        ("requested", "success", api::Check::Pending),
        ("pending", "success", api::Check::Pending),
    ] {
        for reverse in [false, true] {
            let fields = if reverse {
                alloc::fmt::format(format_args!(r#""conclusion":"{conclusion}","status":"{status}""#))
            } else {
                alloc::fmt::format(format_args!(r#""status":"{status}","conclusion":"{conclusion}""#))
            };
            let json = alloc::fmt::format(format_args!(r#"{{"check_runs":[{{{fields}}}]}}"#));
            assert_decode(Kind::CheckRuns, json.as_bytes(), Document::Rows, &[Row::Run(document::Run::new(check))]);
        }
    }
    for (json, expected) in [
        (br#"{"check_runs":[{"status":"completed"}]}"#.as_slice(), Malformed::Value { field: b"conclusion" }),
        (br#"{"check_runs":[{"status":"completed","conclusion":null}]}"#, Malformed::Value { field: b"conclusion" }),
        (br#"{"check_runs":[{"conclusion":"other","status":"completed"}]}"#, Malformed::Value { field: b"conclusion" }),
        (br#"{"check_runs":[{"status":"other"}]}"#, Malformed::Value { field: b"status" }),
        (b"{}", Malformed::Missing { field: b"check_runs" }),
    ] {
        for chunk in [1, 7] {
            assert_eq!(decode(Kind::CheckRuns, json, chunk, &limits()).1, Err(expected));
        }
    }
    assert_decode(
        Kind::CheckRuns,
        br#"{"check_runs":[{"status":"queued","conclusion":null}]}"#,
        Document::Rows,
        &[Row::Run(document::Run::new(api::Check::Pending))],
    );
    let mut small = Writer::new(1);
    assert_eq!(server::runs(&[], &mut small), Err(Overflow));
    assert_eq!(small.written(), 0);
}
