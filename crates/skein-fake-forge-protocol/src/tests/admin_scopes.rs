//! Scope refusals happen before setup mutates its tables (forge-setup.md, 8).
use super::fixtures::{self, Harness, boxed};
use skein_forge::forgejo::{
    client,
    request::{Method, Request},
};
use skein_lib::Secret;
const TOKEN: &[u8] = b"admin-scope-token";
fn harness(scopes: &[&[u8]]) -> Harness {
    let mut h = Harness::creating();
    h.service.setup_mut().org(b"work", b"kit-admin");
    h.service.setup_mut().token(fixtures::PERSON, b"delete-me", &[b"read:user"], Secret::new(boxed(b"delete-token")));
    h.service.setup_mut().token(fixtures::ADMIN, b"scoped", scopes, Secret::new(boxed(TOKEN)));
    h
}
fn ask(h: &mut Harness, request: &Request) {
    let encoded = client::encode(request, &h.env.limits.documents).expect("scope request");
    let method: &[u8] = match encoded.method {
        Method::Get => b"GET",
        Method::Post => b"POST",
        Method::Put => b"PUT",
        Method::Patch => b"PATCH",
        Method::Delete => b"DELETE",
    };
    h.ask_as(method, &encoded.target, encoded.body.as_deref().unwrap_or(b""), TOKEN);
}
fn cases() -> [(Request, &'static [u8], u16); 9] {
    [
        (Request::UserOf { login: boxed(b"kit") }, b"read:user", 200),
        (
            Request::CreateUser {
                login: boxed(b"new-user"),
                email: boxed(b"new@example.invalid"),
                full_name: boxed(b"kit:bot"),
                password: Secret::new(boxed(b"fixture-password")),
                must_change_password: false,
            },
            b"write:admin",
            201,
        ),
        (Request::Tokens { login: boxed(b"kit"), page: fixtures::PAGE }, b"read:admin", 200),
        (
            Request::CreateToken {
                login: boxed(b"kit"),
                name: boxed(b"made"),
                scopes: Box::new([boxed(b"read:user")]),
            },
            b"write:admin",
            201,
        ),
        (Request::DeleteToken { login: boxed(b"kit"), id: 2 }, b"write:admin", 204),
        (
            Request::CreateFile {
                at: fixtures::at(),
                path: boxed(b"file"),
                branch: boxed(b"main"),
                content: boxed(b"bytes"),
                message: boxed(b"Add file"),
                author: skein_forge::forgejo::request::Author {
                    name: boxed(b"kit"),
                    email: boxed(b"kit@example.invalid"),
                },
            },
            b"write:repository",
            201,
        ),
        (Request::Org { name: boxed(b"work") }, b"read:organization", 200),
        (Request::CreateOrg { person: boxed(b"kit-admin"), name: boxed(b"new-org") }, b"write:admin", 201),
        (
            Request::CreateRepository { org: boxed(b"work"), name: boxed(b"starter"), private: true },
            b"write:organization",
            201,
        ),
    ]
}
#[test]
fn a_missing_setup_scope_is_403_and_names_forgejos_required_scope() {
    for (request, required, _) in cases() {
        let mut h = harness(&[b"read:issue"]);
        let accounts = h.service.setup().find(b"new-user").is_some();
        let tokens = h.service.setup().issued().len();
        ask(&mut h, &request);
        assert_eq!(h.status(), 403, "{request:?}");
        let prefix = b"{\"message\":\"token does not have at least one of required scope(s): [";
        let suffix = b"]\"}";
        let mut expected =
            skein_lib::Writer::new(prefix.len().saturating_add(required.len()).saturating_add(suffix.len()));
        for part in [prefix.as_slice(), required, suffix] {
            expected.put(part).expect("sized scope error");
        }
        assert_eq!(h.body().as_ref(), expected.finish().as_ref());

        assert_eq!(h.service.setup().find(b"new-user").is_some(), accounts);
        assert_eq!(h.service.setup().issued().len(), tokens);
        assert!(h.service.setup().organization(b"new-org").is_none());
    }
}
#[test]
fn each_least_setup_scope_admits_its_route_and_write_implies_read() {
    for (request, scope, status) in cases() {
        let write = if scope == b"read:user" {
            b"write:user".as_slice()
        } else if scope == b"read:admin" {
            b"write:admin"
        } else if scope == b"read:organization" {
            b"write:organization"
        } else {
            scope
        };
        for granted in [scope, write, b"all"] {
            let mut h = harness(&[granted]);
            ask(&mut h, &request);
            assert_eq!(h.status(), status, "{request:?} {granted:?}");
        }
    }
}
#[test]
fn read_scopes_are_refused_setup_writes() {
    for (request, scope, _) in cases() {
        let read: &[u8] = if scope == b"write:admin" {
            b"read:admin".as_slice()
        } else if scope == b"write:organization" {
            b"read:organization"
        } else if scope == b"write:repository" {
            b"read:repository"
        } else {
            continue;
        };
        let mut h = harness(&[read]);
        ask(&mut h, &request);
        assert_eq!(h.status(), 403, "{request:?}");
    }
}
#[test]
fn write_admin_does_not_grant_organization_reads_or_repository_creation() {
    for request in [
        Request::Org { name: boxed(b"work") },
        Request::CreateRepository { org: boxed(b"work"), name: boxed(b"starter"), private: true },
    ] {
        let mut h = harness(&[b"write:admin", b"read:user", b"write:repository"]);
        ask(&mut h, &request);
        assert_eq!(h.status(), 403, "{request:?}");
    }
}
#[test]
fn settings_needs_no_scope_from_an_authenticated_token() {
    let mut h = harness(&[b"read:issue"]);
    ask(&mut h, &Request::ApiSettings);
    assert_eq!(h.status(), 200);
}
