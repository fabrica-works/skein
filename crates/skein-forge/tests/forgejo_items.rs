#![cfg(test)]
#![expect(clippy::needless_raw_string_hashes, reason = "literal malformed fields use consistent raw-byte notation")]
use skein_forge::forgejo::{client, decode::Malformed, server};
pub mod common;
use common::cases::items::{cases, expected};
use common::forgejo;
#[test]
fn item_transcripts_decode_to_their_documents_and_rows() {
    for (case, request, answer) in cases() {
        let transcript = forgejo::transcript(case);
        assert_eq!(transcript.status, 200, "{case}");
        let (document, rows) = expected(answer);
        let result = forgejo::decode(&request, &transcript.body);
        assert_eq!(result.document, Ok(document), "{case}");
        assert_eq!(result.rows, rows, "{case}");
    }
}
#[test]
fn item_requests_encode_as_the_tool_sent_them() {
    for (case, request, _) in cases() {
        let transcript = forgejo::transcript(case);
        let encoded = client::encode(&request, &forgejo::limits()).expect("request");
        assert_eq!(encoded.method, transcript.method, "{case}");
        assert_eq!(encoded.target.as_ref(), transcript.target, "{case}");
        assert_eq!(encoded.body.as_deref().unwrap_or(b""), transcript.request_body, "{case}");
    }
}
#[test]
fn item_requests_route_on_the_servers_side_to_the_same_request() {
    for (case, request, _) in cases() {
        assert_eq!(forgejo::route(&forgejo::transcript(case)), request, "{case}");
    }
}
#[test]
fn item_answers_the_server_encodes_decode_unchanged_with_the_transcripts_shape() {
    for (case, request, answer) in cases() {
        let transcript = forgejo::transcript(case);
        let bytes = server::encode(&answer, &forgejo::limits()).expect("server answer");
        assert!(forgejo::shape(&bytes).is_subset(&forgejo::shape(&transcript.body)), "{case}");
        let (document, rows) = expected(answer);
        let result = forgejo::decode(&request, &bytes);
        assert_eq!(result.document, Ok(document), "{case}");
        assert_eq!(result.rows, rows, "{case}");
    }
}
#[test]
fn item_transcript_mutations_are_malformed_naming_the_field() {
    for (case, field, before, after, expected) in [
        ("issue", "number", br#""number":3,"#.as_slice(), br#""#.as_slice(), Malformed::Missing { field: b"number" }),
        ("issue", "state", br#""state":"open""#.as_slice(), br#""state":"merged""#.as_slice(), Malformed::Value { field: b"state" }),
        ("pull", "head", br#""sha":"0000000000000000000000000000000000000003""#.as_slice(), br#""sha":"000000000000000000000000000000000000000""#.as_slice(), Malformed::Id { field: b"sha" }),
        ("issue", "updated_at", br#""updated_at":"2020-01-01T00:00:05+05:30""#.as_slice(), br#""updated_at":"2020-01-01T00:00:05""#.as_slice(), Malformed::Time { field: b"updated_at" }),
        ("issue", "labels", br#""labels":[]"#.as_slice(), br#""labels":[{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"},{"name":"label"}]"#.as_slice(), Malformed::Bound { field: b"labels" }),
    ] {
        let transcript = forgejo::transcript(case);
        let request = forgejo::route(&transcript);
        let bytes = forgejo::replace(&transcript.body, before, after);
        assert_eq!(forgejo::decode(&request, &bytes).document, Err(expected), "{case}/{field}");
    }
}
