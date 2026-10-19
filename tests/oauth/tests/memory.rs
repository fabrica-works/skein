use skein_fake_oauth as fake;
use skein_heap::{Counting, Meter};
use skein_lib::{Duration, Queue, Time, Wall};
use skein_oauth as oauth;
use skein_oauth_world::{boxed, client_limits, documents, registration, terms, visit};

#[global_allocator]
static HEAP: Counting = Counting;

#[test]
fn client_and_rotating_issuer_steps_fit_declared_heap_bounds() {
    let mut document = documents();
    document.document_bytes = 2048;
    document.string_bytes = 1024;
    check_client(document);
    check_issuer(document);
}

fn check_client(document: oauth::Limits) {
    let client_limits = oauth::ClientLimits {
        document,
        uri_bytes: 256,
        scope_bytes: 64,
        state_bytes: 64,
        code_bytes: 64,
        url_bytes: 1024,
        request_bytes: 1024,
        parameter_bytes: 0,
        resource_bytes: 0,
        nonce_bytes: 0,
        sign_in_time: Duration::from_secs(120),
        request_time: Duration::from_secs(10),
        backoff_base: Duration::from_secs(2),
        backoff_ceiling: Duration::from_secs(8),
        max_attempts: 3,
    };
    let bound = oauth::client_worst_case(&client_limits).expect("bound");
    let meter = Meter::new();
    let mut client = oauth::Client::new(client_limits).expect("client");
    let mut out = Queue::with_capacity(1);
    let event = oauth::Event::Refresh {
        registration: oauth::Registration {
            authorization_url: boxed(b"https://issuer.example/authorize"),
            token_endpoint: boxed(b"https://issuer.example/token"),
            client_id: boxed(b"client"),
            redirect_uri: boxed(b"http://127.0.0.1:1234/callback"),
            scope: boxed(b""),
            wire: oauth::WireFormat::Json,
            client_secret: None,
            pkce_for_confidential: false,
            metadata_claim: None,
            terms: None,
        },
        prior: oauth::RefreshState {
            key: 1,
            generation: 0,
            refresh_token: boxed(b"seed"),
            issued_client_id: Some(boxed(&[b'c'; 64])),
            granted_scopes: Some(boxed(&[b's'; 64])),
            subject: Some(boxed(&[b'u'; 64])),
            id_token: Some(boxed(&[b'i'; 1024])),
        },
        now: Time::ZERO,
    };
    meter.start();
    client.step(event, &mut out);
    let measured = meter.end();
    drop(out.pop());
    meter.check(measured, bound, &"OAuth client refresh");
}

fn check_issuer(document: oauth::Limits) {
    let fake_limits = fake::Limits { document, uri_bytes: 256, request_bytes: 1024, codes: 8, rotations: 8, plans: 8 };
    let bound = fake::worst_case(&fake_limits).expect("bound");
    let meter = Meter::new();
    let mut issuer = fake::Issuer::new(
        fake::Config {
            authorization_url: boxed(b"https://issuer.example/authorize"),
            token_endpoint: boxed(b"https://issuer.example/token"),
            client_id: boxed(b"client"),
            client_secret: None,
            redirect_uri: boxed(b"http://127.0.0.1:1234/callback"),
            refresh_token: boxed(b"seed"),
        },
        fake_limits,
    )
    .expect("issuer");
    issuer
        .queue(fake::Plan {
            status: 200,
            body: fake::Body::Token(oauth::TokenResponse {
                access_token: boxed(b"access"),
                refresh_token: Some(boxed(b"next")),
                expires_in: 30,
                scope: Some(boxed(&[b's'; 64])),
                id_token: Some(boxed(&[b'i'; 1024])),
                earliest_refresh_at: Some(Wall::from_nanos(10_000_000_000)),
            }),
            delay: Duration::ZERO,
            retry_after: Duration::ZERO,
        })
        .expect("plan");
    let request = oauth::HttpRequest {
        id: 1,
        endpoint: boxed(b"https://issuer.example/token"),
        content_type: b"application/json",
        body: boxed(br#"{"grant_type":"refresh_token","client_id":"client","refresh_token":"seed"}"#),
        deadline: Time::from_nanos(10_000_000_000),
    };
    let mut out = Queue::with_capacity(1);
    meter.start();
    issuer.step(fake::Event::Post { request, now: Time::ZERO, wall: Wall::EPOCH }, &mut out);
    let measured = meter.end();
    drop(out.pop());
    meter.check(measured, bound, &"OAuth issuer rotation");
}

#[test]
fn terms_at_their_most_entries_fit_the_worst_case() {
    let mut limits = client_limits();
    limits.url_bytes = 2048;
    let bound = oauth::client_worst_case(&limits).expect("bound");
    let meter = Meter::new();
    let mut client = oauth::Client::new(limits).expect("client");
    meter.start();
    let mut registration = registration(true);
    let mut additions = terms();
    let mut parameters = Vec::new();
    for _ in 0..limits.parameter_bytes {
        parameters.push(oauth::Parameter { name: boxed(b"a"), value: boxed(b""), sent: oauth::Sent::EverySignIn });
    }
    additions.parameters = parameters.into_boxed_slice();
    registration.terms = Some(additions);
    let url = visit(&mut client, registration, None, None);
    let measured = meter.end();
    drop(url);
    meter.check(measured, bound, &"OAuth terms at their most entries");
}
