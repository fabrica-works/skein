use super::fixtures;
use crate::forgejo::{
    client,
    decode::Malformed,
    request::{Grant, Method, Repo, Request},
    server,
};
use alloc::boxed::Box;
fn request(permission: Grant) -> Request {
    Request::AddCollaborator {
        at: Repo { owner: Box::from(*b"acme"), name: Box::from(*b"widgets") },
        login: Box::from(*b"alice"),
        permission,
    }
}
#[test]
fn forgejo_collaborator_put_encodes_target_and_permission_body() {
    let encoded = client::encode(&request(Grant::Write), &fixtures::limits()).unwrap();
    assert_eq!(encoded.method, Method::Put);
    assert_eq!(encoded.target.as_ref(), b"/api/v1/repos/acme/widgets/collaborators/alice");
    assert_eq!(encoded.body.as_deref(), Some(br#"{"permission":"write"}"#.as_slice()));
}
#[test]
fn forgejo_collaborator_put_writes_each_permission_spelling() {
    for (grant, body) in [
        (Grant::Read, br#"{"permission":"read"}"#.as_slice()),
        (Grant::Write, br#"{"permission":"write"}"#),
        (Grant::Admin, br#"{"permission":"admin"}"#),
    ] {
        assert_eq!(client::encode(&request(grant), &fixtures::limits()).unwrap().body.as_deref(), Some(body));
    }
}
#[test]
fn forgejo_collaborator_put_has_no_decoder() {
    assert!(client::decoder(&request(Grant::Write), &fixtures::limits()).is_none());
}
#[test]
fn forgejo_collaborator_server_round_trips_the_put() {
    for grant in [Grant::Read, Grant::Write, Grant::Admin] {
        let request = request(grant);
        let encoded = client::encode(&request, &fixtures::limits()).unwrap();
        assert_eq!(
            fixtures::request(encoded.method, &encoded.target, encoded.body.as_deref().unwrap(), &fixtures::limits()),
            Ok(request)
        );
    }
    for method in [Method::Get, Method::Post, Method::Patch, Method::Delete] {
        assert_eq!(
            server::route(method, b"/api/v1/repos/acme/widgets/collaborators/alice", &fixtures::limits()).unwrap_err(),
            server::Unrouted::Method
        );
    }
    assert_eq!(
        server::route(Method::Put, b"/api/v1/repos/acme/widgets/collaborators/alice/permission", &fixtures::limits())
            .unwrap_err(),
        server::Unrouted::Method
    );
}
#[test]
fn forgejo_collaborator_server_refuses_an_unknown_permission_or_a_missing_one() {
    for (body, expected) in [
        (br#"{"permission":"owner"}"#.as_slice(), Malformed::Value { field: b"permission" }),
        (br#"{"permission":""}"#, Malformed::Value { field: b"permission" }),
        (b"{}", Malformed::Missing { field: b"permission" }),
        (br#"{"permission":"write","permission":"read"}"#, Malformed::Shape { field: b"permission" }),
        (br#"{"permission":1}"#, Malformed::Shape { field: b"permission" }),
        (br#"{"permission":null}"#, Malformed::Missing { field: b"permission" }),
        (br#"{"permission":{}}"#, Malformed::Shape { field: b"permission" }),
    ] {
        assert_eq!(
            fixtures::request(
                Method::Put,
                b"/api/v1/repos/acme/widgets/collaborators/alice",
                body,
                &fixtures::limits()
            ),
            Err(expected)
        );
    }
}
