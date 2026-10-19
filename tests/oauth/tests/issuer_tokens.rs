use skein_fake_oauth as fake;
use skein_llm::{Credential, Endpoint, Provider, Scheme, client};
use skein_llm_world::{fake::Exchange, limits_for, public_call};
use skein_oauth as oauth;
use skein_oauth_world::*;

struct Issued {
    client: oauth::Client,
    issuer: fake::Issuer,
    registration: oauth::Registration,
    saved: oauth::SavedToken,
    granted: Box<[Box<[u8]>]>,
    held: Vec<Box<[u8]>>,
}

fn revocable_registration() -> oauth::Registration {
    let mut registration = registration(true);
    let mut additions = terms();
    additions.revocation_endpoint = Some(boxed(b"https://issuer.example/revoke"));
    registration.terms = Some(additions);
    registration
}

fn signed_in_and_refreshed() -> Issued {
    let mut issuer = issuer_with(true, Some(issued(&[])));
    issuer.queue(success(b"sign-in-access", Some(b"rotated-refresh"), 0)).expect("sign-in plan");
    issuer.queue(success(b"refreshed-access", None, 0)).expect("refresh plan");
    let granted = issuer.granted();
    let mut client = oauth::Client::new(client_limits()).expect("client");
    let request = sign_in_with(&mut client, &mut issuer, revocable_registration());
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(2), wall: wall(100) }));
    let saved = record(client_step(&mut client, oauth::Event::Http(answer)));
    let mut held = vec![saved.access_token.clone()];
    drop(client_step(&mut client, oauth::Event::Reset));
    let request = http(client_step(
        &mut client,
        oauth::Event::Refresh {
            registration: revocable_registration(),
            prior: saved.refresh_state().expect("refresh"),
            now: time(3),
        },
    ));
    let answer = response(fake_step(&mut issuer, fake::Event::Post { request, now: time(3), wall: wall(101) }));
    let saved = record(client_step(&mut client, oauth::Event::Http(answer)));
    held.push(saved.access_token.clone());
    Issued { client, issuer, registration: revocable_registration(), saved, granted, held }
}

fn exchange(granted: &[Box<[u8]>], carried: usize) -> Exchange {
    let bounds = limits_for(Provider::OpenAi);
    let mut call = public_call(1);
    call.credential = Some(Credential::bearer(granted[carried].clone()));
    let machine = client::Client::prepare(call, &bounds).expect("prepared public call");
    let mut world = Exchange::prepared(
        machine,
        Endpoint::openai(),
        Some(Credential::bearer(granted[0].clone())),
        bounds,
        Box::new([]),
    );
    for token in &granted[1..] {
        world.accept(Credential::bearer(token.clone())).expect("accepted before serving");
    }
    world
}

#[test]
fn the_issuers_granted_tokens_are_the_llm_peers_bearers() {
    let issued = signed_in_and_refreshed();
    assert!(issued.granted.as_ref() == issued.held.as_slice(), "the script lists what the client holds");
    for (carried, index) in [(0, 0), (1, 1)] {
        let mut world = exchange(&issued.granted, carried);
        world.start();
        world.run();
        let [seen] = world.presented.as_slice() else { panic!("one request") };
        assert_eq!((seen.scheme, seen.index), (Some(Scheme::Bearer), Some(index)));
        assert!(world.seen.iter().any(|event| matches!(event, client::Event::Completed { .. })));
    }
}

#[test]
fn a_token_the_issuer_revoked_stays_accepted_by_the_llm_peer() {
    let mut issued = signed_in_and_refreshed();
    issued.issuer.queue(success(b"unused", None, 0)).unwrap();
    drop(client_step(&mut issued.client, oauth::Event::Reset));
    let request = http(client_step(
        &mut issued.client,
        oauth::Event::Revoke {
            registration: issued.registration,
            prior: issued.saved.refresh_state().unwrap(),
            now: time(4),
        },
    ));
    let answer = response(fake_step(&mut issued.issuer, fake::Event::Post { request, now: time(4), wall: wall(102) }));
    assert!(matches!(
        client_step(&mut issued.client, oauth::Event::Http(answer)),
        Some(oauth::Request::Revoked { confirmed: true })
    ));
    let mut world = exchange(&issued.granted, 1);
    world.start();
    world.run();
    let [seen] = world.presented.as_slice() else { panic!("one request") };
    assert_eq!((seen.scheme, seen.index), (Some(Scheme::Bearer), Some(1)));
    assert!(world.seen.iter().any(|event| matches!(event, client::Event::Completed { .. })));
}
