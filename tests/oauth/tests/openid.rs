use skein_fake_oauth as fake;
use skein_oauth as oauth;
use skein_oauth_world::{
    begin, client_limits, client_step, failure, fake_step, http, openid_issuer, openid_registration, response, time,
    wall,
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
