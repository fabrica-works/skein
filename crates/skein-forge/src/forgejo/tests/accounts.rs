use super::fixtures;
use crate::forgejo::{
    client,
    decode::Malformed,
    document::{Body, Comment, Document},
    request::{Method, Repo, Request},
    server::{self, Answer, Unrouted},
};
use alloc::boxed::Box;

#[test]
fn a_top_level_user_keeps_admin_flag_and_full_name() {
    let limits = fixtures::limits();
    let body = br#"{"id":1,"login":"fixture","full_name":"kit:bot","email":"fixture@example.invalid","is_admin":true}"#;
    let decoder = client::decoder(&Request::User, &limits).unwrap();
    let user = fixtures::bot();
    assert_eq!(fixtures::decode(decoder, body, &limits).document, Ok(Document::User(user)));
}
#[test]
fn an_embedded_user_keeps_id_and_login_alone() {
    let limits = fixtures::limits();
    let request = Request::PostComment {
        at: Repo { owner: Box::from(*b"a"), name: Box::from(*b"b") },
        number: 1,
        body: Box::from([]),
    };
    let body = br#"{"id":3,"user":{"id":1,"login":"fixture","is_admin":true,"full_name":"kit:bot"},"body":""}"#;
    let decoder = client::decoder(&request, &limits).unwrap();
    let comment = Comment {
        id: 3,
        user: fixtures::user(1, b"fixture"),
        created_at: None,
        updated_at: None,
        body: Body::Kept(Box::from([])),
    };
    assert_eq!(fixtures::decode(decoder, body, &limits).document, Ok(Document::Comment(comment)));
}
#[test]
fn a_user_without_the_two_members_decodes_with_empty_defaults() {
    let limits = fixtures::limits();
    let decoder = client::decoder(&Request::User, &limits).unwrap();
    let user = fixtures::user(1, b"fixture");
    assert_eq!(fixtures::decode(decoder, br#"{"id":1,"login":"fixture"}"#, &limits).document, Ok(Document::User(user)));
}
#[test]
fn a_full_name_past_string_bytes_is_too_long_naming_it() {
    // Eight bytes here; the shipped small profile permits 512.
    let limits = fixtures::small();
    let decoder = client::decoder(&Request::User, &limits).unwrap();
    assert_eq!(
        fixtures::decode(decoder, br#"{"id":1,"login":"a","full_name":"123456789"}"#, &limits).document,
        Err(Malformed::TooLong { field: b"full_name" })
    );
}
#[test]
fn an_admin_flag_that_is_not_a_boolean_is_malformed_naming_it() {
    let limits = fixtures::limits();
    for body in [br#"{"id":1,"login":"a","is_admin":"true"}"#.as_slice(), br#"{"id":1,"login":"a","is_admin":1}"#] {
        let decoder = client::decoder(&Request::User, &limits).unwrap();
        assert_eq!(
            fixtures::decode(decoder, body, &limits).document,
            Err(Malformed::Shape { field: b"is_admin" }),
            "{body:?}"
        );
    }
}
#[test]
fn the_servers_user_answer_decodes_to_the_user_it_encodes() {
    let limits = fixtures::limits();
    let user = fixtures::bot();
    let encoded = server::encode(&Answer::User(user.clone()), &limits).unwrap();
    let decoder = client::decoder(&Request::User, &limits).unwrap();
    assert_eq!(fixtures::decode(decoder, &encoded, &limits).document, Ok(Document::User(user)));
}
#[test]
fn a_put_is_refused_by_every_family_of_routes() {
    let limits = fixtures::limits();
    for target in [
        b"/api/v1/user".as_slice(),
        b"/api/v1/repos/a/b",
        b"/api/v1/repos/a/b/issues",
        b"/api/v1/repos/a/b/pulls/1/files",
        b"/api/v1/repos/a/b/actions/runs?head_sha=0123456789abcdef0123456789abcdef01234567&run_number=1",
        b"/api/v1/repos/a/b/branches",
    ] {
        assert_eq!(server::route(Method::Put, target, &limits).unwrap_err(), Unrouted::Method, "{target:?}");
    }
}

fn new_user() -> Request {
    Request::CreateUser {
        login: Box::from(*b"bot"),
        email: Box::from(*b"bot@example.invalid"),
        full_name: Box::from(*b"kit:bot"),
        password: skein_lib::Secret::new(Box::from(*b"private-password")),
        must_change_password: false,
    }
}
#[test]
fn a_requests_debug_shows_a_password_by_its_length_only() {
    assert!(alloc::fmt::format(format_args!("{:?}", new_user())).contains("Secret { len: 16 }"));
    assert!(!alloc::fmt::format(format_args!("{:?}", new_user())).contains("private-password"));
}
#[test]
fn a_create_user_body_names_every_member_in_order() {
    let encoded = client::encode(&new_user(), &fixtures::limits()).expect("user encoding");
    assert_eq!(encoded.body.as_deref(), Some(br#"{"username":"bot","email":"bot@example.invalid","password":"private-password","full_name":"kit:bot","must_change_password":false}"#.as_slice()));
}
