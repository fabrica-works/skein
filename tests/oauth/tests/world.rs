use skein_fake_oauth as fake;
use skein_lib::{Duration, bytes};
use skein_oauth as oauth;
use skein_oauth_world::{
    boxed, client_limits, client_step, documents, error, failure, fake_limits, fake_step, http, issuer, issuer_for,
    planned, record, redirected, registration, response, sign_in, success, terms, time, visit, wall,
};

#[test]
fn a_localhost_registration_signs_in_through_its_exact_redirect() {
    let redirect_uri = b"http://localhost:1234/callback";
    let mut registration = registration(true);
    registration.redirect_uri = boxed(redirect_uri);
    let mut issuer = fake::Issuer::new(
        fake::Config {
            authorization_url: registration.authorization_url.clone(),
            token_endpoint: registration.token_endpoint.clone(),
            client_id: registration.client_id.clone(),
            client_secret: None,
            redirect_uri: boxed(redirect_uri),
            refresh_token: boxed(b"seed"),
            terms: None,
        },
        fake_limits(),
    )
    .expect("issuer");
    issuer.queue(success(b"access", Some(b"refresh"), 0)).expect("plan");
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let Some(oauth::Request::Visit { url }) = client_step(
        &mut client,
        oauth::Event::SignIn {
            issued_client_id: None,
            id_token: None,
            subject: None,
            nonce: None,
            port: None,
            registration,
            key: 7,
            generation: 0,
            state: boxed(b"0123456789abcdef"),
            verifier: Some(boxed(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")),
            now: time(1),
        },
    ) else {
        panic!("visit expected")
    };
    let Some(fake::Request::Redirect { uri, state, code, client_id }) =
        fake_step(&mut issuer, fake::Event::Authorize { url, now: time(1) })
    else {
        panic!("redirect expected")
    };
    assert_eq!(uri.as_ref(), redirect_uri);
    let request = http(client_step(
        &mut client,
        oauth::Event::Redirected { client_id, uri, state, code: Some(code), error: None, now: time(2) },
    ));
    assert!(bytes::find(&request.body, b"redirect_uri=http%3A%2F%2Flocalhost%3A1234%2Fcallback").is_some());
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    let saved = record(client_step(&mut client, oauth::Event::Http(answer)));
    assert_eq!(saved.key, 7);
    assert_eq!(saved.generation, 1);
    assert!(client.is_done());
}

#[test]
fn public_sign_in_refresh_rotation_and_old_refresh_replay() {
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let mut issuer = issuer(true);
    issuer.queue(success(b"access-1", Some(b"refresh-1"), 0)).expect("plan");
    let request = sign_in(&mut client, &mut issuer, true);
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    let first = record(client_step(&mut client, oauth::Event::Http(answer)));
    assert_eq!(first.generation, 1);
    assert_eq!(first.refresh_token.as_deref().expect("refresh token"), b"refresh-1");
    assert_eq!(issuer.generation(), 1);

    issuer.queue(success(b"access-2", Some(b"refresh-2"), 0)).expect("plan");
    assert!(client_step(&mut client, oauth::Event::Reset).is_none());
    let request = http(client_step(
        &mut client,
        oauth::Event::Refresh {
            registration: registration(true),
            prior: first.refresh_state().expect("record has refresh state"),
            now: time(3),
        },
    ));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(3), wall: wall(101) }));
    let second = record(client_step(&mut client, oauth::Event::Http(answer)));
    assert_eq!(second.generation, 2);
    assert_eq!(second.refresh_token.as_deref().expect("refresh token"), b"refresh-2");
    assert_eq!(issuer.generation(), 2);
    assert_eq!(second.remaining(wall(126)), Duration::from_secs(5));

    assert!(client_step(&mut client, oauth::Event::Reset).is_none());
    let replay = http(client_step(
        &mut client,
        oauth::Event::Refresh {
            registration: registration(true),
            prior: first.refresh_state().expect("record has refresh state"),
            now: time(4),
        },
    ));
    let refusal =
        response(fake_step(&mut issuer, fake::Event::Post { request: replay, now: time(4), wall: wall(102) }));
    assert_eq!(failure(client_step(&mut client, oauth::Event::Http(refusal)).as_ref()), oauth::Failure::Refused);
}

#[test]
fn confidential_sign_in_and_stable_refresh_use_the_same_machine() {
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let mut issuer = issuer(false);
    issuer.queue(success(b"access-1", Some(b"refresh-1"), 0)).expect("plan");
    let request = sign_in(&mut client, &mut issuer, false);
    assert!(bytes::find(&request.body, b"\"client_secret\":\"secret\"").is_some());
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    let first = record(client_step(&mut client, oauth::Event::Http(answer)));
    issuer.queue(success(b"access-2", None, 0)).expect("plan");
    assert!(client_step(&mut client, oauth::Event::Reset).is_none());
    let request = http(client_step(
        &mut client,
        oauth::Event::Refresh {
            registration: registration(false),
            prior: first.refresh_state().expect("record has refresh state"),
            now: time(3),
        },
    ));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(3), wall: wall(101) }));
    let second = record(client_step(&mut client, oauth::Event::Http(answer)));
    assert_eq!(second.refresh_token, first.refresh_token);
    assert_eq!(issuer.generation(), 1, "omission preserves the issuer's refresh token");
}

#[test]
fn issuer_revocation_error_and_malformed_document_are_typed() {
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let mut issuer = issuer(true);
    issuer.queue(error(400, b"invalid_grant", 0)).expect("plan");
    let request = http(client_step(
        &mut client,
        oauth::Event::Refresh {
            registration: registration(true),
            prior: oauth::RefreshState {
                key: 1,
                generation: 0,
                refresh_token: boxed(b"seed"),
                issued_client_id: None,
                granted_scopes: None,
                subject: None,
                id_token: None,
            },
            now: time(1),
        },
    ));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    assert_eq!(failure(client_step(&mut client, oauth::Event::Http(answer)).as_ref()), oauth::Failure::Refused);

    issuer
        .queue(fake::Plan {
            status: 200,
            body: fake::Body::Raw(boxed(b"{not-json")),
            delay: Duration::ZERO,
            retry_after: Duration::ZERO,
        })
        .expect("plan");
    assert!(client_step(&mut client, oauth::Event::Reset).is_none());
    let request = http(client_step(
        &mut client,
        oauth::Event::Refresh {
            registration: registration(true),
            prior: oauth::RefreshState {
                key: 1,
                generation: 0,
                refresh_token: boxed(b"seed"),
                issued_client_id: None,
                granted_scopes: None,
                subject: None,
                id_token: None,
            },
            now: time(3),
        },
    ));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(3), wall: wall(101) }));
    assert_eq!(failure(client_step(&mut client, oauth::Event::Http(answer)).as_ref()), oauth::Failure::Malformed);
}

#[test]
fn delayed_rotated_answer_lost_before_delivery_makes_old_grant_revoked() {
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let mut issuer = issuer(true);
    issuer.queue(success(b"access", Some(b"new"), 20)).expect("plan");
    let request = http(client_step(
        &mut client,
        oauth::Event::Refresh {
            registration: registration(true),
            prior: oauth::RefreshState {
                key: 1,
                generation: 0,
                refresh_token: boxed(b"seed"),
                issued_client_id: None,
                granted_scopes: None,
                subject: None,
                id_token: None,
            },
            now: time(1),
        },
    ));
    assert!(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }).is_none());
    assert_eq!(issuer.generation(), 1, "rotation precedes response delivery");
    assert_eq!(
        failure(client_step(&mut client, oauth::Event::Tick { now: time(11) }).as_ref()),
        oauth::Failure::TimedOut
    );
    assert!(fake_step(&mut issuer, fake::Event::LoseResponse).is_none());
    assert!(client_step(&mut client, oauth::Event::Reset).is_none());
    let retry = http(client_step(
        &mut client,
        oauth::Event::Refresh {
            registration: registration(true),
            prior: oauth::RefreshState {
                key: 1,
                generation: 0,
                refresh_token: boxed(b"seed"),
                issued_client_id: None,
                granted_scopes: None,
                subject: None,
                id_token: None,
            },
            now: time(12),
        },
    ));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request: retry, now: time(12), wall: wall(110) }));
    assert_eq!(failure(client_step(&mut client, oauth::Event::Http(answer)).as_ref()), oauth::Failure::Refused);
}

#[test]
fn issuer_rate_limit_then_client_backoff_reaches_success() {
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let mut issuer = issuer(true);
    issuer.queue(error(429, b"slow_down", 2)).expect("rate limit");
    issuer.queue(success(b"access", Some(b"next"), 0)).expect("success");
    let request = http(client_step(
        &mut client,
        oauth::Event::Refresh {
            registration: registration(true),
            prior: oauth::RefreshState {
                key: 1,
                generation: 0,
                refresh_token: boxed(b"seed"),
                issued_client_id: None,
                granted_scopes: None,
                subject: None,
                id_token: None,
            },
            now: time(1),
        },
    ));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    assert!(client_step(&mut client, oauth::Event::Http(answer)).is_none());
    assert_eq!(client.next_deadline(), Some(time(4)));
    let retry = http(client_step(&mut client, oauth::Event::Tick { now: time(4) }));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request: retry, now: time(4), wall: wall(102) }));
    let record = record(client_step(&mut client, oauth::Event::Http(answer)));
    assert_eq!(record.refresh_token.as_deref().expect("refresh token"), b"next");
}

#[test]
fn authorization_code_cannot_be_redeemed_twice() {
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let mut issuer = issuer(true);
    issuer.queue(success(b"access", Some(b"refresh"), 0)).expect("plan");
    let request = sign_in(&mut client, &mut issuer, true);
    let replay = oauth::HttpRequest {
        method: oauth::Method::Post,
        id: request.id.saturating_add(1),
        endpoint: request.endpoint.clone(),
        content_type: request.content_type,
        body: request.body.clone(),
        deadline: request.deadline,
    };
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    let _record = record(client_step(&mut client, oauth::Event::Http(answer)));
    let refusal =
        response(fake_step(&mut issuer, fake::Event::Post { request: replay, now: time(3), wall: wall(101) }));
    assert_eq!(refusal.status, 400);
    assert!(bytes::find(&refusal.body, b"invalid_grant").is_some());
}

const VERSION_TWO: &[u8] = b"SKOT\x00\x02\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x03\x00\x00\x00\x01a\x00\x00\x00\x04seed\x00\x00\x00\x00";
fn refreshed(prior: &oauth::SavedToken, plan: fake::Plan) -> oauth::SavedToken {
    let registration = registration(true);
    let (mut client, mut issuer) = planned(&registration, plan);
    let request = http(client_step(
        &mut client,
        oauth::Event::Refresh { registration, prior: prior.refresh_state().expect("refreshable"), now: time(1) },
    ));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(1), wall: wall(1) }));
    record(client_step(&mut client, oauth::Event::Http(answer)))
}
#[test]
fn a_refresh_naming_no_scope_keeps_the_granted_scopes() {
    let mut prior = oauth::decode_record(VERSION_TWO, &documents()).expect("version two");
    prior.granted_scopes = Some(boxed(b"read"));
    let expected = prior.granted_scopes.clone();
    let record = refreshed(&prior, success(b"access", Some(b"next"), 0));
    assert!(record.granted_scopes == expected);
}
#[test]
fn a_refresh_keeps_the_subject_and_id_token() {
    let mut prior = oauth::decode_record(VERSION_TWO, &documents()).expect("version two");
    prior.subject = Some(boxed(b"subject"));
    prior.id_token = Some(boxed(b"held.header.signature"));
    let expected = (prior.subject.clone(), prior.id_token.clone());
    let mut plan = success(b"access", Some(b"next"), 0);
    let fake::Body::Token(answer) = &mut plan.body else { panic!("token plan") };
    answer.id_token = Some(boxed(b"replacement.header.signature"));
    let record = refreshed(&prior, plan);
    assert!((record.subject, record.id_token) == expected);
}
#[test]
fn a_version_two_record_refreshes_into_version_three() {
    let prior = oauth::decode_record(VERSION_TWO, &documents()).expect("version two");
    let record = refreshed(&prior, success(b"access", Some(b"next"), 0));
    let saved = oauth::encode_record(&record, &documents()).expect("saved");
    assert_eq!(saved.get(..6), Some(b"SKOT\x00\x03".as_slice()));
    let restored = oauth::decode_record(&saved, &documents()).expect("version three");
    assert!(restored.refresh_token == Some(boxed(b"next")));
    assert_eq!(restored.generation, 3);
}

#[test]
fn a_varying_redirect_refuses_another_ports_redirect() {
    const BOUND_PORT: u16 = 40001; // The listener bound for this sign-in.
    let mut registration = registration(true);
    registration.redirect_uri = boxed(b"http://127.0.0.1:40001/callback");
    let mut additions = terms();
    additions.varying_port = true;
    registration.terms = Some(additions);
    let mut issuer = issuer_for(&registration);
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let url = visit(&mut client, registration, None, Some(BOUND_PORT));
    let oauth::Event::Redirected { state, code, error, now, .. } = redirected(&mut issuer, url) else {
        panic!("redirect")
    };
    let result = client_step(
        &mut client,
        oauth::Event::Redirected {
            client_id: None,
            uri: boxed(b"http://127.0.0.1:40002/callback"),
            state,
            code,
            error,
            now,
        },
    );
    assert_eq!(failure(result.as_ref()), oauth::Failure::InvalidRedirect);
}

fn resource_registration(public: bool) -> oauth::Registration {
    let mut registration = registration(public);
    let mut additions = terms();
    additions.resource = Some(boxed(b"https://service.example"));
    registration.terms = Some(additions);
    registration
}

#[test]
fn the_resource_goes_in_every_request() {
    for (public, field) in [
        (true, b"resource=https%3A%2F%2Fservice.example".as_slice()),
        (false, br#""resource":"https://service.example""#.as_slice()),
    ] {
        let registration = resource_registration(public);
        let mut issuer = issuer_for(&registration);
        issuer.queue(success(b"access", Some(b"refresh"), 0)).expect("plan");
        let mut client = oauth::Client::new(client_limits()).expect("client");
        let url = visit(&mut client, registration, None, None);
        assert!(bytes::find(&url, b"&resource=https%3A%2F%2Fservice.example").is_some(), "authorization {public}");
        let event = redirected(&mut issuer, url);
        let request = http(client_step(&mut client, event));
        assert!(bytes::find(&request.body, field).is_some(), "exchange {public}");
        let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
        let saved = record(client_step(&mut client, oauth::Event::Http(answer)));
        drop(client_step(&mut client, oauth::Event::Reset));
        let request = http(client_step(
            &mut client,
            oauth::Event::Refresh {
                registration: resource_registration(public),
                prior: saved.refresh_state().expect("refresh"),
                now: time(3),
            },
        ));
        assert!(bytes::find(&request.body, field).is_some(), "refresh {public}");
    }
}

#[test]
fn an_openid_sign_in_fails_before_its_check_exists() {
    let mut issuer = skein_oauth_world::openid_issuer(None);
    issuer.queue(success(b"access", Some(b"refresh"), 0)).expect("plan");
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let get = http(skein_oauth_world::begin(&mut client, skein_oauth_world::openid_registration(), 1));
    let answer = response(fake_step(
        &mut issuer,
        fake::Event::Get { id: get.id, endpoint: get.endpoint, now: time(1), wall: wall(100) },
    ));
    let Some(oauth::Request::Visit { url }) = client_step(&mut client, oauth::Event::Http(answer)) else {
        panic!("visit")
    };
    let event = redirected(&mut issuer, url);
    let request = http(client_step(&mut client, event));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    let result = client_step(&mut client, oauth::Event::Http(answer));
    assert_eq!(failure(result.as_ref()), oauth::Failure::Malformed);
}

#[test]
fn an_added_error_name_on_a_refresh_signs_in_again() {
    let mut registration = registration(true);
    let mut additions = terms();
    additions.sign_in_again = Box::new([boxed(b"refresh_gone")]);
    registration.terms = Some(additions);
    let (mut client, mut issuer) = planned(&registration, error(500, b"refresh_gone", 0));
    let prior = oauth::decode_record(VERSION_TWO, &documents()).expect("version two");
    let request = http(client_step(
        &mut client,
        oauth::Event::Refresh { registration, prior: prior.refresh_state().expect("refreshable"), now: time(1) },
    ));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(1), wall: wall(1) }));
    assert_eq!(failure(client_step(&mut client, oauth::Event::Http(answer)).as_ref()), oauth::Failure::Refused);
}

#[test]
fn tokens_name_the_required_scopes_the_issuer_withheld() {
    let mut registration = registration(true);
    let mut additions = terms();
    additions.required_scopes = Some(boxed(b"read profile"));
    registration.terms = Some(additions);
    let mut plan = success(b"access", Some(b"refresh"), 0);
    let fake::Body::Token(answer) = &mut plan.body else { panic!("token plan") };
    answer.scope = Some(boxed(b"read"));
    let (mut client, mut issuer) = planned(&registration, plan);
    let url = visit(&mut client, registration, None, None);
    let event = redirected(&mut issuer, url);
    let request = http(client_step(&mut client, event));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    let Some(oauth::Request::Tokens { record, missing }) = client_step(&mut client, oauth::Event::Http(answer)) else {
        panic!("tokens")
    };
    assert_eq!(missing.as_ref(), b"profile");
    assert_eq!(record.key, 7);
}
