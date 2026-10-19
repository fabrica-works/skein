use super::*;

const ENDPOINT: &[u8] = b"https://issuer.example/revoke";

fn configured(public: bool, wire: WireFormat, issued: bool, endpoint: bool) -> Registration {
    let mut registration = registration(public, wire);
    let mut additions = terms::terms(false, false);
    additions.issued_client_id = issued;
    additions.revocation_endpoint = if endpoint { Some(boxed(ENDPOINT)) } else { None };
    registration.terms = Some(additions);
    registration
}

fn begin(registration: Registration, held_id: bool) -> Event {
    let Event::Refresh { registration, prior, now } =
        refresh(registration, if held_id { Some(boxed(b"client-1")) } else { None })
    else {
        panic!("refresh fixture")
    };
    Event::Revoke { registration, prior, now }
}

fn confirmed(result: Option<Request>) -> bool {
    let Some(Request::Revoked { confirmed }) = result else { panic!("revocation terminal") };
    confirmed
}

fn started(bounds: ClientLimits) -> (Client, HttpRequest) {
    let mut client = Client::new(bounds).expect("client");
    let http = request(step(&mut client, begin(configured(true, WireFormat::Form, false, true), false)));
    (client, http)
}

fn field(fields: &mut &[u8], expected_name: &[u8], expected_value: &[u8]) {
    let end = bytes::find(fields, b"&").unwrap_or(fields.len());
    let value = fields.get(..end).expect("field");
    *fields = fields.get(end.saturating_add(1)..).unwrap_or(b"");
    let separator = bytes::find(value, b"=").expect("field has a value");
    assert_eq!(value.get(..separator).expect("name"), expected_name);
    assert!(value.get(separator.saturating_add(1)..).expect("value") == expected_value);
}

#[test]
fn a_revocation_posts_its_form() {
    for (public, wire, issued) in
        [(true, WireFormat::Form, false), (false, WireFormat::Json, false), (false, WireFormat::Form, true)]
    {
        let event = begin(configured(public, wire, issued, true), issued);
        let Event::Revoke { registration, prior, now } = event else { panic!("revocation fixture") };
        let expected_token = prior.refresh_token.clone();
        let expected_id = if issued {
            prior.issued_client_id.as_ref().expect("issued id").clone()
        } else {
            registration.client_id.clone()
        };
        let expected_secret = registration.client_secret.clone();
        let mut client = Client::new(limits()).expect("client");
        let http = request(step(&mut client, Event::Revoke { registration, prior, now }));
        assert_eq!(http.endpoint.as_ref(), ENDPOINT);
        assert_eq!(http.content_type, b"application/x-www-form-urlencoded");
        let mut fields = http.body.as_ref();
        field(&mut fields, b"token", &expected_token);
        field(&mut fields, b"token_type_hint", b"refresh_token");
        field(&mut fields, b"client_id", &expected_id);
        if let Some(secret) = expected_secret {
            field(&mut fields, b"client_secret", &secret);
        }
        assert!(fields.is_empty());
    }
}

fn answer_case(status: u16, body: &[u8], evidence: Option<HttpEvidence>, retry: bool, bounds: ClientLimits) -> bool {
    let (mut client, http) = started(bounds);
    let event = match evidence {
        Some(evidence) => {
            let Event::Http(mut answer) = answered(http.id, status, body, 2) else { panic!("HTTP fixture") };
            answer.evidence = evidence;
            Event::Http(answer)
        }
        None => Event::Tick { now: time(11) },
    };
    let first = step(&mut client, event);
    let terminal = if retry {
        assert!(first.is_none());
        let next = request(step(&mut client, Event::Tick { now: time(4) }));
        step(&mut client, answered(next.id, 200, b"", 5))
    } else {
        first
    };
    assert!(client.is_done());
    assert_eq!(client.next_deadline(), None);
    confirmed(terminal)
}

#[test]
fn a_revocation_is_confirmed_only_by_a_200() {
    for (status, body, evidence, retry, attempts, expected) in [
        (200, b"".as_slice(), Some(HttpEvidence::Response), false, 3, true),
        (200, b"not JSON", Some(HttpEvidence::Response), false, 3, true),
        (400, b"", Some(HttpEvidence::Response), false, 3, false),
        (503, b"", Some(HttpEvidence::Response), false, 3, false),
        (0, b"", Some(HttpEvidence::Unsent), true, 3, true),
        (429, b"", Some(HttpEvidence::Response), true, 3, true),
        (0, b"", Some(HttpEvidence::Unsent), false, 1, false),
        (429, b"", Some(HttpEvidence::Response), false, 1, false),
        (0, b"", Some(HttpEvidence::Unknown), false, 3, false),
        (0, b"", None, false, 3, false),
    ] {
        let bounds = ClientLimits { max_attempts: attempts, ..limits() };
        assert_eq!(
            answer_case(status, body, evidence, retry, bounds),
            expected,
            "status {status}, evidence {evidence:?}, attempts {attempts}"
        );
    }
}

#[test]
fn a_revocation_without_an_endpoint_ends_unconfirmed_unsent() {
    for (endpoint, issued, held) in [(false, false, false), (true, true, false)] {
        let mut client = Client::new(limits()).expect("client");
        assert!(!confirmed(step(&mut client, begin(configured(true, WireFormat::Form, issued, endpoint), held))));
        assert!(client.is_done());
    }
}

#[test]
fn only_a_cancel_fails_a_revocation() {
    let (mut client, _) = started(limits());
    assert_eq!(failure(step(&mut client, Event::Cancel)), Failure::Cancelled);
    assert!(client.is_done());
}
