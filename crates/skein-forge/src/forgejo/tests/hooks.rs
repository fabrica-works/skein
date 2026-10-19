use super::fixtures;
use crate::forgejo::{
    client,
    decode::Malformed,
    document::{Document, Hook},
    request::{Method, Request},
};
use alloc::boxed::Box;
#[test]
fn a_hook_request_writes_type_config_events_and_active() {
    let request = Request::CreateOrgHook { org: Box::from(*b"ensemble"), hook: fixtures::hook_spec() };
    let encoded = client::encode(&request, &fixtures::limits()).unwrap();
    assert_eq!(encoded.method, Method::Post);
    assert_eq!(&*encoded.target, b"/api/v1/orgs/ensemble/hooks");
    assert_eq!(encoded.body.as_deref(), Some(br#"{"type":"forgejo","config":{"url":"http://127.0.0.1:3000/hooks/1","content_type":"json","secret":"0123456789abcdef"},"events":["push"],"active":true}"#.as_slice()));
}
#[test]
fn a_hook_patch_carries_the_config_and_the_secret_again() {
    let request = Request::EditOrgHook { org: Box::from(*b"ensemble"), id: 1, hook: fixtures::hook_spec() };
    let encoded = client::encode(&request, &fixtures::limits()).unwrap();
    assert_eq!(encoded.method, Method::Patch);
    assert_eq!(&*encoded.target, b"/api/v1/orgs/ensemble/hooks/1");
    assert_eq!(encoded.body.as_deref(), Some(br#"{"config":{"url":"http://127.0.0.1:3000/hooks/1","content_type":"json","secret":"0123456789abcdef"},"events":["push"],"active":true}"#.as_slice()));
}
#[test]
fn a_hook_requests_secret_shows_only_its_length() {
    let request = Request::CreateOrgHook { org: Box::from(*b"ensemble"), hook: fixtures::hook_spec() };
    let Request::CreateOrgHook { hook, .. } = request else { panic!("a hook") };
    assert_eq!(hook.secret.len(), 16);
    assert_eq!(fixtures::debug(&hook.secret).as_ref(), b"Secret { len: 16 }");
}
#[test]
fn a_hook_answer_reads_its_target_from_config_url() {
    let limits = fixtures::limits();
    let request = Request::CreateOrgHook { org: Box::from(*b"ensemble"), hook: fixtures::hook_spec() };
    let decoder = client::decoder(&request, &limits).unwrap();
    assert_eq!(fixtures::decode(decoder, br#"{"id":1,"url":"http://wrong.invalid/","config":{"url":"http://127.0.0.1:3000/hooks/1","content_type":"json"},"events":["push"],"active":true}"#, &limits).document,
        Ok(Document::Hook(Hook { id: 1, url: Box::from(*b"http://127.0.0.1:3000/hooks/1"), events: Box::from([Box::<[u8]>::from(*b"push")]), active: true })));
}
fn refused(body: &str, limits: &crate::forgejo::limits::Limits, expected: Malformed) {
    assert_eq!(fixtures::request(Method::Post, b"/api/v1/orgs/ensemble/hooks", body.as_bytes(), limits), Err(expected));
}
#[test]
fn hook_events_past_the_inner_limit_are_refused() {
    let body = fixtures::hook_body().replace(r#"["push"]"#, r#"["push","create","delete"]"#);
    refused(
        &body,
        &crate::forgejo::limits::Limits { inner: 2, ..fixtures::limits() },
        Malformed::Bound { field: b"events" },
    );
}
#[test]
fn a_hook_body_with_a_content_type_other_than_json_is_refused() {
    let body = fixtures::hook_body().replace(r#""content_type":"json""#, r#""content_type":"form""#);
    refused(&body, &fixtures::limits(), Malformed::Value { field: b"content_type" });
}
#[test]
fn a_hook_delete_is_a_delete_with_no_body_and_no_answer() {
    let request = Request::DeleteOrgHook { org: Box::from(*b"ensemble"), id: 1 };
    let encoded = client::encode(&request, &fixtures::limits()).unwrap();
    assert_eq!(encoded.method, Method::Delete);
    assert_eq!(&*encoded.target, b"/api/v1/orgs/ensemble/hooks/1");
    assert_eq!(encoded.body, None);
    assert!(client::decoder(&request, &fixtures::limits()).is_none());
}
