use skein_fake_oauth as fake;
use skein_oauth as oauth;
use skein_oauth_world::*;

fn signed_in(issuer: &mut fake::Issuer, registration: oauth::Registration, plan: fake::Plan) -> oauth::SavedToken {
    issuer.queue(plan).expect("plan");
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let request = sign_in_with(&mut client, issuer, registration);
    let answer = response(fake_step(issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    record(client_step(&mut client, oauth::Event::Http(answer)))
}
#[test]
fn the_redirects_issued_client_id_reaches_the_record() {
    let mut issuer = issuer_with(true, Some(issued(&[b"client-1"])));
    let saved = signed_in(&mut issuer, issuing_registration(), success(b"access", Some(b"refresh"), 0));
    assert_eq!(saved.issued_client_id.as_deref(), Some(b"client-1".as_slice()));
    assert!(issuer.carried().is_empty());
}
#[test]
fn signing_in_again_keeps_a_newly_issued_client_id() {
    let mut issuer = issuer_with(true, Some(issued(&[b"client-1", b"client-2"])));
    let saved = signed_in(&mut issuer, issuing_registration(), success(b"access", Some(b"refresh"), 0));
    issuer.queue(success(b"access", Some(b"refresh"), 0)).expect("plan");
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let url = again(&mut client, issuing_registration(), Some(saved), None);
    let redirect = redirected(&mut issuer, url);
    let request = http(client_step(&mut client, redirect));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    let saved = record(client_step(&mut client, oauth::Event::Http(answer)));
    assert_eq!(saved.issued_client_id.as_deref(), Some(b"client-2".as_slice()));
}
fn again(
    client: &mut oauth::Client,
    registration: oauth::Registration,
    prior: Option<oauth::SavedToken>,
    nonce: Option<Box<[u8]>>,
) -> Box<[u8]> {
    let (issued_client_id, id_token, subject) =
        prior.map_or((None, None, None), |prior| (prior.issued_client_id, prior.id_token, prior.subject));
    let request = client_step(
        client,
        oauth::Event::SignIn {
            registration,
            issued_client_id,
            id_token,
            subject,
            nonce,
            port: None,
            key: 7,
            generation: 0,
            state: boxed(b"0123456789abcdef"),
            verifier: Some(boxed(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")),
            now: time(1),
        },
    );
    let Some(oauth::Request::Visit { url }) = request else { panic!("visit") };
    url
}
#[test]
fn an_incomplete_redirect_ends_the_sign_in_with_no_exchange() {
    let mut terms = issued(&[b"client-1"]);
    terms.omit_issued = true;
    let mut issuer = issuer_with(true, Some(terms));
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let url = visit(&mut client, issuing_registration(), None, None);
    let event = redirected(&mut issuer, url);
    assert_eq!(failure(client_step(&mut client, event).as_ref()), oauth::Failure::Incomplete);
    assert_eq!(issuer.posts(), 0);
}
fn parameter_registration() -> oauth::Registration {
    let mut registration = issuing_registration();
    let terms = registration.terms.as_mut().expect("terms");
    terms.parameters = Box::new([
        oauth::Parameter { name: boxed(b"extra"), value: boxed(b""), sent: oauth::Sent::EverySignIn },
        oauth::Parameter { name: boxed(b"name"), value: boxed(b"a"), sent: oauth::Sent::NewRegistration },
    ]);
    registration
}
#[test]
fn each_form_sends_what_it_must_and_nothing_else() {
    for (prior, expected) in [
        (false, [b"extra".as_slice(), b"name"].as_slice()),
        (true, [b"extra".as_slice(), b"id_token_hint", b"login_hint"].as_slice()),
    ] {
        let mut issuer = issuer_with(true, Some(issued(&[b"client-1"])));
        let prior = if prior {
            let mut saved = signed_in(&mut issuer, issuing_registration(), success(b"access", Some(b"refresh"), 0));
            saved.id_token = Some(boxed(b"held.header.signature"));
            saved.subject = Some(boxed(b"subject"));
            Some(saved)
        } else {
            None
        };
        let mut client = oauth::Client::new(client_limits()).expect("client");
        let url = again(&mut client, parameter_registration(), prior, None);
        drop(redirected(&mut issuer, url));
        assert_eq!(issuer.carried().iter().map(AsRef::as_ref).collect::<Vec<&[u8]>>(), expected);
    }
}
#[test]
fn the_resource_reaches_the_issuer_in_every_request() {
    let registration = registration_with_resource();
    let mut issuer = issuer(true);
    issuer.queue(success(b"access", Some(b"refresh"), 0)).expect("plan");
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let url = visit(&mut client, registration, None, None);
    let event = redirected(&mut issuer, url);
    assert_eq!(issuer.carried(), [boxed(b"resource")]);
    let request = http(client_step(&mut client, event));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    assert_eq!(issuer.carried(), [boxed(b"resource")]);
    let saved = record(client_step(&mut client, oauth::Event::Http(answer)));
    drop(client_step(&mut client, oauth::Event::Reset));
    let mut registration = registration_with_resource();
    registration.terms.as_mut().expect("terms").resource = Some(boxed(b"https://service.example"));
    let request = http(client_step(
        &mut client,
        oauth::Event::Refresh { registration, prior: saved.refresh_state().expect("refresh"), now: time(3) },
    ));
    drop(fake_step(&mut issuer, fake::Event::Post { request, now: time(3), wall: wall(101) }));
    assert_eq!(issuer.carried(), [boxed(b"resource")]);
}
fn registration_with_resource() -> oauth::Registration {
    let mut registration = registration(true);
    let mut additions = terms();
    additions.resource = Some(boxed(b"https://service.example"));
    registration.terms = Some(additions);
    registration
}
#[test]
fn a_varying_redirect_signs_in_on_its_bound_port() {
    const PORT: u16 = 5678; // Bound listener port for this sign-in.
    let mut fake_terms = issued(&[]);
    fake_terms.varying_port = true;
    let mut issuer = issuer_with(true, Some(fake_terms));
    issuer.queue(success(b"access", Some(b"refresh"), 0)).expect("plan");
    let mut registration = registration(true);
    let mut additions = terms();
    additions.varying_port = true;
    registration.terms = Some(additions);
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let url = visit(&mut client, registration, None, Some(PORT));
    let event = redirected(&mut issuer, url);
    let request = http(client_step(&mut client, event));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    assert_eq!(record(client_step(&mut client, oauth::Event::Http(answer))).key, 7);
}
#[test]
fn fewer_scopes_granted_than_required_are_named() {
    let mut registration = registration(true);
    let mut additions = terms();
    additions.required_scopes = Some(boxed(b"read profile"));
    registration.terms = Some(additions);
    let mut plan = success(b"access", Some(b"refresh"), 0);
    let fake::Body::Token(body) = &mut plan.body else { panic!("token") };
    body.scope = Some(boxed(b"read"));
    let (mut client, mut issuer) = planned(&registration, plan);
    let request = sign_in_with(&mut client, &mut issuer, registration);
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    let Some(oauth::Request::Tokens { missing, .. }) = client_step(&mut client, oauth::Event::Http(answer)) else {
        panic!("tokens")
    };
    assert_eq!(missing.as_ref(), b"profile");
}
#[test]
fn an_added_error_name_on_an_exchange_signs_in_again() {
    for status in [400, 401, 500] {
        let mut registration = registration(true);
        let mut additions = terms();
        additions.sign_in_again = Box::new([boxed(b"gone")]);
        registration.terms = Some(additions);
        let (mut client, mut issuer) = planned(&registration, error(status, b"gone", 0));
        let request = sign_in_with(&mut client, &mut issuer, registration);
        let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
        assert_eq!(failure(client_step(&mut client, oauth::Event::Http(answer)).as_ref()), oauth::Failure::Refused);
    }
}
#[test]
fn an_earliest_refresh_time_reaches_the_record() {
    let mut plan = success(b"access", Some(b"refresh"), 0);
    let fake::Body::Token(body) = &mut plan.body else { panic!("token") };
    body.earliest_refresh_at = Some(wall(120));
    let saved = signed_in(&mut issuer(true), registration(true), plan);
    assert_eq!(saved.earliest_refresh_at, Some(wall(120)));
}
fn revocation(plan: fake::Plan, lose: bool) -> Option<oauth::Request> {
    let mut issuer = issuer_with(true, Some(issued(&[b"client-1"])));
    let saved = signed_in(&mut issuer, issuing_registration(), success(b"access", Some(b"refresh"), 0));
    issuer.queue(plan).expect("plan");
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let request = http(client_step(
        &mut client,
        oauth::Event::Revoke {
            registration: issuing_registration(),
            prior: saved.refresh_state().expect("refresh"),
            now: time(3),
        },
    ));
    let answer = fake_step(&mut issuer, fake::Event::Post { request, now: time(3), wall: wall(101) });
    if lose {
        drop(fake_step(&mut issuer, fake::Event::LoseResponse));
        client_step(&mut client, oauth::Event::Tick { now: time(13) })
    } else {
        client_step(&mut client, oauth::Event::Http(response(answer)))
    }
}
#[test]
fn a_revocation_the_issuer_confirms_is_confirmed() {
    let result = revocation(success(b"unused", None, 0), false);
    assert!(matches!(result, Some(oauth::Request::Revoked { confirmed: true })));
}
#[test]
fn a_revocation_the_issuer_refuses_or_loses_is_unconfirmed() {
    for (plan, lose) in [(error(400, b"refused", 0), false), (success(b"unused", None, 20), true)] {
        assert!(matches!(revocation(plan, lose), Some(oauth::Request::Revoked { confirmed: false })));
    }
}
