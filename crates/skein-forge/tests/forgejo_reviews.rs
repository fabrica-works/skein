#![expect(clippy::disallowed_macros, reason = "the document harness formats fixture values and literal expected rows")]
#![expect(
    clippy::disallowed_methods,
    reason = "the document harness transforms finite fixture rows with ordinary Rust iterators"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the document test harness retains finite fixture rows and mutation buffers in ordinary Rust collections"
)]
#![expect(clippy::needless_raw_string_hashes, reason = "literal wire fixtures use one consistent raw-byte notation")]
#![cfg(test)]
use skein_forge::forgejo::{
    client,
    decode::Malformed,
    document::{Document, Row},
    request::Request,
    server::{self, Answer},
};
pub mod common;
use common::cases::reviews::{cases, expected};
use common::forgejo;
#[test]
fn review_transcripts_decode_to_their_documents_and_rows() {
    for (case, request, answer) in cases() {
        let transcript = forgejo::transcript(case);
        if let Some(answer) = answer {
            assert_eq!(transcript.status, 200, "{case}");
            let (document, rows) = expected(answer);
            let decoded = forgejo::decode(&request, &transcript.body);
            assert_eq!(decoded.document, Ok(document), "{case}");
            assert_eq!(decoded.rows, rows, "{case}");
        } else {
            assert_eq!(transcript.status, 403, "{case}");
        }
    }
}
#[test]
fn review_requests_encode_as_the_tool_sent_them() {
    for (case, request, _) in cases() {
        let transcript = forgejo::transcript(case);
        let encoded = client::encode(&request, &forgejo::limits()).expect("request");
        assert_eq!(encoded.method, transcript.method, "{case}");
        assert_eq!(encoded.target.as_ref(), transcript.target, "{case}");
        assert_eq!(encoded.body.as_deref().unwrap_or(b""), transcript.request_body, "{case}");
    }
}
#[test]
fn review_requests_route_on_the_servers_side_to_the_same_request() {
    for (case, request, _) in cases() {
        assert_eq!(forgejo::route(&forgejo::transcript(case)), request, "{case}");
    }
}
#[test]
fn review_answers_the_server_encodes_decode_unchanged_with_the_transcripts_shape() {
    for (case, request, answer) in cases() {
        let transcript = forgejo::transcript(case);
        if let Some(answer) = answer {
            let bytes = server::encode(&answer, &forgejo::limits()).expect("answer");
            assert!(forgejo::shape(&bytes).is_subset(&forgejo::shape(&transcript.body)), "{case}");
            let (document, rows) = expected(answer);
            let decoded = forgejo::decode(&request, &bytes);
            assert_eq!(decoded.document, Ok(document), "{case}");
            assert_eq!(decoded.rows, rows, "{case}");
        } else {
            assert_eq!(transcript.status, 403, "{case}");
        }
    }
}
#[test]
fn review_transcript_mutations_are_malformed_naming_the_field() {
    for (case, field, before, after, expected) in [
        ("reviews", "state", br#""state":"APPROVED""#.as_slice(), br#""state":"approved""#.as_slice(), Malformed::Value { field: b"state" }),
        ("reviews", "commit_id", br#""commit_id":"0000000000000000000000000000000000000004""#.as_slice(), br#""commit_id":"000000000000000000000000000000000000000""#.as_slice(), Malformed::Id { field: b"commit_id" }),
        ("review-comments", "position", br#""position":1"#.as_slice(), br#""position":4294967296"#.as_slice(), Malformed::Range { field: b"position" }),
        ("branch", "commit", br#""id":"0000000000000000000000000000000000000003","#.as_slice(), br#""#.as_slice(), Malformed::Missing { field: b"id" }),
        ("branch-protection", "status_check_contexts", br#""status_check_contexts":["fixture/test"]"#.as_slice(), br#""status_check_contexts":["context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context","context"]"#.as_slice(), Malformed::Bound { field: b"status_check_contexts" }),
        ("collaborator-permission", "permission", br#""permission":"write""#.as_slice(), br#""permission":"maintainer""#.as_slice(), Malformed::Value { field: b"permission" }),
    ] {
        let transcript = forgejo::transcript(case);
        let bytes = forgejo::replace(&transcript.body, before, after);
        assert_eq!(
            forgejo::decode(&forgejo::route(&transcript), &bytes).document,
            Err(expected),
            "{case}/{field}"
        );
    }
}
#[test]
fn open_pulls_rows_are_the_pull_transcripts_documents() {
    let request = Request::Pulls { at: forgejo::specimen(), page: forgejo::page(1, 2) };
    let (body, pulls) = open_pulls();
    let expected = pulls.iter().cloned().map(Row::Pull).collect::<Vec<_>>();
    let decoded = forgejo::decode(&request, &body);
    assert_eq!(decoded.document, Ok(Document::Listed { skipped: 0 }));
    assert_eq!(decoded.rows, expected);
    let bytes = server::encode(&Answer::Pulls(pulls.into_boxed_slice()), &forgejo::limits()).expect("pull list");
    let decoded = forgejo::decode(&request, &bytes);
    assert_eq!(decoded.document, Ok(Document::Listed { skipped: 0 }));
    assert_eq!(decoded.rows, expected);
    assert!(forgejo::shape(&bytes).is_subset(&forgejo::shape(&body)));
    let encoded = client::encode(&request, &forgejo::limits()).expect("request");
    assert_eq!(encoded.method, skein_forge::forgejo::request::Method::Get);
    assert_eq!(&*encoded.target, b"/api/v1/repos/fixture/specimen/pulls?state=open&sort=oldest&page=1&limit=2");
    let server::Route::Ready(routed) =
        server::route(encoded.method, &encoded.target, &forgejo::limits()).expect("route")
    else {
        panic!("read route has no body")
    };
    assert_eq!(routed, request);
}

fn open_pulls() -> (Vec<u8>, Vec<skein_forge::forgejo::document::PullRequest>) {
    let mut body = vec![b'['];
    let mut pulls = Vec::new();
    for (i, case) in ["pull", "pull-merged"].into_iter().enumerate() {
        let transcript = forgejo::transcript(case);
        if i != 0 {
            body.push(b',');
        }
        body.extend_from_slice(&transcript.body);
        let Document::Pull(pull) =
            forgejo::decode(&forgejo::route(&transcript), &transcript.body).document.expect("pull")
        else {
            panic!("pull document");
        };
        assert_eq!(pull.head.repo_id, 1, "{case} head");
        assert_eq!(pull.base.repo_id, 1, "{case} base");
        pulls.push(pull);
    }
    body.push(b']');
    (body, pulls)
}
