//! Worlds over the OAuth client and independent fake issuer.
use skein_fake_oauth as fake;
use skein_lib::{Duration, Queue, Time, Wall, bytes};
use skein_oauth as oauth;

#[must_use]
pub fn boxed(input: &[u8]) -> Box<[u8]> {
    bytes::copy_of(input)
}
#[must_use]
pub fn time(seconds: u64) -> Time {
    Time::from_nanos(seconds.saturating_mul(1_000_000_000))
}
#[must_use]
pub fn wall(seconds: u64) -> Wall {
    Wall::from_nanos(seconds.saturating_mul(1_000_000_000))
}
#[must_use]
pub fn documents() -> oauth::Limits {
    oauth::Limits {
        document_bytes: 4096,
        string_bytes: 1024,
        token_bytes: 256,
        client_bytes: 64,
        detail_bytes: 64,
        record_bytes: 2048,
        depth: 8,
        tokens: 64,
        issued_client_bytes: 64,
        granted_bytes: 64,
        subject_bytes: 64,
        id_token_bytes: 1024,
    }
}
#[must_use]
pub fn client_limits() -> oauth::ClientLimits {
    oauth::ClientLimits {
        document: documents(),
        uri_bytes: 256,
        scope_bytes: 64,
        state_bytes: 64,
        code_bytes: 64,
        url_bytes: 1024,
        request_bytes: 1024,
        parameter_bytes: 256,
        resource_bytes: 128,
        nonce_bytes: 64,
        sign_in_time: Duration::from_secs(120),
        request_time: Duration::from_secs(10),
        backoff_base: Duration::from_secs(2),
        backoff_ceiling: Duration::from_secs(8),
        max_attempts: 3,
    }
}
#[must_use]
pub fn fake_limits() -> fake::Limits {
    fake::Limits {
        document: documents(),
        uri_bytes: 256,
        request_bytes: 1024,
        codes: 8,
        rotations: 8,
        plans: 8,
        carried: 8,
    }
}
#[must_use]
pub fn registration(public: bool) -> oauth::Registration {
    oauth::Registration {
        authorization_url: boxed(b"https://issuer.example/authorize"),
        token_endpoint: boxed(b"https://issuer.example/token"),
        client_id: boxed(b"client"),
        redirect_uri: if public {
            boxed(b"http://127.0.0.1:1234/callback")
        } else {
            boxed(b"https://web.example/callback")
        },
        scope: boxed(b"read profile"),
        wire: if public { oauth::WireFormat::Form } else { oauth::WireFormat::Json },
        client_secret: if public { None } else { Some(boxed(b"secret")) },
        pkce_for_confidential: false,
        metadata_claim: None,
        terms: None,
    }
}
#[must_use]
pub fn issuer(public: bool) -> fake::Issuer {
    issuer_with(public, None)
}
#[must_use]
pub fn issuer_for(registration: &oauth::Registration) -> fake::Issuer {
    configured_issuer(registration, None)
}
#[must_use]
pub fn issuer_with(public: bool, terms: Option<fake::Terms>) -> fake::Issuer {
    configured_issuer(&registration(public), terms)
}
fn configured_issuer(registration: &oauth::Registration, terms: Option<fake::Terms>) -> fake::Issuer {
    fake::Issuer::new(
        fake::Config {
            authorization_url: registration.authorization_url.clone(),
            token_endpoint: registration.token_endpoint.clone(),
            client_id: registration.client_id.clone(),
            client_secret: registration.client_secret.clone(),
            redirect_uri: registration.redirect_uri.clone(),
            refresh_token: boxed(b"seed"),
            terms,
        },
        fake_limits(),
    )
    .expect("issuer")
}
#[must_use]
pub fn client_step(client: &mut oauth::Client, event: oauth::Event) -> Option<oauth::Request> {
    let mut out = Queue::with_capacity(1);
    client.step(event, &mut out);
    out.pop()
}
#[must_use]
pub fn fake_step(issuer: &mut fake::Issuer, event: fake::Event) -> Option<fake::Request> {
    let mut out = Queue::with_capacity(1);
    issuer.step(event, &mut out);
    out.pop()
}
#[must_use]
pub fn http(request: Option<oauth::Request>) -> oauth::HttpRequest {
    match request {
        Some(oauth::Request::Http(value)) => value,
        _ => panic!("HTTP request expected"),
    }
}
#[must_use]
pub fn response(request: Option<fake::Request>) -> oauth::HttpResponse {
    match request {
        Some(fake::Request::Http(value)) => value,
        _ => panic!("HTTP response expected"),
    }
}
#[must_use]
pub fn record(request: Option<oauth::Request>) -> oauth::SavedToken {
    match request {
        Some(oauth::Request::Tokens { record, .. }) => record,
        _ => panic!("token record expected"),
    }
}
#[must_use]
pub fn failure(request: Option<&oauth::Request>) -> oauth::Failure {
    match request {
        Some(oauth::Request::Failed { failure }) => *failure,
        _ => panic!("failure expected"),
    }
}
#[must_use]
pub fn success(access: &[u8], refresh: Option<&[u8]>, delay: u64) -> fake::Plan {
    fake::Plan {
        status: 200,
        body: fake::Body::Token(oauth::TokenResponse {
            access_token: boxed(access),
            refresh_token: refresh.map(boxed),
            expires_in: 30,
            scope: None,
            id_token: None,
            earliest_refresh_at: None,
        }),
        delay: Duration::from_secs(delay),
        retry_after: Duration::ZERO,
    }
}
#[must_use]
pub fn error(status: u16, code: &[u8], retry_after: u64) -> fake::Plan {
    fake::Plan {
        status,
        body: fake::Body::Error(oauth::OAuthError { code: boxed(code), detail: boxed(b"scripted") }),
        delay: Duration::ZERO,
        retry_after: Duration::from_secs(retry_after),
    }
}
#[must_use]
pub fn sign_in(client: &mut oauth::Client, issuer: &mut fake::Issuer, public: bool) -> oauth::HttpRequest {
    sign_in_with(client, issuer, registration(public))
}
#[must_use]
pub fn sign_in_with(
    client: &mut oauth::Client,
    issuer: &mut fake::Issuer,
    registration: oauth::Registration,
) -> oauth::HttpRequest {
    let url = visit(client, registration, None, None);
    let event = redirected(issuer, url);
    http(client_step(client, event))
}
#[must_use]
pub fn issued(ids: &[&[u8]]) -> fake::Terms {
    fake::Terms {
        issued: ids.iter().map(|id| boxed(id)).collect(),
        omit_issued: false,
        revocation_endpoint: Some(boxed(b"https://issuer.example/revoke")),
        varying_port: false,
        openid: None,
    }
}
#[must_use]
pub fn issuing_registration() -> oauth::Registration {
    let mut registration = registration(true);
    let mut additions = terms();
    additions.issued_client_id = true;
    additions.revocation_endpoint = Some(boxed(b"https://issuer.example/revoke"));
    registration.terms = Some(additions);
    registration
}

#[must_use]
pub fn terms() -> oauth::Terms {
    oauth::Terms {
        parameters: Box::new([]),
        openid: None,
        issued_client_id: false,
        varying_port: false,
        resource: None,
        required_scopes: None,
        revocation_endpoint: None,
        sign_in_again: Box::new([]),
    }
}

#[must_use]
pub fn visit(
    client: &mut oauth::Client,
    registration: oauth::Registration,
    nonce: Option<Box<[u8]>>,
    port: Option<u16>,
) -> Box<[u8]> {
    let verifier = if registration.client_secret.is_none() || registration.pkce_for_confidential {
        Some(boxed(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"))
    } else {
        None
    };
    let request = client_step(
        client,
        oauth::Event::SignIn {
            issued_client_id: None,
            id_token: None,
            subject: None,
            registration,
            key: 7,
            generation: 0,
            state: boxed(b"0123456789abcdef"),
            verifier,
            nonce,
            port,
            now: time(1),
        },
    );
    let Some(oauth::Request::Visit { url }) = request else { panic!("visit expected") };
    url
}

#[must_use]
pub fn redirected(issuer: &mut fake::Issuer, url: Box<[u8]>) -> oauth::Event {
    let redirect = fake_step(issuer, fake::Event::Authorize { url, now: time(1) });
    let Some(fake::Request::Redirect { uri, state, code, client_id }) = redirect else { panic!("redirect expected") };
    oauth::Event::Redirected { client_id, uri, state, code: Some(code), error: None, now: time(2) }
}

#[must_use]
pub fn planned(registration: &oauth::Registration, plan: fake::Plan) -> (oauth::Client, fake::Issuer) {
    let mut issuer = issuer_for(registration);
    issuer.queue(plan).expect("plan");
    (oauth::Client::new(client_limits()).expect("client"), issuer)
}
