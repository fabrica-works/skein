//! Bound redirect ports, client fields and request-ordered seeded draws.
use super::private::Owner;
use super::*;
use skein_lib::Token;

const ZERO: &[u8] = b"http://127.0.0.1:0/callback";
const PREFERRED: &[u8] = b"http://127.0.0.1:31234/callback";
const BOUND: &[u8] = b"http://127.0.0.1:41000/callback";
const ADDRESS: core::net::SocketAddr =
    core::net::SocketAddr::new(core::net::IpAddr::V4(core::net::Ipv4Addr::LOCALHOST), 31000);

fn varying(mut account: Account, uri: &[u8]) -> Account {
    if let Account::SignIn { registration, .. } = &mut account {
        let mut terms = issued_terms();
        terms.issued_client_id = false;
        terms.varying_port = true;
        registration.terms = Some(terms);
        registration.redirect_uri = bytes::copy_of(uri);
    }
    account
}

fn owner(accounts: impl IntoIterator<Item = Account>, profile: &Limits) -> Owner {
    let mut configured = List::with_capacity(profile.accounts);
    for account in accounts {
        configured.push(account).ok().expect("configured account room");
    }
    Owner {
        component: Component::new(configured, profile, crypto(17)).expect("valid registrations"),
        env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: *profile },
        above: Queue::with_capacity(32),
        io: Queue::with_capacity(32),
        files: Queue::with_capacity(32),
    }
}
fn public(uri: &[u8]) -> Owner {
    owner([varying(refresh_account(0, ADDRESS), uri)], &limits())
}
fn fire(owner: &mut Owner) {
    owner.component.fire(&owner.env, &mut owner.above, &mut owner.io, &mut owner.files);
}
fn socket(owner: &mut Owner, event: LowerEvent) {
    owner.component.up(&owner.env, event, &mut owner.above, &mut owner.io, &mut owner.files);
}
fn take_listen(owner: &mut Owner) -> (Token, u16) {
    let Lower::Listen { owner, addr: skein_io::Addr::Ip(address) } = owner.io.pop().expect("listen effect") else {
        panic!("IP listen effect")
    };
    (owner, address.port())
}
fn listen(owner: &mut Owner) -> Token {
    take_listen(owner).0
}
fn listening(owner: &mut Owner, identity: Token, port: u16) {
    socket(
        owner,
        LowerEvent::Listening {
            owner: identity,
            listener: Token::new(10),
            addr: skein_io::Addr::Ip(([127, 0, 0, 1], port).into()),
        },
    );
}
fn visit(owner: &mut Owner) -> Box<[u8]> {
    for _ in 0_u32..8 {
        fire(owner);
        if let Some(Event::Visit { url, .. }) = owner.above.pop() {
            return url;
        }
    }
    panic!("visit after listening")
}
fn query<'a>(url: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    let mut query = url.get(bytes::find(url, b"?")?.checked_add(1)?..)?;
    loop {
        let end = bytes::find(query, b"&").unwrap_or(query.len());
        let pair = query.get(..end)?;
        let at = bytes::find(pair, b"=")?;
        if pair.get(..at)? == name {
            return pair.get(at.checked_add(1)?..);
        }
        query = query.get(end.checked_add(1)?..)?;
    }
}

#[test]
fn an_account_with_port_zero_listens_on_port_zero() {
    let mut owner = public(ZERO);
    owner.ask(Request::SignIn { account: 0 });
    assert_eq!(take_listen(&mut owner).1, 0);
}
#[test]
fn an_account_with_port_zero_visits_at_its_bound_port() {
    let mut owner = public(ZERO);
    owner.ask(Request::SignIn { account: 0 });
    let identity = listen(&mut owner);
    listening(&mut owner, identity, 41000);
    assert!(query(&visit(&mut owner), b"redirect_uri") == Some(b"http%3A%2F%2F127.0.0.1%3A41000%2Fcallback"));
}
fn fixed_zero() -> Account {
    let mut account = refresh_account(0, ADDRESS);
    if let Account::SignIn { registration, .. } = &mut account {
        registration.redirect_uri = bytes::copy_of(ZERO);
    }
    account
}
fn new_refuses(account: Account) -> Option<Unusable> {
    let mut configured = List::with_capacity(1);
    configured.push(account).ok().expect("one account");
    Component::new(configured, &limits(), crypto(17)).err()
}
fn add_refuses(account: Account) -> Option<Unusable> {
    let mut owner = owner([], &limits());
    owner.component.add(&owner.env, 0, account, &mut owner.above);
    if let Some((_, _, Refusal::Unusable(why))) = refusal_of(owner.above.pop()) { Some(why) } else { None }
}
struct Admission {
    name: &'static str,
    admit: fn(Account) -> Option<Unusable>,
    expected: Option<Unusable>,
}
#[test]
fn a_fixed_redirect_with_port_zero_is_unusable() {
    for row in [
        Admission { name: "new", admit: new_refuses, expected: Some(Unusable::Redirect { account: 0 }) },
        Admission { name: "add", admit: add_refuses, expected: Some(Unusable::Redirect { account: 0 }) },
    ] {
        assert_eq!((row.admit)(fixed_zero()), row.expected, "{}", row.name);
    }
}
#[test]
fn a_redirect_is_matched_against_the_bound_uri() {
    for (authority, uri, expected) in [
        (b"127.0.0.1:41000".as_slice(), b"http://127.0.0.1:41000/callback?state=a&code=b".as_slice(), true),
        (b"127.0.0.1:31234".as_slice(), b"http://127.0.0.1:31234/callback?state=a&code=b".as_slice(), false),
    ] {
        let call = skein_http::server::Call {
            method: skein_http::Method::Get,
            target: bytes::copy_of(b"/callback?state=a&code=b"),
            version: skein_http::Version::Http11,
            body: skein_http::server::Body::None,
            headers: Box::new([skein_http::Header { name: bytes::copy_of(b"host"), value: bytes::copy_of(authority) }]),
        };
        assert_eq!(
            (
                redirect::parse(uri, BOUND, &limits().client).is_some(),
                redirect::from_head(&call, BOUND, &limits().client).is_some()
            ),
            (expected, expected)
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RedirectOutcome {
    Refused,
    Incomplete,
    Sent,
    Unexpected,
}
fn redirected(suffix: &[u8]) -> RedirectOutcome {
    let mut profile = limits();
    profile.client.document.issued_client_bytes = 6;
    let mut account = refresh_account(0, ADDRESS);
    if let Account::SignIn { registration, keeper, .. } = &mut account {
        registration.terms = Some(issued_terms());
        registration.client_secret = Some(bytes::copy_of(b"secret"));
        registration.redirect_uri = bytes::copy_of(b"https://service.example/callback");
        *keeper = Keeper::Owner { kept: None };
    }
    let mut owner = owner([account], &profile);
    owner.ask(Request::SignIn { account: 0 });
    let url = visit(&mut owner);
    let parts = [
        b"https://service.example/callback?state=".as_slice(),
        query(&url, b"state").expect("state"),
        b"&code=code",
        suffix,
    ];
    let mut length = 0_usize;
    for part in parts {
        length = length.checked_add(part.len()).expect("fixture URI length");
    }
    let mut uri = skein_lib::Writer::new(length);
    for part in parts {
        uri.put(part).expect("fixture URI room");
    }
    owner.ask(Request::Redirected { account: 0, uri: uri.finish() });
    fire(&mut owner);
    let event = owner.above.pop();
    if let Some(Event::Refused { asked: Asked::Redirected, why: Refusal::Redirect, .. }) = event {
        RedirectOutcome::Refused
    } else if let Some(Event::Failed {
        ends: Ends::SignIn,
        failure: Failure::Exchange(skein_oauth::Failure::Incomplete),
        ..
    }) = event
    {
        RedirectOutcome::Incomplete
    } else if event.is_none()
        && let Some(Lower::Connect { .. }) = owner.io.pop()
    {
        RedirectOutcome::Sent
    } else {
        RedirectOutcome::Unexpected
    }
}
#[test]
fn a_redirects_client_id_reaches_the_machine() {
    for (name, suffix, expected) in [
        ("present", b"&client_id=id%2B1".as_slice(), RedirectOutcome::Sent),
        ("absent", b"".as_slice(), RedirectOutcome::Incomplete),
        ("repeated", b"&client_id=first&client_id=second".as_slice(), RedirectOutcome::Refused),
        ("at cap", b"&client_id=123456".as_slice(), RedirectOutcome::Sent),
        ("past cap", b"&client_id=1234567".as_slice(), RedirectOutcome::Refused),
    ] {
        assert_eq!(redirected(suffix), expected, "{name}");
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Effect {
    Listen(u16),
    Listening,
    Visit,
}
fn effects(owner: &mut Owner, effects: &mut List<Effect>) {
    while let Some(request) = owner.io.pop() {
        if let Lower::Listen { addr: skein_io::Addr::Ip(address), .. } = request {
            effects.push(Effect::Listen(address.port())).expect("effect room");
        }
    }
    while let Some(event) = owner.above.pop() {
        if let Event::Visit { .. } = event {
            effects.push(Effect::Visit).expect("effect room");
        }
    }
}
#[test]
fn a_public_sign_in_visits_only_after_its_listener_listens() {
    for (name, account, expected) in [
        ("fixed", refresh_account(0, ADDRESS), [Effect::Listen(31234), Effect::Listening, Effect::Visit]),
        (
            "varying",
            varying(refresh_account(0, ADDRESS), PREFERRED),
            [Effect::Listen(31234), Effect::Listening, Effect::Visit],
        ),
    ] {
        let mut owner = owner([account], &limits());
        owner.ask(Request::SignIn { account: 0 });
        let (identity, port) = take_listen(&mut owner);
        let mut seen = List::with_capacity(8);
        seen.push(Effect::Listen(port)).expect("effect room");
        for _ in 0_u32..4 {
            fire(&mut owner);
            effects(&mut owner, &mut seen);
        }
        seen.push(Effect::Listening).expect("effect room");
        listening(&mut owner, identity, 31234);
        for _ in 0_u32..4 {
            fire(&mut owner);
            effects(&mut owner, &mut seen);
        }
        assert_eq!(seen.as_slice(), expected, "{name}");
    }
}
#[test]
fn a_varying_redirect_falls_back_to_port_zero() {
    let mut owner = public(PREFERRED);
    owner.ask(Request::SignIn { account: 0 });
    let identity = listen(&mut owner);
    socket(&mut owner, LowerEvent::Failed { owner: identity, error: skein_io::Error::Other });
    socket(&mut owner, LowerEvent::Closed { owner: identity });
    let mut seen = List::with_capacity(8);
    effects(&mut owner, &mut seen);
    listening(&mut owner, identity, 41000);
    for _ in 0_u32..4 {
        fire(&mut owner);
        effects(&mut owner, &mut seen);
    }
    assert_eq!(seen.as_slice(), [Effect::Listen(0), Effect::Visit]);
}
#[test]
fn a_listener_that_cannot_bind_ends_the_sign_in_before_its_visit() {
    for (name, account, expected) in [
        (
            "fixed",
            refresh_account(0, ADDRESS),
            Failed { account: 0, ends: Ends::SignIn, failure: Failure::Exchange(skein_oauth::Failure::Unavailable) },
        ),
        (
            "zero",
            varying(refresh_account(0, ADDRESS), ZERO),
            Failed { account: 0, ends: Ends::SignIn, failure: Failure::Exchange(skein_oauth::Failure::Unavailable) },
        ),
    ] {
        let mut owner = owner([account], &limits());
        owner.ask(Request::SignIn { account: 0 });
        let identity = listen(&mut owner);
        socket(&mut owner, LowerEvent::Failed { owner: identity, error: skein_io::Error::Other });
        socket(&mut owner, LowerEvent::Closed { owner: identity });
        fire(&mut owner);
        let failure = failure_of(owner.above.pop());
        assert_eq!(failure, Some(expected), "{name}");
    }
}
#[test]
fn two_sign_ins_on_a_varying_redirect_own_two_ports() {
    let mut profile = limits();
    profile.listeners = 2;
    profile.io.sockets = 6;
    let mut owner =
        owner([varying(refresh_account(0, ADDRESS), ZERO), varying(refresh_account(1, ADDRESS), ZERO)], &profile);
    for account in 0..2 {
        owner.ask(Request::SignIn { account });
    }
    let first = listen(&mut owner);
    let second = listen(&mut owner);
    listening(&mut owner, second, 41001);
    listening(&mut owner, first, 41000);
    let mut ports = List::with_capacity(2);
    for _ in 0_u32..4 {
        fire(&mut owner);
        if let Some(Event::Visit { account, url }) = owner.above.pop() {
            ports
                .push((
                    account,
                    query(&url, b"redirect_uri")
                        == Some(if account == 0 {
                            b"http%3A%2F%2F127.0.0.1%3A41000%2Fcallback"
                        } else {
                            b"http%3A%2F%2F127.0.0.1%3A41001%2Fcallback"
                        }),
                ))
                .expect("two ports");
        }
    }
    assert_eq!(ports.as_slice(), [(0, true), (1, true)]);
}
#[derive(Debug, PartialEq, Eq)]
struct Hints {
    issued: bool,
    id_token: bool,
    subject: bool,
}

#[test]
fn signing_in_again_carries_the_held_records_hints() {
    let mut profile = limits();
    profile.client.document.issued_client_bytes = 32;
    profile.client.document.subject_bytes = 32;
    let mut account = refresh_account(0, ADDRESS);
    if let Account::SignIn { registration, keeper, .. } = &mut account {
        registration.terms = Some(issued_terms());
        if let Keeper::Owner { kept: Some(record) } = keeper {
            record.issued_client_id = Some(bytes::copy_of(b"issued"));
            record.id_token = Some(bytes::copy_of(b"token"));
            record.subject = Some(bytes::copy_of(b"person"));
        }
    }
    let mut owner = owner([account], &profile);
    owner.ask(Request::SignIn { account: 0 });
    let identity = listen(&mut owner);
    listening(&mut owner, identity, 31234);
    let url = visit(&mut owner);
    let seen = Hints {
        issued: query(&url, b"client_id") == Some(b"issued"),
        id_token: query(&url, b"id_token_hint") == Some(b"token"),
        subject: query(&url, b"login_hint") == Some(b"person"),
    };
    assert_eq!(seen, Hints { issued: true, id_token: true, subject: true });
}
