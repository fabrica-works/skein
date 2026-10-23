use skein_fake_oauth as fake;
use skein_lib::Duration;
use skein_oauth as oauth;
use skein_oauth_world::{
    begin, boxed, client_limits, client_step, documents, failure, fake_step, http, openid_issuer, openid_issuer_with,
    openid_post, openid_registration, openid_visit, record, redirected, response, success, time, wall,
};

#[test]
fn the_keys_are_fetched_again_at_each_sign_in() {
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let mut issuer = openid_issuer(None);
    for round in 1..=2 {
        let get = http(begin(&mut client, openid_registration(), round));
        assert_eq!(get.method, oauth::Method::Get);
        assert!(get.body.is_empty() && get.content_type.is_empty());
        let event = fake::Event::Get { id: get.id, endpoint: get.endpoint, now: time(round), wall: wall(100) };
        let answer = response(fake_step(&mut issuer, event));
        let visit = client_step(&mut client, oauth::Event::Http(answer));
        assert!(matches!(visit, Some(oauth::Request::Visit { .. })));
        assert!(client_step(&mut client, oauth::Event::Cancel).is_some());
        let _ = client_step(&mut client, oauth::Event::Reset);
    }
}

#[test]
fn each_spoiled_document_fails_before_the_visit() {
    for fault in [
        fake::Fault::UnreadableMetadata,
        fake::Fault::UnreadableKeySet,
        fake::Fault::MetadataIssuer,
        fake::Fault::LongModulus,
        fake::Fault::UseEnc,
    ] {
        let mut client = oauth::Client::new(client_limits()).expect("client");
        let mut issuer = openid_issuer(Some(fault));
        let mut registration = openid_registration();
        registration.terms.as_mut().expect("terms").openid.as_mut().expect("openid").key_set = None;
        let metadata = http(begin(&mut client, registration, 1));
        let answer = response(fake_step(
            &mut issuer,
            fake::Event::Get { id: metadata.id, endpoint: metadata.endpoint, now: time(1), wall: wall(100) },
        ));
        let first = client_step(&mut client, oauth::Event::Http(answer));
        let terminal = match first {
            Some(oauth::Request::Http(keys)) => {
                let answer = response(fake_step(
                    &mut issuer,
                    fake::Event::Get { id: keys.id, endpoint: keys.endpoint, now: time(1), wall: wall(100) },
                ));
                client_step(&mut client, oauth::Event::Http(answer))
            }
            other => other,
        };
        assert_eq!(failure(terminal.as_ref()), oauth::Failure::Unverified { check: oauth::Check::Keys }, "{fault:?}");
    }
}

fn exchange(
    client: &mut oauth::Client,
    issuer: &mut fake::Issuer,
    registration: oauth::Registration,
) -> oauth::HttpResponse {
    issuer.queue(success(b"access", Some(b"refresh"), 0)).expect("plan");
    let request = openid_post(client, issuer, registration);
    response(fake_step(issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }))
}
fn finished(
    client: &mut oauth::Client,
    issuer: &mut fake::Issuer,
    registration: oauth::Registration,
) -> Option<oauth::Request> {
    let answer = exchange(client, issuer, registration);
    client_step(client, oauth::Event::Http(answer))
}
#[test]
fn an_openid_sign_in_keeps_the_subject_and_the_id_token() {
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let mut issuer = openid_issuer(None);
    let answer = exchange(&mut client, &mut issuer, openid_registration());
    let json = oauth::Json::from_bytes(&answer.body, &documents()).expect("answer");
    let token = oauth::decode_response(&json, &documents()).expect("token answer").id_token;
    let record = record(client_step(&mut client, oauth::Event::Http(answer)));
    assert!(record.subject.as_deref() == Some(b"user-42"));
    assert!(record.id_token.is_some() && record.id_token == token);
}
#[test]
fn each_spoiled_id_token_is_unverified_and_returns_nothing() {
    for (fault, check) in [
        (fake::Fault::UnknownKid, oauth::Check::Keys),
        (fake::Fault::AnotherKey, oauth::Check::Signature),
        (fake::Fault::AlgNone, oauth::Check::Signature),
        (fake::Fault::AlgHs256, oauth::Check::Signature),
        (fake::Fault::TwoKeysNoKid, oauth::Check::Keys),
        (fake::Fault::Issuer, oauth::Check::Issuer),
        (fake::Fault::Audience, oauth::Check::Audience),
        (fake::Fault::Azp, oauth::Check::Audience),
        (fake::Fault::Expired(Duration::from_secs(60)), oauth::Check::Expiry),
        (fake::Fault::Nonce, oauth::Check::Nonce),
        (fake::Fault::NoSubject, oauth::Check::Missing),
        (fake::Fault::NoIdToken, oauth::Check::Missing),
    ] {
        let mut client = oauth::Client::new(client_limits()).expect("client");
        let mut issuer = openid_issuer(Some(fault));
        let terminal = finished(&mut client, &mut issuer, openid_registration());
        assert_eq!(failure(terminal.as_ref()), oauth::Failure::Unverified { check }, "{fault:?}");
    }
}
#[test]
fn keys_and_audiences_the_check_ignores_still_sign_in() {
    for fault in [fake::Fault::ExtraEcKey, fake::Fault::TwoAudiences, fake::Fault::Expired(Duration::from_secs(59))] {
        let mut client = oauth::Client::new(client_limits()).expect("client");
        let mut issuer = openid_issuer(Some(fault));
        let record = record(finished(&mut client, &mut issuer, openid_registration()));
        assert!(record.subject.as_deref() == Some(b"user-42"), "{fault:?}");
    }
}
fn redeeming_twice(client: &mut oauth::Client, issuer: &mut fake::Issuer) -> (oauth::HttpRequest, oauth::Event) {
    let url = openid_visit(client, issuer, openid_registration());
    let event = redirected(issuer, url);
    let oauth::Event::Redirected { client_id, uri, state, code, error, now } = &event else { panic!("redirected") };
    let duplicate = oauth::Event::Redirected {
        client_id: client_id.clone(),
        uri: uri.clone(),
        state: state.clone(),
        code: code.clone(),
        error: error.clone(),
        now: *now,
    };
    (http(client_step(client, event)), duplicate)
}
#[test]
fn an_unverified_code_is_not_redeemed_again() {
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let mut issuer = openid_issuer(Some(fake::Fault::Nonce));
    issuer.queue(success(b"access", Some(b"refresh"), 0)).expect("plan");
    let (request, duplicate) = redeeming_twice(&mut client, &mut issuer);
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    let terminal = client_step(&mut client, oauth::Event::Http(answer));
    assert_eq!(failure(terminal.as_ref()), oauth::Failure::Unverified { check: oauth::Check::Nonce });
    for seconds in [4, 10, 20] {
        assert!(client_step(&mut client, oauth::Event::Tick { now: time(seconds) }).is_none());
    }
    assert!(client_step(&mut client, duplicate).is_none());
}
#[test]
fn an_issued_client_id_is_the_audience_checked() {
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let mut issuer = openid_issuer_with(None, &[b"issued-client"]);
    let mut registration = openid_registration();
    registration.terms.as_mut().expect("terms").issued_client_id = true;
    let record = record(finished(&mut client, &mut issuer, registration));
    assert!(record.issued_client_id.as_deref() == Some(b"issued-client"));
    assert!(record.subject.as_deref() == Some(b"user-42"));
}

#[test]
fn two_subjects_share_an_address_in_one_world() {
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let mut issuer = openid_issuer(None);
    for subject in [b"member".as_slice(), b"another-subject"] {
        issuer
            .sign_in_as(fake::Account {
                subject: boxed(subject),
                email: Some(fake::Email { address: boxed(b"shared@example.test"), verified: true }),
                name: None,
            })
            .expect("account");
        let record = record(finished(&mut client, &mut issuer, openid_registration()));
        assert_eq!(record.subject.as_deref(), Some(subject));
        let token = record.id_token.expect("ID token");
        let payload = token.split(|byte| *byte == b'.').nth(1).expect("payload");
        let payload = skein_lib::base64::decode(skein_lib::base64::Alphabet::Url, payload, documents().id_token_bytes)
            .expect("base64url");
        let json = oauth::Json::from_bytes(&payload, &documents()).expect("claims");
        let expected =
            oauth::Json::from_bytes(br#"{"email":"shared@example.test"}"#, &documents()).expect("email claim");
        let email = expected.as_tokens().get(1..3).expect("email key and value");
        assert!(json.as_tokens().windows(email.len()).any(|pair| pair == email));
        let _ = client_step(&mut client, oauth::Event::Reset);
    }
}

#[test]
fn a_google_like_sign_in_keeps_an_access_only_identity() {
    for confidential in [false, true] {
        let mut registration = openid_registration();
        registration.scope = boxed(b"openid email profile");
        if confidential {
            registration.client_secret = Some(boxed(b"test-secret"));
        }
        let mut additions = skein_oauth_world::issued(&[]);
        additions.openid = Some(fake::OpenId {
            issuer: boxed(b"https://issuer.example"),
            key_set_url: boxed(b"https://issuer.example/keys"),
            subject: boxed(b"user-42"),
            lifetime: Duration::from_secs(30),
            header_kid: true,
            fault: None,
        });
        let mut issuer = skein_oauth_world::configured_issuer(&registration, Some(additions));
        issuer
            .sign_in_as(fake::Account {
                subject: boxed(b"user-42"),
                email: Some(fake::Email { address: boxed(b"person@example.test"), verified: true }),
                name: Some(boxed(b"Test Person")),
            })
            .expect("account");
        let mut plan = success(b"access", None, 0);
        if let fake::Body::Token(token) = &mut plan.body {
            token.scope = Some(boxed(b"openid email profile"));
        }
        issuer.queue(plan).expect("plan");
        let mut client = oauth::Client::new(client_limits()).expect("client");
        let post = openid_post(&mut client, &mut issuer, registration);
        let answer =
            response(fake_step(&mut issuer, fake::Event::Post { request: post, now: time(2), wall: wall(100) }));
        let json = oauth::Json::from_bytes(&answer.body, &documents()).expect("answer");
        let id_token = oauth::decode_response(&json, &documents()).expect("token answer").id_token;
        let record = record(client_step(&mut client, oauth::Event::Http(answer)));
        assert!(record.refresh_token.is_none());
        assert!(record.subject.as_deref() == Some(b"user-42"));
        assert!(record.id_token.is_some() && record.id_token == id_token);
        assert!(record.granted_scopes.as_deref() == Some(b"openid email profile"));
        let encoded = oauth::encode_record(&record, &documents()).expect("encode");
        assert!(oauth::decode_record(&encoded, &documents()).expect("decode") == record);
    }
}
