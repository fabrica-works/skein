#![expect(clippy::disallowed_macros, reason = "the document harness formats fixture values and literal expected rows")]
#![expect(
    clippy::disallowed_methods,
    reason = "the document harness transforms finite fixture rows with ordinary Rust iterators"
)]
#![cfg(test)]
use skein_forge::forgejo::{
    client,
    document::{Body, Document, ErrorBody},
    failure,
    server::{self, Answer},
};
pub mod common;
use common::cases::writes::{answers, expected, failure_cases};
use common::forgejo;
#[test]
fn write_requests_encode_as_the_tool_sent_them() {
    for (case, request) in forgejo::write_cases() {
        let transcript = forgejo::transcript(case);
        let encoded = client::encode(&request, &forgejo::limits()).expect("write encoding");
        assert_eq!(encoded.method, transcript.method, "{case}");
        assert_eq!(&*encoded.target, transcript.target, "{case}");
        assert_eq!(encoded.body.as_deref().unwrap_or_default(), transcript.request_body, "{case}");
    }
}
#[test]
fn write_requests_route_on_the_servers_side_to_the_same_request() {
    for (case, request) in forgejo::write_cases() {
        assert_eq!(forgejo::route(&forgejo::transcript(case)), request, "{case}");
    }
}
#[test]
fn write_transcripts_decode_to_their_documents_and_rows() {
    let requests = forgejo::write_cases();
    for (case, answer) in answers() {
        let request = &requests.iter().find(|(name, _)| *name == case).expect("request case").1;
        let transcript = forgejo::transcript(case);
        let decoded = forgejo::decode(request, &transcript.body);
        assert_eq!(decoded.document, Ok(expected(answer)), "{case}");
        assert!(decoded.rows.is_empty(), "{case}: single document");
    }
}
#[test]
fn write_answers_the_server_encodes_decode_unchanged_with_the_transcripts_shape() {
    let requests = forgejo::write_cases();
    for (case, answer) in answers() {
        let request = &requests.iter().find(|(name, _)| *name == case).expect("request case").1;
        let encoded = server::encode(&answer, &forgejo::limits()).expect("answer encoding");
        let shape = forgejo::shape(&forgejo::transcript(case).body);
        for field in forgejo::shape(&encoded) {
            assert!(shape.contains(&field), "{case}: {field:?}");
        }
        let decoded = forgejo::decode(request, &encoded);
        assert_eq!(decoded.document, Ok(expected(answer)), "{case}");
        assert!(decoded.rows.is_empty(), "{case}");
    }
}
#[test]
fn write_transcript_mutations_are_malformed_naming_the_field() {
    let request =
        forgejo::write_cases().into_iter().find(|(case, _)| *case == "post-comment").expect("fixture value").1;
    let body = forgejo::transcript("post-comment").body;
    let bytes = forgejo::replace(&body, br#""id":4,"#, b"");
    let decoded = forgejo::decode(&request, &bytes);
    assert_eq!(decoded.document, Err(skein_forge::forgejo::decode::Malformed::Missing { field: b"id" }));
}
#[test]
fn write_bodies_enforce_fields_types_and_bounds() {
    let mut limits = forgejo::limits();
    limits.inner = 1;
    limits.string_bytes = 8;
    limits.body_bytes = 16;
    let rows: [(&[u8], &[u8], &str); 7] = [
        (b"issues", br#"{"body":"a"}"#, "title"),
        (b"issues", br#"{"title":"a","title":"b","body":"a"}"#, "title"),
        (b"issues", br#"{"title":2,"body":"a"}"#, "title"),
        (b"issues", br#"{"title":"123456789","body":"a"}"#, "title"),
        (b"issues", br#"{"title":"a","body":"12345678901234567"}"#, "body"),
        (b"pulls/1/requested_reviewers", br#"{"reviewers":["a","b"]}"#, "reviewers"),
        (b"pulls/1/requested_reviewers", br#"{"reviewers":["123456789"]}"#, "reviewers"),
    ];
    for (tail, body, field) in rows {
        let target = [b"/api/v1/repos/a/b/".as_slice(), tail].concat();
        let error = forgejo::decode_route(skein_forge::forgejo::request::Method::Post, &target, body, &limits)
            .expect_err("malformed request");
        assert!(error.contains(&format!("{:?}", field.as_bytes())), "{field}: {error}");
    }
}
#[test]
fn failure_transcripts_name_their_refusal_by_status_and_message() {
    for (case, status, message, expected) in failure_cases() {
        let transcript = forgejo::transcript(case);
        assert_eq!(transcript.status, status, "{case}");
        let decoded = forgejo::decode_error(&transcript.body);
        if case == "moved-repository" {
            assert!(decoded.document.is_err(), "{case}: no JSON message");
        } else {
            assert_eq!(
                decoded.document,
                Ok(Document::Error(ErrorBody { message: Body::Kept(Box::from(message)) })),
                "{case}"
            );
        }
        assert!(decoded.rows.is_empty(), "{case}");
        assert_eq!(failure::refusal(status, message), expected, "{case}");
    }
}
#[test]
fn error_answers_meet_the_client_with_the_transcripts_shape() {
    for (case, _, message, _) in failure_cases() {
        if case == "wrong-token" || case == "moved-repository" {
            continue;
        }
        let expected = ErrorBody { message: Body::Kept(Box::from(message)) };
        let encoded = server::encode(&Answer::Error(expected.clone()), &forgejo::limits()).expect("error encoding");
        let shape = forgejo::shape(&forgejo::transcript(case).body);
        for field in forgejo::shape(&encoded) {
            assert!(shape.contains(&field), "{case}: {field:?}");
        }
        assert_eq!(forgejo::decode_error(&encoded).document, Ok(Document::Error(expected)), "{case}");
    }
}
