//! Decoded redirect admission through the client's exported API, oauth.md, section 3.

use skein_fake_oauth as fake;
use skein_lib::{Duration, Time};
use skein_oauth::{self as oauth, Inadmissible};
use skein_oauth_world::{self as world, boxed, client_step, fake_step, time, wall};

const STATE: &[u8] = b"0123456789abcdef";
const LAST_NANOSECOND: Time = Time::from_nanos(120_999_999_999);

fn decoded(event: oauth::Event) -> oauth::Redirect {
    let oauth::Event::Redirected { uri, state, code, error, client_id, .. } = event else {
        panic!("the fake authorization must return a redirect");
    };
    oauth::Redirect { uri, state, code, error, client_id }
}

fn waiting_with(
    registration: oauth::Registration,
    mut issuer: fake::Issuer,
) -> (oauth::Client, fake::Issuer, oauth::Redirect) {
    let mut client = oauth::Client::new(world::client_limits()).expect("valid world client limits");
    let url = world::visit(&mut client, registration, None, None);
    let redirect = decoded(world::redirected(&mut issuer, url));
    (client, issuer, redirect)
}

fn waiting() -> (oauth::Client, fake::Issuer, oauth::Redirect) {
    waiting_with(world::registration(true), world::issuer(true))
}

fn submit(client: &mut oauth::Client, redirect: &oauth::Redirect, now: Time) -> Option<oauth::Request> {
    client_step(
        client,
        oauth::Event::Redirected {
            uri: redirect.uri.clone(),
            state: redirect.state.clone(),
            code: redirect.code.clone(),
            error: redirect.error.clone(),
            client_id: redirect.client_id.clone(),
            now,
        },
    )
}

fn answer(issuer: &mut fake::Issuer, request: oauth::HttpRequest) -> oauth::HttpResponse {
    world::response(fake_step(issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }))
}

#[test]
fn redirect_addresses_require_the_exact_registered_spelling() {
    let (client, _, mut redirect) = waiting();
    for address in [
        b"https://127.0.0.1:1234/callback".as_slice(),
        b"http://localhost:1234/callback",
        b"http://127.0.0.1:1235/callback",
        b"http://127.0.0.1:1234/Callback",
        b"http://127.0.0.1:1234/callback/",
        b"http://127.0.0.1:1234/%63allback",
        b"http://127.0.0.1:1234/callback?ignored=1",
        b"http://127.0.0.1:1234/callback#fragment",
    ] {
        redirect.uri = boxed(address);
        assert_eq!(client.admits(&redirect, time(2)), Err(Inadmissible::Address));
    }
    redirect.uri = world::registration(true).redirect_uri;
    assert_eq!(client.admits(&redirect, time(2)), Ok(()));
}

#[test]
fn state_mismatches_at_either_end_leave_admission_available() {
    let (client, _, mut redirect) = waiting();
    for state in [b"X123456789abcdef".as_slice(), b"0123456789abcdeX", b"0123456789abcde", b"0123456789abcdefX", b""] {
        redirect.state = boxed(state);
        assert_eq!(client.admits(&redirect, time(2)), Err(Inadmissible::State));
    }
    redirect.state = boxed(STATE);
    assert_eq!(client.admits(&redirect, time(2)), Ok(()));
}

#[test]
fn empty_code_and_error_fields_count_as_absent() {
    let (client, _, mut redirect) = waiting();
    let rows = [
        (None, None, Err(Inadmissible::Address)),
        (Some(b"".as_slice()), None, Err(Inadmissible::Address)),
        (None, Some(b"".as_slice()), Err(Inadmissible::Address)),
        (Some(b"code".as_slice()), Some(b"access_denied".as_slice()), Err(Inadmissible::Address)),
        (Some(b"code".as_slice()), Some(b"".as_slice()), Ok(())),
        (Some(b"".as_slice()), Some(b"access_denied".as_slice()), Ok(())),
    ];
    for (code, error, expected) in rows {
        redirect.code = code.map(boxed);
        redirect.error = error.map(boxed);
        assert_eq!(client.admits(&redirect, time(2)), expected);
    }
}

#[test]
fn missing_issued_client_ids_are_refused_without_spending_the_attempt() {
    let registration = world::issuing_registration();
    let issuer = world::configured_issuer(&registration, Some(world::issued(&[b"issued-for-this-visit"])));
    let (mut client, mut issuer, mut redirect) = waiting_with(registration, issuer);
    let client_id = redirect.client_id.take();
    for missing in [None, Some(boxed(b""))] {
        redirect.client_id = missing;
        assert_eq!(client.admits(&redirect, time(2)), Err(Inadmissible::Address));
    }
    redirect.client_id = client_id;
    issuer.queue(world::success(b"access", Some(b"refresh"), 0)).unwrap();
    assert_eq!(client.admits(&redirect, time(2)), Ok(()));
    let post = world::http(submit(&mut client, &redirect, time(2)));
    let record = world::record(client_step(&mut client, oauth::Event::Http(answer(&mut issuer, post))));
    assert_eq!(record.issued_client_id.as_deref(), Some(b"issued-for-this-visit".as_slice()));
    assert_eq!(issuer.posts(), 1);
}

#[test]
fn an_admitted_provider_denial_ends_without_a_code_exchange() {
    let (mut client, issuer, mut redirect) = waiting();
    redirect.code = None;
    redirect.error = Some(boxed(b"access_denied"));
    assert_eq!(client.admits(&redirect, time(2)), Ok(()));
    let terminal = submit(&mut client, &redirect, time(2));
    assert_eq!(world::failure(terminal.as_ref()), oauth::Failure::Refused);
    assert_eq!(client.admits(&redirect, time(2)), Err(Inadmissible::NotWaiting));
    assert_eq!(issuer.posts(), 0);
}

#[test]
fn the_original_deadline_refuses_at_equality_and_afterward() {
    let (client, _, redirect) = waiting();
    for (now, expected) in [
        (LAST_NANOSECOND, Ok(())),
        (time(121), Err(Inadmissible::NotWaiting)),
        (time(122), Err(Inadmissible::NotWaiting)),
    ] {
        assert_eq!(client.admits(&redirect, now), expected);
    }
}

#[test]
fn admission_checks_preserve_the_original_timer_and_check_order() {
    let (client, _, mut redirect) = waiting();
    redirect.uri = boxed(b"http://localhost:1234/callback");
    redirect.state = boxed(b"another-state");
    assert_eq!(client.admits(&redirect, time(120)), Err(Inadmissible::Address));
    assert_eq!(client.admits(&redirect, time(121)), Err(Inadmissible::NotWaiting));
    redirect.uri = world::registration(true).redirect_uri;
    assert_eq!(client.admits(&redirect, time(120)), Err(Inadmissible::State));
    redirect.state = boxed(STATE);
    assert_eq!(client.admits(&redirect, time(120)), Ok(()));
    assert_eq!(client.next_deadline(), Some(time(121)));
    assert!(!client.is_done());
}

#[test]
fn taking_a_late_redirect_does_not_extend_the_exchange_deadline() {
    let (mut client, _, redirect) = waiting();
    let post = world::http(submit(&mut client, &redirect, time(120)));
    assert_eq!(post.deadline, time(121));
    assert_eq!(client.next_deadline(), Some(time(121)));
    let terminal = client_step(&mut client, oauth::Event::Tick { now: time(121) });
    assert!(terminal.is_some());
    assert!(client.is_done());
}

#[test]
fn duplicate_redirects_make_only_one_fake_issuer_post() {
    let (mut client, mut issuer, redirect) = waiting();
    issuer.queue(world::success(b"access", Some(b"refresh"), 0)).unwrap();
    let post = world::http(submit(&mut client, &redirect, time(2)));
    assert_eq!(client.admits(&redirect, time(2)), Err(Inadmissible::NotWaiting));
    assert!(submit(&mut client, &redirect, time(2)).is_none());
    let terminal = client_step(&mut client, oauth::Event::Http(answer(&mut issuer, post)));
    let record = world::record(terminal);
    assert_eq!(record.access_token.as_ref(), b"access");
    assert!(submit(&mut client, &redirect, time(3)).is_none());
    assert_eq!(issuer.posts(), 1);
}

#[test]
fn a_cancelled_attempt_needs_a_reset_and_a_fresh_state() {
    let (mut client, mut issuer, old_redirect) = waiting();
    let terminal = client_step(&mut client, oauth::Event::Cancel);
    assert_eq!(world::failure(terminal.as_ref()), oauth::Failure::Cancelled);
    assert_eq!(client.admits(&old_redirect, time(2)), Err(Inadmissible::NotWaiting));
    assert!(submit(&mut client, &old_redirect, time(2)).is_none());
    let _ = client_step(&mut client, oauth::Event::Reset);
    let visit = client_step(
        &mut client,
        oauth::Event::SignIn {
            registration: world::registration(true),
            key: 7,
            generation: 0,
            state: boxed(b"fedcba9876543210"),
            verifier: Some(boxed(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")),
            nonce: None,
            port: None,
            issued_client_id: None,
            id_token: None,
            subject: None,
            now: time(3),
        },
    );
    let Some(oauth::Request::Visit { url }) = visit else { panic!("fresh visit expected") };
    let fresh = decoded(world::redirected(&mut issuer, url));
    assert_eq!(client.admits(&old_redirect, time(4)), Err(Inadmissible::State));
    assert_eq!(client.admits(&fresh, time(4)), Ok(()));
}

#[test]
fn redirects_are_not_admitted_before_openid_keys_allow_the_visit() {
    let mut client = oauth::Client::new(world::client_limits()).unwrap();
    let _keys = world::http(world::begin(&mut client, world::openid_registration(), 1));
    let redirect = oauth::Redirect {
        uri: world::registration(true).redirect_uri,
        state: boxed(STATE),
        code: Some(boxed(b"not-issued")),
        error: None,
        client_id: None,
    };
    assert_eq!(client.admits(&redirect, time(2)), Err(Inadmissible::NotWaiting));
    assert_eq!(client.next_deadline(), Some(time(11)));
}

#[test]
fn admitted_redirects_still_fail_invalid_id_token_checks() {
    for (fault, check) in [
        (fake::Fault::Nonce, oauth::Check::Nonce),
        (fake::Fault::Audience, oauth::Check::Audience),
        (fake::Fault::Issuer, oauth::Check::Issuer),
        (fake::Fault::NoSubject, oauth::Check::Missing),
        (fake::Fault::NoIdToken, oauth::Check::Missing),
    ] {
        let mut client = oauth::Client::new(world::client_limits()).unwrap();
        let mut issuer = world::openid_issuer(Some(fault));
        issuer.queue(world::success(b"access", Some(b"refresh"), 0)).unwrap();
        let url = world::openid_visit(&mut client, &mut issuer, world::openid_registration());
        let redirect = decoded(world::redirected(&mut issuer, url));
        assert_eq!(client.admits(&redirect, time(2)), Ok(()));
        let post = world::http(submit(&mut client, &redirect, time(2)));
        let terminal = client_step(&mut client, oauth::Event::Http(answer(&mut issuer, post)));
        assert_eq!(world::failure(terminal.as_ref()), oauth::Failure::Unverified { check });
        assert_eq!(issuer.posts(), 1);
        assert_eq!(client.admits(&redirect, time(3)), Err(Inadmissible::NotWaiting));
    }
}

#[test]
fn an_ambiguous_code_exchange_is_never_retried() {
    let (mut client, _, redirect) = waiting();
    let post = world::http(submit(&mut client, &redirect, time(2)));
    let outcome = oauth::HttpResponse {
        id: post.id,
        status: 0,
        body: boxed(b""),
        retry_after: Duration::ZERO,
        evidence: oauth::HttpEvidence::Unknown,
        now: time(3),
        wall: wall(100),
    };
    let terminal = client_step(&mut client, oauth::Event::Http(outcome));
    assert!(matches!(terminal, Some(oauth::Request::Failed { .. })));
    assert!(client.is_done());
    assert_eq!(client.next_deadline(), None);
    assert!(client_step(&mut client, oauth::Event::Tick { now: time(12) }).is_none());
    assert_eq!(client.admits(&redirect, time(12)), Err(Inadmissible::NotWaiting));
}
