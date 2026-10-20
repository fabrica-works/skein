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

fn repo() -> crate::forgejo::request::Repo {
    crate::forgejo::request::Repo { owner: Box::from(*b"acme"), name: Box::from(*b"widgets") }
}
fn repo_spec() -> crate::forgejo::request::HookSpec {
    crate::forgejo::request::HookSpec { url: Box::from(*b"https://hooks.example/"), ..fixtures::hook_spec() }
}
const REPO_ANSWER: &str = r#"{"id":1,"url":"https://hooks.example/wrong","config":{"url":"https://hooks.example/","content_type":"json"},"events":["push"],"active":true}"#;
#[test]
fn forgejo_hook_repo_create_encodes_target_and_body() {
    let request = Request::CreateRepoHook { at: repo(), hook: repo_spec() };
    let encoded = client::encode(&request, &fixtures::limits()).unwrap();
    assert_eq!(encoded.method, Method::Post);
    assert_eq!(encoded.target.as_ref(), b"/api/v1/repos/acme/widgets/hooks");
    let expected = fixtures::hook_body().replace("http://127.0.0.1:3000/hooks/1", "https://hooks.example/");
    assert_eq!(encoded.body.as_deref(), Some(expected.as_bytes()));
}
#[test]
fn forgejo_hook_repo_list_encodes_page_and_limit() {
    use crate::forgejo::request::Page;
    for (page, target) in [
        (Page { page: Some(2), limit: Some(3) }, b"/api/v1/repos/acme/widgets/hooks?page=2&limit=3".as_slice()),
        (Page { page: Some(2), limit: None }, b"/api/v1/repos/acme/widgets/hooks?page=2"),
        (Page { page: None, limit: Some(3) }, b"/api/v1/repos/acme/widgets/hooks?limit=3"),
        (Page { page: None, limit: None }, b"/api/v1/repos/acme/widgets/hooks"),
    ] {
        let encoded = client::encode(&Request::RepoHooks { at: repo(), page }, &fixtures::limits()).unwrap();
        assert_eq!(encoded.method, Method::Get);
        assert_eq!(encoded.target.as_ref(), target);
        assert!(encoded.body.is_none());
    }
}
#[test]
fn forgejo_hook_repo_edit_carries_the_config_and_the_secret_again() {
    let request = Request::EditRepoHook { at: repo(), id: 7, hook: repo_spec() };
    let encoded = client::encode(&request, &fixtures::limits()).unwrap();
    assert_eq!(encoded.method, Method::Patch);
    assert_eq!(encoded.target.as_ref(), b"/api/v1/repos/acme/widgets/hooks/7");
    let expected = fixtures::hook_body()
        .replace("http://127.0.0.1:3000/hooks/1", "https://hooks.example/")
        .replace(r#""type":"forgejo","#, "");
    assert_eq!(encoded.body.as_deref(), Some(expected.as_bytes()));
}
#[test]
fn forgejo_hook_repo_delete_has_no_body_and_no_decoder() {
    let request = Request::DeleteRepoHook { at: repo(), id: 7 };
    let encoded = client::encode(&request, &fixtures::limits()).unwrap();
    assert_eq!(encoded.method, Method::Delete);
    assert_eq!(encoded.target.as_ref(), b"/api/v1/repos/acme/widgets/hooks/7");
    assert!(encoded.body.is_none());
    assert!(client::decoder(&request, &fixtures::limits()).is_none());
}
fn decode_repo(body: &[u8], limits: &crate::forgejo::limits::Limits) -> fixtures::Decoded {
    fixtures::decode(
        client::decoder(&Request::CreateRepoHook { at: repo(), hook: repo_spec() }, limits).unwrap(),
        body,
        limits,
    )
}
#[test]
fn forgejo_hook_repo_answer_reads_its_url_from_config() {
    assert_eq!(
        decode_repo(REPO_ANSWER.as_bytes(), &fixtures::limits()).document,
        Ok(Document::Hook(Hook { id: 1, url: repo_spec().url, events: repo_spec().events, active: true }))
    );
}
#[test]
fn forgejo_hook_repo_content_type_other_than_json_is_malformed() {
    let body = REPO_ANSWER.replace(r#""content_type":"json""#, r#""content_type":"form""#);
    assert_eq!(
        decode_repo(body.as_bytes(), &fixtures::limits()).document,
        Err(Malformed::Value { field: b"content_type" })
    );
    let body = fixtures::hook_body().replace(r#""content_type":"json""#, r#""content_type":"form""#);
    assert_eq!(
        fixtures::request(Method::Post, b"/api/v1/repos/acme/widgets/hooks", body.as_bytes(), &fixtures::limits()),
        Err(Malformed::Value { field: b"content_type" })
    );
}
#[test]
fn forgejo_hook_repo_missing_member_is_malformed() {
    for (body, field) in [
        (br#"{"config":{"url":"https://hooks.example/"},"events":[],"active":true}"#.as_slice(), b"id".as_slice()),
        (br#"{"id":1,"events":[],"active":true}"#, b"config"),
        (br#"{"id":1,"config":{},"events":[],"active":true}"#, b"url"),
        (br#"{"id":1,"config":{"url":"https://hooks.example/"},"active":true}"#, b"events"),
        (br#"{"id":1,"config":{"url":"https://hooks.example/"},"events":[]}"#, b"active"),
    ] {
        assert_eq!(decode_repo(body, &fixtures::limits()).document, Err(Malformed::Missing { field }));
    }
}
#[test]
fn forgejo_hook_repo_events_past_the_inner_limit_are_refused() {
    let limits = crate::forgejo::limits::Limits { inner: 0, ..fixtures::limits() };
    assert_eq!(decode_repo(REPO_ANSWER.as_bytes(), &limits).document, Err(Malformed::Bound { field: b"events" }));
    assert_eq!(
        fixtures::request(Method::Post, b"/api/v1/repos/acme/widgets/hooks", fixtures::hook_body().as_bytes(), &limits),
        Err(Malformed::Bound { field: b"events" })
    );
}
#[test]
fn forgejo_hook_server_round_trips_create_edit_list_delete() {
    use crate::forgejo::{request::Page, server};
    for request in [
        Request::CreateRepoHook { at: repo(), hook: repo_spec() },
        Request::EditRepoHook { at: repo(), id: 7, hook: repo_spec() },
        Request::RepoHooks { at: repo(), page: Page { page: Some(2), limit: Some(3) } },
        Request::DeleteRepoHook { at: repo(), id: 7 },
    ] {
        let encoded = client::encode(&request, &fixtures::limits()).unwrap();
        let decoded = match encoded.body {
            Some(body) => fixtures::request(encoded.method, &encoded.target, &body, &fixtures::limits()).unwrap(),
            None => {
                let server::Route::Ready(value) =
                    server::route(encoded.method, &encoded.target, &fixtures::limits()).unwrap()
                else {
                    panic!("ready route")
                };
                value
            }
        };
        assert_eq!(decoded, request);
    }
    for (method, target) in [
        (Method::Put, b"/api/v1/repos/acme/widgets/hooks".as_slice()),
        (Method::Get, b"/api/v1/repos/acme/widgets/hooks/1"),
        (Method::Post, b"/api/v1/repos/acme/widgets/hooks/1"),
    ] {
        assert_eq!(server::route(method, target, &fixtures::limits()).unwrap_err(), server::Unrouted::Method);
    }
    assert_eq!(
        server::route(Method::Get, b"/api/v1/repos/acme/widgets/hooks/1/extra", &fixtures::limits()).unwrap_err(),
        server::Unrouted::NotFound
    );
}
