fn copies(ids: &[&[u8]]) -> Box<[Box<[u8]>]> {
    let mut out = List::with_capacity(u32::try_from(ids.len()).expect("fixture length"));
    for id in ids {
        out.push(bytes::copy_of(id)).expect("capacity");
    }
    out.into_boxed()
}
use super::*;

fn limits() -> Limits {
    Limits {
        document: DocumentLimits {
            document_bytes: 4096,
            string_bytes: 1024,
            token_bytes: 256,
            client_bytes: 64,
            detail_bytes: 64,
            record_bytes: 2048,
            depth: 8,
            tokens: 128,
            issued_client_bytes: 64,
            granted_bytes: 64,
            subject_bytes: 64,
            id_token_bytes: 1024,
        },
        uri_bytes: 256,
        request_bytes: 2048,
        codes: 8,
        rotations: 8,
        plans: 8,
        carried: 8,
    }
}
fn config() -> Config {
    Config {
        authorization_url: bytes::copy_of(b"https://issuer.example/authorize"),
        token_endpoint: bytes::copy_of(b"https://issuer.example/token"),
        client_id: bytes::copy_of(b"client"),
        client_secret: Some(bytes::copy_of(b"secret")),
        redirect_uri: bytes::copy_of(b"http://127.0.0.1:1234/callback"),
        refresh_token: bytes::copy_of(b"seed"),
        terms: None,
    }
}
fn terms(ids: &[&[u8]]) -> Terms {
    Terms {
        issued: copies(ids),
        omit_issued: false,
        revocation_endpoint: Some(bytes::copy_of(b"https://issuer.example/revoke")),
        varying_port: false,
    }
}
fn issuer(terms: Terms) -> Issuer {
    let mut config = config();
    config.terms = Some(terms);
    Issuer::new(config, limits()).expect("issuer")
}
fn step(issuer: &mut Issuer, event: Event) -> Option<Request> {
    let mut out = Queue::with_capacity(MAX_OUT);
    issuer.step(event, &mut out);
    out.pop()
}
fn authorize(issuer: &mut Issuer) -> Option<Request> {
    authorization(issuer, b"http://127.0.0.1:1234/callback", b"")
}
fn authorization(issuer: &mut Issuer, redirect: &[u8], extra: &[u8]) -> Option<Request> {
    let url = joined(&[
        b"https://issuer.example/authorize?response_type=code&client_id=client&redirect_uri=",
        redirect,
        b"&state=0123456789abcdef",
        extra,
    ]);
    step(issuer, Event::Authorize { url, now: Time::ZERO })
}
fn plan(status: u16, delay: u64) -> Plan {
    Plan {
        status,
        body: Body::Raw(bytes::copy_of(b"{}")),
        delay: Duration::from_secs(delay),
        retry_after: Duration::ZERO,
    }
}
fn post(issuer: &mut Issuer, endpoint: &[u8], body: &[u8]) -> Option<Request> {
    step(
        issuer,
        Event::Post {
            request: HttpRequest {
                id: 1,
                endpoint: bytes::copy_of(endpoint),
                content_type: b"application/x-www-form-urlencoded",
                body: bytes::copy_of(body),
                deadline: Time::ZERO.saturating_add(Duration::from_secs(10)),
            },
            now: Time::ZERO,
            wall: Wall::EPOCH,
        },
    )
}
fn token_post(issuer: &mut Issuer, body: &[u8]) -> Option<Request> {
    post(issuer, b"https://issuer.example/token", body)
}
fn revoke(issuer: &mut Issuer) -> Option<Request> {
    post(
        issuer,
        b"https://issuer.example/revoke",
        b"token=seed&token_type_hint=refresh_token&client_id=client&client_secret=secret",
    )
}
fn refresh(issuer: &mut Issuer) -> Option<Request> {
    token_post(issuer, b"grant_type=refresh_token&refresh_token=seed&client_id=client&client_secret=secret")
}
fn status(request: Option<Request>) -> u16 {
    let Some(Request::Http(answer)) = request else { panic!("HTTP answer") };
    answer.status
}
#[test]
fn an_issued_registration_redirects_with_each_issued_id() {
    let mut issuer = issuer(terms(&[b"client-1", b"client-2"]));
    for expected in [b"client-1".as_slice(), b"client-2", b"client-2"] {
        let Some(Request::Redirect { client_id, .. }) = authorize(&mut issuer) else { panic!("redirect expected") };
        assert_eq!(client_id.as_deref(), Some(expected));
    }
}
#[test]
fn an_omitted_issue_leaves_the_redirect_without_a_client_id() {
    let mut terms = terms(&[b"client-1"]);
    terms.omit_issued = true;
    let Some(Request::Redirect { client_id, .. }) = authorize(&mut issuer(terms)) else { panic!("redirect") };
    assert_eq!(client_id, None);
}
#[test]
fn a_later_request_without_the_latest_issued_id_is_refused() {
    for (endpoint, body) in [
        (b"https://issuer.example/token".as_slice(), b"grant_type=authorization_code&code=0000000000000001&redirect_uri=http://127.0.0.1:1234/callback&client_id=client&client_secret=secret".as_slice()),
        (b"https://issuer.example/token", b"grant_type=refresh_token&refresh_token=seed&client_id=client&client_secret=secret"),
        (b"https://issuer.example/revoke", b"token=seed&token_type_hint=refresh_token&client_id=client&client_secret=secret"),
    ] {
        let mut issuer = issuer(terms(&[b"client-1"])); drop(authorize(&mut issuer));
        let Some(Request::Http(answer)) = post(&mut issuer, endpoint, body) else { panic!("answer") };
        assert_eq!(answer.status, 400);
        assert!(bytes::find(&answer.body, b"invalid_grant").is_some());
    }
}
#[test]
fn carried_names_each_added_name_in_order() {
    let mut issuer = issuer(terms(&[]));
    drop(authorization(
        &mut issuer,
        b"http://127.0.0.1:1234/callback",
        b"&added=a&resource=b&nonce=c&id_token_hint=d&login_hint=e",
    ));
    let names = issuer.carried();
    assert_eq!(names.len(), 5);
    for (name, expected) in
        names.iter().zip([b"added".as_slice(), b"resource", b"nonce", b"id_token_hint", b"login_hint"])
    {
        assert_eq!(name.as_ref(), expected);
    }
}
#[test]
fn carried_names_stop_at_their_bound() {
    const NAMES: u32 = 2; // Two scripted added fields fit; the third exceeds the bound.
    for (extra, accepted) in [(b"&a=1&b=2".as_slice(), true), (b"&a=1&b=2&c=3", false)] {
        let mut limits = limits();
        limits.carried = NAMES;
        let mut issuer = Issuer::new(config(), limits).expect("issuer");
        let answer = authorization(&mut issuer, b"http://127.0.0.1:1234/callback", extra);
        assert_eq!(is_redirect(answer), accepted);
    }
}
#[test]
fn a_varying_port_redirect_is_held_for_its_exchange() {
    for (configured, bound) in [
        (b"http://127.0.0.1:0/callback".as_slice(), b"http://127.0.0.1:5678/callback".as_slice()),
        (b"http://[::1]:0/callback", b"http://[::1]:5678/callback"),
    ] {
        let mut config = config();
        config.redirect_uri = bytes::copy_of(configured);
        let mut terms = terms(&[]);
        terms.varying_port = true;
        config.terms = Some(terms);
        let mut issuer = Issuer::new(config, limits()).expect("issuer");
        issuer.queue(plan(200, 0)).expect("plan");
        let Some(Request::Redirect { uri, code, .. }) = authorization(&mut issuer, bound, b"") else {
            panic!("redirect")
        };
        assert_eq!(uri.as_ref(), bound);
        let body = joined(&[
            b"grant_type=authorization_code&client_id=client&client_secret=secret&code=",
            &code,
            b"&redirect_uri=",
            bound,
        ]);
        assert_eq!(status(token_post(&mut issuer, &body)), 200);
    }
}
#[test]
fn a_varying_port_is_refused_off_loopback_or_at_exchange() {
    for uri in [b"http://localhost:5678/callback".as_slice(), b"http://127.0.0.1:5678/other"] {
        let mut terms = terms(&[]);
        terms.varying_port = true;
        let mut issuer = issuer(terms);
        assert!(is_refused(authorization(&mut issuer, uri, b"")));
    }
    let mut terms = terms(&[]);
    terms.varying_port = true;
    let mut issuer = issuer(terms);
    drop(authorization(&mut issuer, b"http://127.0.0.1:5678/callback", b""));
    assert_eq!(status(token_post(&mut issuer, b"grant_type=authorization_code&client_id=client&client_secret=secret&code=0000000000000001&redirect_uri=http://127.0.0.1:9999/callback")), 400);
}
#[test]
fn a_confirmed_revocation_spends_the_refresh_token() {
    let mut issuer = issuer(terms(&[]));
    issuer.queue(plan(200, 0)).expect("plan");
    drop(revoke(&mut issuer));
    assert_eq!(status(refresh(&mut issuer)), 400);
}
#[test]
fn an_unconfirmed_revocation_leaves_the_refresh_token() {
    for (status_code, delay) in [(400, 0), (503, 0), (200, 20)] {
        let mut issuer = issuer(terms(&[]));
        issuer.queue(plan(status_code, delay)).expect("plan");
        drop(revoke(&mut issuer));
        drop(step(&mut issuer, Event::LoseResponse));
        issuer.queue(plan(200, 0)).expect("plan");
        assert_eq!(status(refresh(&mut issuer)), 200);
    }
}
#[test]
fn granted_lists_queued_tokens_in_answer_order() {
    const GRANTS: usize = 3; // Three successful token plans below.
    let mut issuer = issuer(terms(&[]));
    for access in [b"access-1".as_slice(), b"access-2", b"access-3"] {
        issuer.queue(plan(400, 0)).expect("error");
        issuer
            .queue(Plan {
                status: 200,
                body: Body::Token(TokenResponse {
                    access_token: bytes::copy_of(access),
                    refresh_token: None,
                    expires_in: 30,
                    scope: None,
                    id_token: None,
                    earliest_refresh_at: None,
                }),
                delay: Duration::ZERO,
                retry_after: Duration::ZERO,
            })
            .expect("token");
    }
    let tokens = issuer.granted();
    assert_eq!(tokens.len(), GRANTS);
    for (index, token) in tokens.iter().enumerate() {
        assert!(
            token == &bytes::copy_of([b"access-1".as_slice(), b"access-2", b"access-3"].get(index).expect("index"))
        );
    }
}

fn is_redirect(answer: Option<Request>) -> bool {
    match answer {
        Some(Request::Redirect { .. }) => true,
        Some(Request::Http(_) | Request::Refused) | None => false,
    }
}
fn is_refused(answer: Option<Request>) -> bool {
    match answer {
        Some(Request::Refused) => true,
        Some(Request::Http(_) | Request::Redirect { .. }) | None => false,
    }
}

fn joined(parts: &[&[u8]]) -> Box<[u8]> {
    let mut size = 0_usize;
    for part in parts {
        size = size.checked_add(part.len()).expect("fixture fits");
    }
    let mut out = Writer::new(size);
    for part in parts {
        out.put(part).expect("measured");
    }
    out.finish()
}
