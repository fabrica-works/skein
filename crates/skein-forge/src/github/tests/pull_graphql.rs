use super::fixtures::{self, bytes, decode, limits};
use crate::{
    api,
    github::{
        decode::{self, Malformed},
        document::{self, Document, Kind},
        request::{self, Accept, Auth, Method, Refusal, Request},
        server,
    },
};
use skein_lib::{Overflow, Writer};

const ANSWER: &str = r#"{"data":{"repository":{"pullRequest":{"id":"opaque+/=node","mergeCommit":null,"mergeQueueEntry":{"position":3},"timelineItems":{"nodes":[{"reason":"checks failed"}]}}}}}"#;
fn row() -> document::PullGraphql {
    document::PullGraphql {
        id: Box::from(*b"opaque+/=node"),
        merge_commit: None,
        queued: Some(3),
        dequeued: Some(Box::from(*b"checks failed")),
    }
}
fn request() -> Request {
    Request::PullGraphql { owner: Box::from(*b"acme"), name: Box::from(*b"widgets"), number: 5 }
}
#[test]
fn github_pull_graphql_request() {
    let mut limits = limits();
    limits.body_bytes = 1024;
    let encoded = request::encode(&request(), &limits).unwrap();
    assert_eq!(encoded.method, Method::Post);
    assert_eq!(encoded.target.as_ref(), b"/graphql");
    assert_eq!(encoded.auth, Auth::Installation);
    assert_eq!(encoded.accept, Accept::Json);
    assert_eq!(decode::kind(&request()), Some(Kind::PullGraphql));
    let document = "query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) { pullRequest(number: $number) { id mergeCommit { oid } mergeQueueEntry { position } timelineItems(last: 1, itemTypes: [REMOVED_FROM_MERGE_QUEUE_EVENT]) { nodes { ... on RemovedFromMergeQueueEvent { reason } } } } } }";
    assert_eq!(document::PULL, document.as_bytes());
    let golden = alloc::fmt::format(format_args!(
        r#"{{"query":"{document}","variables":{{"owner":"acme","name":"widgets","number":5}}}}"#
    ));
    assert_eq!(encoded.body.as_deref(), Some(golden.as_bytes()));
    limits.body_bytes = u32::try_from(golden.len()).unwrap();
    limits.target_bytes = 8;
    assert_eq!(request::encode(&request(), &limits), Ok(encoded));
    limits.body_bytes -= 1;
    assert_eq!(request::encode(&request(), &limits), Err(Refusal::TooLong));
    limits.body_bytes += 1;
    limits.target_bytes = 7;
    assert_eq!(request::encode(&request(), &limits), Err(Refusal::TooLong));
    limits.target_bytes = 1024;
    for number in [0, u64::from(i32::MAX.unsigned_abs()) + 1, u64::MAX] {
        let request = Request::PullGraphql { owner: Box::from(*b"acme"), name: Box::from(*b"widgets"), number };
        assert_eq!(request::encode(&request, &limits), Err(Refusal::Text));
    }
    limits.body_bytes = 1024;
    for length in [128, 129] {
        let request = Request::PullGraphql {
            owner: Box::from([b'a'; 129].get(..length).unwrap()),
            name: Box::from(*b"widgets"),
            number: u64::from(i32::MAX.unsigned_abs()),
        };
        assert_eq!(request::encode(&request, &limits).is_ok(), length == 128);
    }
    let escaped = Request::PullGraphql { owner: Box::from(*b"acme\""), name: Box::from(*b"widgets\\"), number: 5 };
    let encoded = request::encode(&escaped, &limits).unwrap();
    assert!(skein_lib::bytes::find(&encoded.body.unwrap(), br#""owner":"acme\"","name":"widgets\\""#).is_some());
}
#[test]
fn github_pull_graphql_documents() {
    for chunk in [1, 7] {
        assert_eq!(decode(Kind::PullGraphql, ANSWER.as_bytes(), chunk, &limits()).1, Ok(Document::PullGraphql(row())));
        for (queued, merge_commit, dequeued) in [
            (None, None, None),
            (Some(0), Some(fixtures::pull(5).base_commit), Some(Box::from([]))),
            (Some(u32::MAX), None, Some(Box::from(*b"reason"))),
        ] {
            let row = document::PullGraphql { merge_commit, queued, dequeued, ..row() };
            let mut out = Writer::new(4096);
            server::pull_graphql(&row, &mut out).unwrap();
            assert_eq!(decode(Kind::PullGraphql, &bytes(out), chunk, &limits()).1, Ok(Document::PullGraphql(row)));
        }
        let null_reason = ANSWER.replace(r#""checks failed""#, "null");
        let mut expected = row();
        expected.dequeued = None;
        assert_eq!(
            decode(Kind::PullGraphql, null_reason.as_bytes(), chunk, &limits()).1,
            Ok(Document::PullGraphql(expected))
        );
        let unknown = ANSWER.replace(r#""id":"#, r#""extra":{"nested":[false,{}]},"id":"#);
        assert_eq!(decode(Kind::PullGraphql, unknown.as_bytes(), chunk, &limits()).1, Ok(Document::PullGraphql(row())));
    }
    let mut small = Writer::new(1);
    assert_eq!(server::pull_graphql(&row(), &mut small), Err(Overflow));
    assert_eq!(small.written(), 0);
}
#[test]
fn github_pull_graphql_bounds_and_utf8() {
    for chunk in [1, 7] {
        for length in [128, 129] {
            let json = ANSWER.replace("opaque+/=node", &"x".repeat(length));
            let result = decode(Kind::PullGraphql, json.as_bytes(), chunk, &limits()).1;
            if length == 128 {
                let mut row = row();
                row.id = Box::from([b'x'; 129].get(..length).unwrap());
                assert_eq!(result, Ok(Document::PullGraphql(row)));
            } else {
                assert_eq!(result, Err(Malformed::TooLong { field: b"id" }));
            }
        }
        let mut limits = limits();
        limits.json.string = 1024;
        for reason in ["x".repeat(256), "x".repeat(257), alloc::fmt::format(format_args!("{}éend", "x".repeat(255)))] {
            let json = ANSWER.replace("checks failed", &reason);
            let end = if reason.starts_with(&"x".repeat(256)) { 256 } else { 255 };
            let mut row = row();
            row.dequeued = Some(Box::from(reason.as_bytes().get(..end).unwrap()));
            assert_eq!(decode(Kind::PullGraphql, json.as_bytes(), chunk, &limits).1, Ok(Document::PullGraphql(row)));
        }
        limits.inner = 0;
        assert_eq!(
            decode(Kind::PullGraphql, ANSWER.as_bytes(), chunk, &limits).1,
            Err(Malformed::Bound { field: b"nodes" })
        );
        limits.inner = 4;
        let json = ANSWER.replace(r#"{"reason":"checks failed"}"#, r#"{"reason":"first"},{"reason":"last"}"#);
        assert_eq!(
            decode(Kind::PullGraphql, json.as_bytes(), chunk, &limits).1,
            Err(Malformed::Bound { field: b"nodes" })
        );
    }
}
#[test]
fn github_pull_graphql_errors_and_malformed() {
    for chunk in [1, 7] {
        for json in [
            r#"{"data":null,"errors":[{"type":"FORBIDDEN","message":"refused"}]}"#.into(),
            ANSWER.replacen('{', r#"{"errors":[{"type":"FORBIDDEN","message":"refused"}],"#, 1),
            r#"{"data":{"repository":null},"errors":[{"type":"NOT_FOUND","message":"refused"}]}"#
                .replace("NOT_FOUND", "FORBIDDEN"),
        ] {
            assert_eq!(
                decode(Kind::PullGraphql, json.as_bytes(), chunk, &limits()).1,
                Ok(Document::Graphql(document::GraphqlRefusal {
                    kind: Box::from(*b"FORBIDDEN"),
                    message: Box::from(*b"refused")
                }))
            );
        }
        for (json, error) in [
            ("{}".into(), Malformed::Missing { field: b"pullRequest" }),
            (r#"{"data":{"repository":{"pullRequest":null}}}"#.into(), Malformed::Missing { field: b"pullRequest" }),
            (r#"{"errors":[]}"#.into(), Malformed::Missing { field: b"type" }),
            (r#"{"errors":[{"type":"FORBIDDEN"}]}"#.into(), Malformed::Missing { field: b"message" }),
            (ANSWER.replace(r#""id":"opaque+/=node","#, ""), Malformed::Missing { field: b"id" }),
            (ANSWER.replace(r#""mergeCommit":null,"#, ""), Malformed::Missing { field: b"mergeCommit" }),
            (
                ANSWER.replace(r#""mergeQueueEntry":{"position":3},"#, ""),
                Malformed::Missing { field: b"mergeQueueEntry" },
            ),
            (ANSWER.replace(r#""id":"opaque+/=node""#, r#""id":1"#), Malformed::Shape { field: b"id" }),
            (ANSWER.replace(r#""id":"opaque+/=node""#, r#""id":"a","id":"b""#), Malformed::Shape { field: b"id" }),
            (ANSWER.replace(r#""position":3"#, r#""position":4294967296"#), Malformed::Range { field: b"position" }),
            (ANSWER.replace(r#""position":3"#, r#""position":-1"#), Malformed::Range { field: b"position" }),
            (ANSWER.replace(r#""position":3"#, r#""position":1.5"#), Malformed::Range { field: b"position" }),
            (
                ANSWER.replace(r#""mergeCommit":null"#, r#""mergeCommit":{"oid":"short"}"#),
                Malformed::Id { field: b"oid" },
            ),
            (ANSWER.replace(r#""mergeCommit":null"#, r#""mergeCommit":{}"#), Malformed::Missing { field: b"oid" }),
            (
                ANSWER.replace(r#""nodes":[{"reason":"checks failed"}]"#, r#""nodes":[{}]"#),
                Malformed::Missing { field: b"reason" },
            ),
        ] {
            assert_eq!(decode(Kind::PullGraphql, json.as_bytes(), chunk, &limits()).1, Err(error), "{json}");
        }
    }
    assert_eq!(api::DEQUEUED_BYTES, 256);
}
