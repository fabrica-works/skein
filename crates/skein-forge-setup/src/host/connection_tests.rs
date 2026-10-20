use super::{Connection, Speech};
use crate::{
    call::{Auth, Call},
    reconcile::Reply,
    tests::{fixtures, github_send::answering, host},
};
use skein_forge::{forgejo::request as fj, github::request as gh};
use skein_lib::{Token, Wall};

// Two rows distinguish the requested capacity from both zero and body_bytes_in.
const PAGE: u32 = 2;
const HEAD: &[u8] = b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n";
const FJ_ROW: &[u8] = br#"{"id":7,"name":"kit-git","token_last_eight":"00000005","scopes":[]}"#;
const GH_ROW: &[u8] = br#"{"id":8,"owner":{"id":2,"login":"ada"},"name":"one","private":true}"#;

fn connection(call: &Call) -> Connection {
    let settings = host::github_settings();
    match call {
        Call::Forgejo(_) => Connection::new(
            Token::new(1),
            settings.forges.get(0).expect("Forgejo fixture"),
            b"kit",
            call,
            &fixtures::secret(fixtures::ADMIN),
            &host::limits(),
        )
        .expect("Forgejo call"),
        Call::GitHub(_) => Connection::github(
            Token::new(1),
            settings.forges.get(1).expect("GitHub fixture"),
            b"kit",
            call,
            &Auth::Bearer(fixtures::secret(b"installation-token")),
            Wall::EPOCH,
            &host::limits(),
        )
        .expect("GitHub call"),
    }
}
fn capacity(connection: &Connection) -> u32 {
    match &connection.speech {
        Speech::Forgejo { rows, .. } => rows.capacity(),
        Speech::GitHub { rows, .. } => rows.capacity(),
    }
}
fn page() -> fj::Page {
    fj::Page { page: Some(1), limit: Some(PAGE) }
}
fn row_calls() -> [Call; 2] {
    [
        fj::Request::Tokens { login: b"kit".as_slice().into(), page: page() }.into(),
        gh::Request::Reachable { page: 1, per_page: PAGE }.into(),
    ]
}
#[test]
fn document_and_empty_requests_reserve_no_rows() {
    for call in [
        fj::Request::User.into(),
        fj::Request::DeleteToken { login: b"kit".as_slice().into(), id: 7 }.into(),
        gh::Request::Viewer.into(),
    ] {
        assert_eq!(capacity(&connection(&call)), 0);
    }
}
#[test]
fn row_requests_reserve_exactly_their_page_size() {
    let at = fj::Repo { owner: b"acme".as_slice().into(), name: b"specimen".as_slice().into() };
    for call in [
        fj::Request::Tokens { login: b"kit".as_slice().into(), page: page() }.into(),
        fj::Request::RepoHooks { at, page: page() }.into(),
        fj::Request::RepoSearch { page: page() }.into(),
        fj::Request::UserRepos { page: page() }.into(),
        gh::Request::Reachable { page: 1, per_page: PAGE }.into(),
    ] {
        assert_eq!(capacity(&connection(&call)), PAGE);
    }
    let settings = host::github_settings();
    let installations = Connection::github(
        Token::new(1),
        settings.forges.get(1).expect("GitHub fixture"),
        b"kit",
        &gh::Request::Installations { page: 1 }.into(),
        &Auth::App { id: 7, key: fixtures::secret(skein_fake_forge_github::keys::KEYS[0]) },
        Wall::EPOCH,
        &host::limits(),
    )
    .expect("App call");
    assert_eq!(capacity(&installations), skein_forge::github::pin::PAGE_CAP);
}
fn listed(call: &Call, count: u32) -> Connection {
    let (start, row, end) = match call {
        Call::Forgejo(_) => (b"[".as_slice(), FJ_ROW, b"]".as_slice()),
        Call::GitHub(_) => (b"{\"repositories\":[".as_slice(), GH_ROW, b"]}".as_slice()),
    };
    let count = usize::try_from(count).expect("small row count");
    let length = start
        .len()
        .checked_add(end.len())
        .expect("body framing fits")
        .checked_add(row.len().checked_mul(count).expect("row bytes fit"))
        .expect("body fits")
        .checked_add(count.saturating_sub(1))
        .expect("commas fit");
    let mut body = skein_lib::Writer::new(length);
    body.put(start).expect("body fits");
    for at in 0..count {
        if at > 0 {
            body.put(b",").expect("body fits");
        }
        body.put(row).expect("body fits");
    }
    body.put(end).expect("body fits");
    answering(connection(call), HEAD, &body.finish())
}
#[test]
fn a_reply_at_the_page_size_succeeds() {
    for call in row_calls() {
        let reply = listed(&call, PAGE).result.expect("reply");
        match reply {
            Reply::Rows(rows) => assert_eq!(rows.len(), usize::try_from(PAGE).expect("page fits")),
            Reply::GithubRows(rows) => assert_eq!(rows.len(), usize::try_from(PAGE).expect("page fits")),
            other @ (Reply::Document(_)
            | Reply::Empty
            | Reply::Status(_)
            | Reply::Down
            | Reply::GithubDocument(_)
            | Reply::Unsigned) => panic!("page reply: {other:?}"),
        }
    }
}
#[test]
fn one_row_past_the_page_size_fails_the_connection() {
    for call in row_calls() {
        let connection = listed(&call, PAGE.checked_add(1).expect("small page"));
        assert_eq!(connection.result, Some(Reply::Down));
        assert_eq!(connection.closing, Some(false));
    }
}
