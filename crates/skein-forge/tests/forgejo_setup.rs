#![expect(clippy::disallowed_macros, reason = "the document harness formats fixture values and literal expected rows")]
#![expect(
    clippy::disallowed_methods,
    reason = "the document harness transforms finite fixture rows with ordinary Rust iterators"
)]
#![cfg(test)]
use skein_forge::forgejo::{client, decode::Malformed, document::Document, request::Method, server};
pub mod common;
use common::cases::setup::{OrgAnswer, answers, expected, hook_answers, org_answers, request};
use common::forgejo;
#[test]
fn setup_requests_encode_as_the_tool_sent_them() {
    let limits = forgejo::profile_limits();
    for (case, request) in forgejo::setup_cases() {
        let transcript = forgejo::transcript(case);
        let encoded = client::encode(&request, &limits).expect("setup encoding");
        assert_eq!(encoded.method, transcript.method, "{case}");
        assert_eq!(&*encoded.target, transcript.target, "{case}");
        assert_eq!(encoded.body.as_deref().unwrap_or_default(), transcript.request_body, "{case}");
    }
}
#[test]
fn setup_requests_route_on_the_servers_side_to_the_same_request() {
    let limits = forgejo::profile_limits();
    for (case, request) in forgejo::setup_cases() {
        let t = forgejo::transcript(case);
        assert_eq!(
            forgejo::decode_route(t.method, &t.target, &t.request_body, &limits).expect("setup request").0,
            request,
            "{case}"
        );
    }
}
#[test]
fn setup_transcripts_decode_to_their_documents() {
    let limits = forgejo::profile_limits();
    for (case, answer) in answers() {
        let decoder = client::decoder(&request(case), &limits).expect("setup decoder");
        let result = forgejo::decode_answer(decoder, &forgejo::transcript(case).body, &limits);
        let (document, rows) = expected(answer);
        assert_eq!(result.document, Ok(document), "{case}");
        assert_eq!(result.rows, rows, "{case}");
    }
    for reply in org_answers().into_iter().chain(hook_answers()) {
        check_document(&reply, &forgejo::transcript(reply.case).body);
    }
}
#[test]
fn setup_answers_the_server_encodes_decode_unchanged_with_the_transcripts_shape() {
    let limits = forgejo::profile_limits();
    for (case, answer) in answers() {
        let encoded = server::encode(&answer, &limits).expect("setup encoding");
        let decoder = client::decoder(&request(case), &limits).expect("setup decoder");
        let result = forgejo::decode_answer(decoder, &encoded, &limits);
        let (document, rows) = expected(answer);
        assert_eq!(result.document, Ok(document), "{case}");
        assert_eq!(result.rows, rows, "{case}");
        assert!(forgejo::shape(&encoded).is_subset(&forgejo::shape(&forgejo::transcript(case).body)), "{case}");
    }
    for reply in org_answers().into_iter().chain(hook_answers()) {
        let encoded = server::encode(&reply.answer, &limits).expect("org encoding");
        check_document(&reply, &encoded);
        assert!(
            forgejo::shape(&encoded).is_subset(&forgejo::shape(&forgejo::transcript(reply.case).body)),
            "{}",
            reply.case
        );
    }
}
fn refused_answer(case: &str, status: u16, message: &[u8]) {
    let limits = forgejo::profile_limits();
    let transcript = forgejo::transcript(case);
    assert_eq!(transcript.status, status);
    assert_eq!(
        forgejo::decode_answer(client::error_decoder(&limits), &transcript.body, &limits).document,
        Ok(Document::Error(skein_forge::forgejo::document::ErrorBody {
            message: skein_forge::forgejo::document::Body::Kept(Box::from(message)),
        }))
    );
}
#[test]
fn creating_an_existing_user_reads_its_refusal() {
    refused_answer("setup-user-create-again", 422, b"user already exists [name: bot]");
}
#[test]
fn creating_a_user_as_a_writer_reads_the_admin_refusal() {
    refused_answer("setup-user-create-by-writer", 403, b"user should be the site admin");
}
#[test]
fn basic_auth_for_a_person_who_must_change_password_reads_the_refusal() {
    refused_answer(
        "setup-user-person-basic",
        403,
        b"You must change your password. Change it at: http://127.0.0.1:3000//user/change_password",
    );
}
#[test]
fn a_token_for_a_person_who_must_change_password_reads_the_refusal() {
    refused_answer(
        "setup-user-person-token",
        403,
        b"You must change your password. Change it at: http://127.0.0.1:3000//user/change_password",
    );
}
fn refused(body: &[u8], expected: Malformed) {
    assert_eq!(
        forgejo::decode_refusal(Method::Post, b"/api/v1/admin/users", body, &forgejo::profile_limits()),
        expected
    );
}
#[test]
fn a_text_must_change_password_is_refused_naming_its_field() {
    refused(br#"{"username":"bot","email":"bot@example.invalid","password":"secret","full_name":"kit:bot","must_change_password":"false"}"#,
        Malformed::Shape { field: b"must_change_password" });
}
#[test]
fn a_password_past_string_bytes_is_refused_naming_its_field() {
    let password = "x".repeat(513);
    let body = format!(
        r#"{{"username":"bot","email":"bot@example.invalid","password":"{password}","full_name":"kit:bot","must_change_password":false}}"#
    );
    refused(body.as_bytes(), Malformed::TooLong { field: b"password" });
}
#[test]
fn a_missing_full_name_is_refused_naming_its_field() {
    refused(
        br#"{"username":"bot","email":"bot@example.invalid","password":"secret","must_change_password":false}"#,
        Malformed::Missing { field: b"full_name" },
    );
}
fn refused_token(body: &[u8], expected: Malformed) {
    assert_eq!(
        forgejo::decode_refusal(Method::Post, b"/api/v1/admin/users/bot/tokens", body, &forgejo::profile_limits()),
        expected
    );
}
#[test]
fn a_scope_list_that_is_text_is_refused_naming_scopes() {
    refused_token(br#"{"name":"kit-api","scopes":"read:user"}"#, Malformed::Shape { field: b"scopes" });
}
#[test]
fn a_scope_list_past_the_profiles_inner_is_refused_naming_scopes() {
    let scopes = ["\"read:user\""; 33].join(",");
    let body = format!(r#"{{"name":"kit-api","scopes":[{scopes}]}}"#);
    refused_token(body.as_bytes(), Malformed::Bound { field: b"scopes" });
}

#[test]
fn making_a_token_of_an_existing_name_reads_the_refusal() {
    refused_answer("setup-token-make-again", 400, b"access token name has been used already");
}
#[test]
fn a_git_token_without_read_user_reads_the_scope_refusal() {
    refused_answer("setup-token-git-alone", 403, b"token does not have at least one of required scope(s): [read:user]");
}
#[test]
fn deleting_a_missing_token_reads_the_refusal() {
    refused_answer("setup-token-delete-missing", 404, b"The target couldn't be found.");
}
#[test]
fn an_api_token_without_write_issue_reads_the_scope_refusal() {
    refused_answer(
        "setup-token-narrow-write",
        403,
        b"token does not have at least one of required scope(s): [write:issue]",
    );
}
#[test]
fn an_admin_token_without_write_admin_reads_the_scope_refusal() {
    refused_answer("setup-scope-no-admin", 403, b"token does not have at least one of required scope(s): [read:admin]");
}
#[test]
fn an_admin_token_without_a_user_scope_reads_the_refusal() {
    refused_answer("setup-scope-no-user", 403, b"token does not have at least one of required scope(s): [read:user]");
}
#[test]
fn a_deleted_tokens_refusal_is_read_with_its_masked_value() {
    refused_answer("setup-token-deleted-use", 401, b"token is malformed: token contains an invalid number of segments\naccess token does not exist [sha: 0000000000000000000000000000000000000004]\ntoken is malformed: token contains an invalid number of segments\ntask with token \"0000000000000000000000000000000000000004\": resource does not exist\nauthorized integration: parse JWT error: token is malformed: token contains an invalid number of segments");
}

fn check_document(reply: &OrgAnswer, body: &[u8]) {
    let limits = forgejo::profile_limits();
    let reader = client::decoder(&reply.request, &limits).expect("org answer");
    let decoded = forgejo::decode_answer(reader, body, &limits);
    assert_eq!(decoded.document, Ok(reply.document.clone()), "{}", reply.case);
    assert_eq!(decoded.rows, reply.rows, "{}", reply.case);
}
#[test]
fn patching_a_missing_hook_reads_its_refusal() {
    refused_answer("setup-hook-patch-missing", 404, b"The target couldn't be found.");
}
#[test]
fn deleting_a_missing_hook_reads_its_refusal() {
    refused_answer("setup-hook-delete-missing", 404, b"The target couldn't be found.");
}
