use super::fixtures;
use crate::forgejo::{
    client::{self, Refusal},
    decode::Malformed,
    request::{Method, Request},
};
use alloc::boxed::Box;

fn request(org: &[u8], name: &[u8], private: bool) -> Request {
    Request::CreateRepository { org: Box::from(org), name: Box::from(name), private }
}
#[test]
fn creating_a_repository_fixes_auto_init_and_main_for_both_visibilities() {
    for (private, body) in [
        (true, br#"{"name":"starter","private":true,"auto_init":true,"default_branch":"main"}"#.as_slice()),
        (false, br#"{"name":"starter","private":false,"auto_init":true,"default_branch":"main"}"#.as_slice()),
    ] {
        let encoded = client::encode(&request(b"creation", b"starter", private), &fixtures::limits()).unwrap();
        assert_eq!(encoded.method, Method::Post);
        assert_eq!(&*encoded.target, b"/api/v1/orgs/creation/repos");
        assert_eq!(encoded.body.as_deref(), Some(body));
        assert_eq!(
            fixtures::request(Method::Post, &encoded.target, body, &fixtures::limits()).unwrap(),
            request(b"creation", b"starter", private)
        );
    }
}
#[test]
fn creating_a_repository_encodes_the_org_as_one_segment_and_the_name_as_json() {
    let encoded = client::encode(&request("a/b é".as_bytes(), b"a\"b\\c", true), &fixtures::limits()).unwrap();
    assert_eq!(&*encoded.target, b"/api/v1/orgs/a%2Fb%20%C3%A9/repos");
    assert_eq!(
        encoded.body.as_deref(),
        Some(br#"{"name":"a\"b\\c","private":true,"auto_init":true,"default_branch":"main"}"#.as_slice())
    );
}
#[test]
fn creating_a_repository_refuses_invalid_utf8_without_writing_json() {
    assert_eq!(client::encode(&request(b"creation", &[0xff], true), &fixtures::limits()), Err(Refusal::Text));
}
#[test]
fn creating_a_repository_checks_the_exact_target_and_body_bounds() {
    let request = request(b"creation", b"starter", true);
    let encoded = client::encode(&request, &fixtures::limits()).unwrap();
    let mut limits = fixtures::limits();
    limits.target_bytes = u32::try_from(encoded.target.len()).unwrap();
    client::encode(&request, &limits).expect("exact room");
    limits.target_bytes -= 1;
    assert_eq!(client::encode(&request, &limits), Err(Refusal::TooLong));
    let mut limits = fixtures::limits();
    limits.writer.length = u32::try_from(encoded.body.unwrap().len()).unwrap();
    client::encode(&request, &limits).expect("exact room");
    limits.writer.length -= 1;
    assert_eq!(client::encode(&request, &limits), Err(Refusal::TooLong));
}
#[test]
fn the_fake_refuses_creation_without_auto_init_or_main() {
    for (body, field) in [
        (
            br#"{"name":"starter","private":true,"auto_init":false,"default_branch":"main"}"#.as_slice(),
            b"auto_init".as_slice(),
        ),
        (
            br#"{"name":"starter","private":true,"auto_init":true,"default_branch":"master"}"#.as_slice(),
            b"default_branch".as_slice(),
        ),
    ] {
        assert_eq!(
            fixtures::request(Method::Post, b"/api/v1/orgs/creation/repos", body, &fixtures::limits()),
            Err(Malformed::Value { field })
        );
    }
}
#[test]
fn the_fake_refuses_wrong_shapes_and_missing_creation_fields() {
    for (body, error) in [
        (
            br#"{"name":"starter","private":"true","auto_init":true,"default_branch":"main"}"#.as_slice(),
            Malformed::Shape { field: b"private" },
        ),
        (
            br#"{"name":"starter","private":true,"default_branch":"main"}"#.as_slice(),
            Malformed::Missing { field: b"auto_init" },
        ),
    ] {
        assert_eq!(
            fixtures::request(Method::Post, b"/api/v1/orgs/creation/repos", body, &fixtures::limits()),
            Err(error)
        );
    }
}
