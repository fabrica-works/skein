#![cfg(test)]
pub mod common;
use common::forgejo;
use skein_forge::forgejo::{
    client,
    decode::Malformed,
    document::{Document, Reachable, Row},
    request::{Method, Page, Request},
    server,
};

const ROW: &[u8] = br#"[{"id":7,"name":"widgets","private":true,"owner":{"id":3,"login":"acme"},"permissions":{"push":true,"pull":true,"admin":false}}]"#;
fn request() -> Request {
    Request::UserRepos { page: Page { page: Some(2), limit: Some(3) } }
}
fn reachable(push: bool) -> Reachable {
    Reachable { repository: 7, owner: 3, login: Box::from(*b"acme"), name: Box::from(*b"widgets"), private: true, push }
}
#[test]
fn forgejo_user_repos_request_encodes_page_and_limit() {
    let encoded = client::encode(&request(), &forgejo::limits()).unwrap();
    let (method, target, body) = common::goldens::request("user-repos");
    assert_eq!(encoded.method, method);
    assert_eq!(&*encoded.target, target);
    assert_eq!(encoded.body.as_deref().unwrap_or_default(), body);
    for (page, target) in [
        (Page { page: Some(2), limit: None }, b"/api/v1/user/repos?page=2".as_slice()),
        (Page { page: None, limit: Some(3) }, b"/api/v1/user/repos?limit=3"),
    ] {
        assert_eq!(&*client::encode(&Request::UserRepos { page }, &forgejo::limits()).unwrap().target, target);
    }
}
#[test]
fn forgejo_user_repos_request_without_page_has_no_query() {
    let request = Request::UserRepos { page: Page { page: None, limit: None } };
    let encoded = client::encode(&request, &forgejo::limits()).unwrap();
    assert_eq!(encoded.method, Method::Get);
    assert_eq!(&*encoded.target, b"/api/v1/user/repos");
    assert_eq!(encoded.body, None);
    let mut limits = forgejo::limits();
    limits.target_bytes = 18;
    client::encode(&request, &limits).unwrap();
    limits.target_bytes = 17;
    assert_eq!(client::encode(&request, &limits), Err(client::Refusal::TooLong));
}
#[test]
fn forgejo_user_repos_rows_decode_id_owner_name_private_push() {
    let decoded = forgejo::decode(&request(), ROW);
    assert_eq!(decoded.document, Ok(Document::Listed { skipped: 0 }));
    assert_eq!(decoded.rows.as_slice(), &[Row::Reachable(reachable(true))]);
}
#[test]
fn forgejo_user_repos_row_with_push_false_is_kept() {
    let body =
        br#"[{"id":7,"name":"widgets","private":true,"owner":{"id":3,"login":"acme"},"permissions":{"push":false}}]"#;
    assert_eq!(forgejo::decode(&request(), body).rows.as_slice(), &[Row::Reachable(reachable(false))]);
}
#[test]
fn forgejo_user_repos_decodes_the_same_in_chunks_of_1_7_and_whole() {
    for chunk in [1, 7, ROW.len()] {
        let decoded = forgejo::decode_chunks(&request(), ROW, chunk);
        assert_eq!(decoded.document, Ok(Document::Listed { skipped: 0 }));
        assert_eq!(decoded.rows.as_slice(), &[Row::Reachable(reachable(true))]);
    }
}
#[test]
fn forgejo_user_repos_skips_unknown_and_null_members() {
    let body = br#"[{"ignored":{"nested":[null,1]},"id":7,"name":"widgets","private":true,"owner":{"id":3,"login":"acme","ignored":null},"permissions":{"push":true,"pull":null,"admin":null,"ignored":[1]}}]"#;
    let decoded = forgejo::decode(&request(), body);
    assert_eq!(decoded.document, Ok(Document::Listed { skipped: 0 }));
    assert_eq!(decoded.rows.as_slice(), &[Row::Reachable(reachable(true))]);
}
#[test]
fn forgejo_user_repos_missing_required_member_is_malformed() {
    for (body, field) in [
        (
            br#"[{"name":"widgets","private":true,"owner":{"id":3,"login":"acme"},"permissions":{"push":true}}]"#
                .as_slice(),
            b"id".as_slice(),
        ),
        (br#"[{"id":7,"private":true,"owner":{"id":3,"login":"acme"},"permissions":{"push":true}}]"#, b"name"),
        (br#"[{"id":7,"name":"widgets","owner":{"id":3,"login":"acme"},"permissions":{"push":true}}]"#, b"private"),
        (br#"[{"id":7,"name":"widgets","private":true,"owner":{"login":"acme"},"permissions":{"push":true}}]"#, b"id"),
        (br#"[{"id":7,"name":"widgets","private":true,"owner":{"id":3},"permissions":{"push":true}}]"#, b"login"),
        (br#"[{"id":7,"name":"widgets","private":true,"owner":{"id":3,"login":"acme"},"permissions":{}}]"#, b"push"),
        (br#"[{"id":7,"name":"widgets","private":true,"permissions":{"push":true}}]"#, b"owner"),
        (br#"[{"id":7,"name":"widgets","private":true,"owner":{"id":3,"login":"acme"}}]"#, b"permissions"),
    ] {
        assert_eq!(forgejo::decode(&request(), body).document, Err(Malformed::Missing { field }));
    }
}
#[test]
fn forgejo_user_repos_wrong_type_or_duplicate_member_is_malformed() {
    for body in [
        br#"[{"id":"7"}]"#.as_slice(),
        br#"[{"name":7}]"#,
        br#"[{"private":7}]"#,
        br#"[{"owner":7}]"#,
        br#"[{"owner":{"id":"3"}}]"#,
        br#"[{"owner":{"login":3}}]"#,
        br#"[{"permissions":7}]"#,
        br#"[{"permissions":{"push":"true"}}]"#,
        br#"[{"permissions":{"pull":"true"}}]"#,
        br#"[{"permissions":{"admin":1}}]"#,
        br#"[{"id":7,"id":8}]"#,
        br#"[{"name":"widgets","name":"widgets"}]"#,
        br#"[{"private":true,"private":false}]"#,
        br#"[{"owner":{"id":3,"id":4}}]"#,
        br#"[{"owner":{"login":"acme","login":"alice"}}]"#,
        br#"[{"permissions":{"push":true,"push":false}}]"#,
    ] {
        forgejo::decode(&request(), body).document.unwrap_err();
    }
}
#[test]
fn forgejo_user_repos_server_round_trips_a_page() {
    let encoded = client::encode(&request(), &forgejo::limits()).unwrap();
    assert_eq!(forgejo::decode_route(encoded.method, &encoded.target, b"", &forgejo::limits()).unwrap().0, request());
    let body =
        server::encode(&server::Answer::Reachable(Box::new([reachable(true), reachable(false)])), &forgejo::limits())
            .unwrap();
    let decoded = forgejo::decode(&request(), &body);
    assert_eq!(decoded.document, Ok(Document::Listed { skipped: 0 }));
    assert_eq!(decoded.rows.as_slice(), &[Row::Reachable(reachable(true)), Row::Reachable(reachable(false))]);
}
#[test]
fn forgejo_user_repos_page_bound_refuses_one_extra_row() {
    let mut limits = forgejo::limits();
    limits.inner = 1;
    let body = server::encode(&server::Answer::Reachable(Box::new([reachable(true)])), &limits).unwrap();
    let decoded = forgejo::decode_answer(client::decoder(&request(), &limits).unwrap(), &body, &limits);
    assert_eq!(decoded.document, Ok(Document::Listed { skipped: 0 }));
    assert_eq!(decoded.rows.as_slice(), &[Row::Reachable(reachable(true))]);
    let body =
        server::encode(&server::Answer::Reachable(Box::new([reachable(true), reachable(false)])), &limits).unwrap();
    let decoded = forgejo::decode_answer(client::decoder(&request(), &limits).unwrap(), &body, &limits);
    assert_eq!(decoded.document, Err(Malformed::Bound { field: b"repositories" }));
    assert_eq!(decoded.rows.as_slice(), &[Row::Reachable(reachable(true))]);
    assert_eq!(forgejo::decode(&request(), b"[]").document, Ok(Document::Listed { skipped: 0 }));
}
