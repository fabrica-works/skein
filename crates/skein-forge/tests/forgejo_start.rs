#![cfg(test)]
use skein_forge::forgejo::{client, server};
pub mod common;
use common::cases::start::{cases, expected};
use common::forgejo;
#[test]
fn start_transcripts_decode_to_their_documents() {
    for (case, request, answer) in cases() {
        let transcript = forgejo::transcript(case);
        assert_eq!(transcript.status, 200, "{case}");
        let (document, rows) = expected(answer);
        let decoded = forgejo::decode(&request, &transcript.body);
        assert_eq!(decoded.document, Ok(document), "{case}");
        assert_eq!(decoded.rows, rows, "{case}");
    }
}
#[test]
fn start_requests_encode_as_the_tool_sent_them() {
    for (case, request, _) in cases() {
        let transcript = forgejo::transcript(case);
        let encoded = client::encode(&request, &forgejo::limits()).expect("encoded request");
        assert_eq!(encoded.method, transcript.method, "{case}");
        assert_eq!(&*encoded.target, transcript.target, "{case}");
        assert_eq!(encoded.body.as_deref().unwrap_or(b""), transcript.request_body, "{case}");
    }
}
#[test]
fn start_requests_route_on_the_servers_side_to_the_same_request() {
    for (case, request, _) in cases() {
        assert_eq!(forgejo::route(&forgejo::transcript(case)), request, "{case}");
    }
}
#[test]
fn start_answers_the_server_encodes_decode_unchanged_with_the_transcripts_shape() {
    for (case, request, answer) in cases() {
        let transcript = forgejo::transcript(case);
        let bytes = server::encode(&answer, &forgejo::limits()).expect("server answer");
        assert!(forgejo::shape(&bytes).is_subset(&forgejo::shape(&transcript.body)), "{case}");
        let (document, rows) = expected(answer);
        let decoded = forgejo::decode(&request, &bytes);
        assert_eq!(decoded.document, Ok(document), "{case}");
        assert_eq!(decoded.rows, rows, "{case}");
    }
}
