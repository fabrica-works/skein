#![cfg(test)]
pub mod common;
use common::forgejo;
use skein_forge::forgejo::{
    client,
    decode::Malformed,
    document::{Document, Reachable, Row},
    request::{Page, Request},
    server,
};

const ROW: &[u8] = br#"{"ok":true,"data":[{"id":7,"name":"widgets","private":true,"owner":{"id":3,"login":"acme"}}]}"#;
fn request() -> Request {
    Request::RepoSearch { page: Page { page: Some(2), limit: Some(3) } }
}
fn reachable(push: bool) -> Reachable {
    Reachable { repository: 7, owner: 3, login: Box::from(*b"acme"), name: Box::from(*b"widgets"), private: true, push }
}
#[test]
fn forgejo_repo_search_request_encodes_sort_order_page_limit() {
    let encoded = client::encode(&request(), &forgejo::limits()).unwrap();
    let (method, target, body) = common::goldens::request("repo-search");
    assert_eq!(encoded.method, method);
    assert_eq!(&*encoded.target, target);
    assert_eq!(encoded.body.as_deref().unwrap_or_default(), body);
    for (page, target) in [
        (Page { page: Some(2), limit: None }, b"/api/v1/repos/search?sort=id&order=asc&page=2".as_slice()),
        (Page { page: None, limit: Some(3) }, b"/api/v1/repos/search?sort=id&order=asc&limit=3"),
    ] {
        assert_eq!(&*client::encode(&Request::RepoSearch { page }, &forgejo::limits()).unwrap().target, target);
    }
}
#[test]
fn forgejo_repo_search_request_without_page_and_limit_keeps_sort_and_order() {
    let request = Request::RepoSearch { page: Page { page: None, limit: None } };
    let encoded = client::encode(&request, &forgejo::limits()).unwrap();
    let (method, target, body) = common::goldens::request("repo-search-unpaged");
    assert_eq!(encoded.method, method);
    assert_eq!(&*encoded.target, target);
    assert_eq!(encoded.body, None);
    assert!(body.is_empty());
    let mut limits = forgejo::limits();
    limits.target_bytes = 38;
    client::encode(&request, &limits).unwrap();
    limits.target_bytes = 37;
    assert_eq!(client::encode(&request, &limits), Err(client::Refusal::TooLong));
}
#[test]
fn forgejo_repo_search_rows_carry_id_owner_login_name_private() {
    let decoded = forgejo::decode(&request(), ROW);
    assert_eq!(decoded.document, Ok(Document::RepoSearch { ok: true }));
    assert_eq!(decoded.rows.as_slice(), &[Row::Found(reachable(true))]);
}
#[test]
fn forgejo_repo_search_every_row_reaches_push_true() {
    let body = br#"{"ok":true,"data":[{"id":7,"name":"widgets","private":true,"owner":{"id":3,"login":"acme"},"permissions":{"push":false}}]}"#;
    assert_eq!(forgejo::decode(&request(), body).rows.as_slice(), &[Row::Found(reachable(true))]);
}
#[test]
fn forgejo_repo_search_decodes_the_same_in_chunks_of_1_7_and_whole() {
    for chunk in [1, 7, ROW.len()] {
        let decoded = forgejo::decode_chunks(&request(), ROW, chunk);
        assert_eq!(decoded.document, Ok(Document::RepoSearch { ok: true }));
        assert_eq!(decoded.rows.as_slice(), &[Row::Found(reachable(true))]);
    }
}
#[test]
fn forgejo_repo_search_skips_unknown_null_and_permissions_members() {
    let body = br#"{"ok":true,"data":[{"ignored":{"nested":[null,1]},"id":7,"name":"widgets","private":true,"owner":{"id":3,"login":"acme","ignored":null},"permissions":{"push":true,"pull":null,"admin":null,"ignored":[1]}}]}"#;
    let decoded = forgejo::decode(&request(), body);
    assert_eq!(decoded.document, Ok(Document::RepoSearch { ok: true }));
    assert_eq!(decoded.rows.as_slice(), &[Row::Found(reachable(true))]);
}
#[test]
fn forgejo_repo_search_missing_row_member_is_malformed() {
    for (body, field) in [
        (
            br#"{"ok":true,"data":[{"name":"widgets","private":true,"owner":{"id":3,"login":"acme"}}]}"#.as_slice(),
            b"id".as_slice(),
        ),
        (br#"{"ok":true,"data":[{"id":7,"private":true,"owner":{"id":3,"login":"acme"}}]}"#, b"name"),
        (br#"{"ok":true,"data":[{"id":7,"name":"widgets","owner":{"id":3,"login":"acme"}}]}"#, b"private"),
        (br#"{"ok":true,"data":[{"id":7,"name":"widgets","private":true,"owner":{"login":"acme"}}]}"#, b"id"),
        (br#"{"ok":true,"data":[{"id":7,"name":"widgets","private":true,"owner":{"id":3}}]}"#, b"login"),
        (br#"{"ok":true,"data":[{"id":7,"name":"widgets","private":true}]}"#, b"owner"),
    ] {
        assert_eq!(forgejo::decode(&request(), body).document, Err(Malformed::Missing { field }));
    }
}
#[test]
fn forgejo_repo_search_wrong_type_or_duplicate_member_is_malformed() {
    for body in [
        br#"{"ok":1,"data":[]}"#.as_slice(),
        br#"{"ok":true,"ok":false,"data":[]}"#,
        br#"{"ok":true,"data":{},"ignored":null}"#,
        br#"{"ok":true,"data":[],"data":[]}"#,
        br#"{"ok":null,"data":[]}"#,
        br#"{"ok":true,"data":null}"#,
        br#"{"ok":true,"data":[{"id":"7"}]}"#.as_slice(),
        br#"{"ok":true,"data":[{"name":7}]}"#,
        br#"{"ok":true,"data":[{"private":7}]}"#,
        br#"{"ok":true,"data":[{"owner":7}]}"#,
        br#"{"ok":true,"data":[{"owner":{"id":"3"}}]}"#,
        br#"{"ok":true,"data":[{"owner":{"login":3}}]}"#,
        br#"{"ok":true,"data":[{"id":7,"id":8}]}"#,
        br#"{"ok":true,"data":[{"name":"widgets","name":"widgets"}]}"#,
        br#"{"ok":true,"data":[{"private":true,"private":false}]}"#,
        br#"{"ok":true,"data":[{"owner":{"id":3,"id":4}}]}"#,
        br#"{"ok":true,"data":[{"owner":{"login":"acme","login":"alice"}}]}"#,
    ] {
        forgejo::decode(&request(), body).document.unwrap_err();
    }
}
#[test]
fn forgejo_repo_search_server_round_trips_a_page() {
    let encoded = client::encode(&request(), &forgejo::limits()).unwrap();
    assert_eq!(forgejo::decode_route(encoded.method, &encoded.target, b"", &forgejo::limits()).unwrap().0, request());
    let body = server::encode(
        &server::Answer::RepoSearch { ok: true, data: Box::new([reachable(true), reachable(false)]) },
        &forgejo::limits(),
    )
    .unwrap();
    let decoded = forgejo::decode(&request(), &body);
    assert_eq!(decoded.document, Ok(Document::RepoSearch { ok: true }));
    assert_eq!(decoded.rows.as_slice(), &[Row::Found(reachable(true)), Row::Found(reachable(true))]);
}
#[test]
fn forgejo_repo_search_page_bound_refuses_one_extra_row() {
    let mut limits = forgejo::limits();
    limits.inner = 1;
    let body =
        server::encode(&server::Answer::RepoSearch { ok: true, data: Box::new([reachable(true)]) }, &limits).unwrap();
    let decoded = forgejo::decode_answer(client::decoder(&request(), &limits).unwrap(), &body, &limits);
    assert_eq!(decoded.document, Ok(Document::RepoSearch { ok: true }));
    assert_eq!(decoded.rows.as_slice(), &[Row::Found(reachable(true))]);
    let body = server::encode(
        &server::Answer::RepoSearch { ok: true, data: Box::new([reachable(true), reachable(false)]) },
        &limits,
    )
    .unwrap();
    let decoded = forgejo::decode_answer(client::decoder(&request(), &limits).unwrap(), &body, &limits);
    assert_eq!(decoded.document, Err(Malformed::Bound { field: b"repositories" }));
    assert_eq!(decoded.rows.as_slice(), &[Row::Found(reachable(true))]);
    assert_eq!(
        forgejo::decode(&request(), br#"{"ok":true,"data":[]}"#).document,
        Ok(Document::RepoSearch { ok: true })
    );
}

#[test]
fn forgejo_repo_search_decodes_ok_and_rows() {
    let body = br#"{"ok":true,"data":[]}"#;
    let decoded = forgejo::decode(&request(), body);
    assert_eq!(decoded.document, Ok(Document::RepoSearch { ok: true }));
    assert!(decoded.rows.is_empty());
}
#[test]
fn forgejo_repo_search_ok_false_is_reported_not_hidden() {
    let body = server::encode(
        &server::Answer::RepoSearch { ok: false, data: Box::new([reachable(false)]) },
        &forgejo::limits(),
    )
    .unwrap();
    let decoded = forgejo::decode(&request(), &body);
    assert_eq!(decoded.document, Ok(Document::RepoSearch { ok: false }));
    assert_eq!(decoded.rows.as_slice(), &[Row::Found(reachable(true))]);
}
#[test]
fn forgejo_repo_search_missing_ok_or_data_is_malformed() {
    for (body, field) in [(br#"{"data":[]}"#.as_slice(), b"ok".as_slice()), (br#"{"ok":true}"#, b"data")] {
        assert_eq!(forgejo::decode(&request(), body).document, Err(Malformed::Missing { field }));
    }
}
