use super::*;

const URI_CAP: usize = 256; // limits().uri_bytes.
const PARAMETER_CAP: usize = 256; // limits().parameter_bytes.
const RESOURCE_CAP: usize = 128; // limits().resource_bytes.
const NONCE_MIN: usize = 16; // oauth.md, section 2.
const NONCE_CAP: usize = 64; // limits().nonce_bytes.

pub(super) fn terms(openid: bool, varying_port: bool) -> Terms {
    Terms {
        parameters: Box::new([]),
        openid: if openid { Some(OpenId { issuer: boxed(b"https://issuer.example"), key_set: None }) } else { None },
        issued_client_id: false,
        varying_port,
        resource: None,
        required_scopes: None,
        revocation_endpoint: None,
        sign_in_again: Box::new([]),
    }
}
fn with_terms(additions: Terms) -> Registration {
    let mut registration = registration(true, WireFormat::Form);
    registration.scope = boxed(b"openid read");
    registration.terms = Some(additions);
    registration
}
fn urls(issuer: &[u8], key_set: Option<&[u8]>, revoke: Option<&[u8]>) -> Registration {
    let mut additions = terms(false, false);
    let mut openid = OpenId { issuer: boxed(issuer), key_set: None };
    if let Some(value) = key_set {
        openid.key_set = Some(boxed(value));
    }
    additions.openid = Some(openid);
    if let Some(value) = revoke {
        additions.revocation_endpoint = Some(boxed(value));
    }
    with_terms(additions)
}
fn uri(length: usize) -> Box<[u8]> {
    let mut value = [b'u'; URI_CAP + 1];
    for (slot, byte) in value.iter_mut().zip(b"https://") {
        *slot = *byte;
    }
    boxed(value.get(..length).expect("URI fixture"))
}
fn parameters(length: usize) -> Registration {
    let mut additions = terms(false, false);
    additions.parameters = Box::new([Parameter {
        name: boxed(b"p"),
        value: boxed(
            [b'p'; PARAMETER_CAP].get(..length.checked_sub(1).expect("parameter name")).expect("parameter fixture"),
        ),
        sent: Sent::EverySignIn,
    }]);
    with_terms(additions)
}
fn empty_parameter_name() -> Registration {
    let mut additions = terms(false, false);
    additions.parameters = Box::new([Parameter { name: boxed(b""), value: boxed(b""), sent: Sent::EverySignIn }]);
    with_terms(additions)
}
fn errors(length: usize) -> Registration {
    let mut additions = terms(false, false);
    additions.sign_in_again = Box::new([boxed([b'e'; PARAMETER_CAP + 1].get(..length).expect("error fixture"))]);
    with_terms(additions)
}
fn resource(length: usize) -> Registration {
    let mut additions = terms(false, false);
    additions.resource = Some(boxed([b'r'; RESOURCE_CAP + 1].get(..length).expect("resource fixture")));
    with_terms(additions)
}
fn start(registration: Registration, nonce: Option<Box<[u8]>>, port: Option<u16>) -> Event {
    Event::SignIn {
        issued_client_id: None,
        id_token: None,
        subject: None,
        registration,
        key: 1,
        generation: 0,
        state: state(),
        verifier: Some(verifier()),
        nonce,
        port,
        now: time(1),
    }
}

type Admission = (&'static str, Registration, Result<(), Failure>);

fn admission_rows() -> Box<[Admission]> {
    let mut no_scope = with_terms(terms(true, false));
    no_scope.scope = boxed(b"read");
    let mut localhost = with_terms(terms(false, true));
    localhost.redirect_uri = boxed(b"http://localhost:1234/callback");
    Box::new([
        ("openid scope", no_scope, Err(Failure::Malformed)),
        ("varying name", localhost, Err(Failure::InvalidRedirect)),
        ("issuer scheme", urls(b"http://issuer.example", None, None), Err(Failure::Malformed)),
        (
            "keys scheme",
            urls(b"https://issuer.example", Some(b"http://issuer.example/keys"), None),
            Err(Failure::Malformed),
        ),
        (
            "revocation scheme",
            urls(b"https://issuer.example", None, Some(b"http://issuer.example/revoke")),
            Err(Failure::Malformed),
        ),
        ("issuer cap", urls(&uri(URI_CAP), None, None), Ok(())),
        ("issuer past cap", urls(&uri(URI_CAP + 1), None, None), Err(Failure::Limit)),
        ("keys cap", urls(b"https://issuer.example", Some(&uri(URI_CAP)), None), Ok(())),
        ("keys past cap", urls(b"https://issuer.example", Some(&uri(URI_CAP + 1)), None), Err(Failure::Limit)),
        ("revocation cap", urls(b"https://issuer.example", None, Some(&uri(URI_CAP))), Ok(())),
        ("revocation past cap", urls(b"https://issuer.example", None, Some(&uri(URI_CAP + 1))), Err(Failure::Limit)),
        ("empty parameter name", empty_parameter_name(), Err(Failure::Limit)),
        ("empty value", parameters(1), Ok(())),
        ("empty error name", errors(0), Err(Failure::Limit)),
        ("parameters cap", parameters(PARAMETER_CAP), Ok(())),
        ("parameters past cap", parameters(PARAMETER_CAP + 1), Err(Failure::Limit)),
        ("error names cap", errors(PARAMETER_CAP), Ok(())),
        ("error names past cap", errors(PARAMETER_CAP + 1), Err(Failure::Limit)),
        ("resource cap", resource(RESOURCE_CAP), Ok(())),
        ("resource past cap", resource(RESOURCE_CAP + 1), Err(Failure::Limit)),
    ])
}

#[test]
fn terms_are_refused_where_the_design_refuses() {
    for (name, registration, expected) in admission_rows() {
        assert_eq!(validate_registration(&registration, &limits(), true), expected, "{name}");
    }
}

#[test]
fn a_registration_admits_port_zero_only_where_the_redirect_varies() {
    for (uri, varying, expected) in [
        (b"http://127.0.0.1:0/callback".as_slice(), true, Ok(())),
        (b"http://[::1]:0/callback".as_slice(), true, Ok(())),
        (b"http://127.0.0.1:0/callback".as_slice(), false, Err(Failure::InvalidRedirect)),
    ] {
        let mut registration = with_terms(terms(false, varying));
        registration.redirect_uri = boxed(uri);
        assert_eq!(validate_registration(&registration, &limits(), true), expected, "{uri:?}");
    }
}

#[test]
fn redirect_address_refuses_port_zero_on_each_loopback() {
    for uri in [b"http://127.0.0.1:0/callback".as_slice(), b"http://[::1]:0/callback"] {
        assert_eq!(redirect_address(uri), Err(Failure::InvalidRedirect), "{uri:?}");
    }
}

type RedirectRow = (&'static [u8], &'static [u8], &'static [u8]);

fn varying_redirects() -> [RedirectRow; 4] {
    [
        (
            b"http://127.0.0.1:0/callback".as_slice(),
            b"redirect_uri=http%3A%2F%2F127.0.0.1%3A40000%2Fcallback".as_slice(),
            b"http://127.0.0.1:40000/callback".as_slice(),
        ),
        (
            b"http://127.0.0.1:1234/callback",
            b"redirect_uri=http%3A%2F%2F127.0.0.1%3A40000%2Fcallback",
            b"http://127.0.0.1:40000/callback",
        ),
        (
            b"http://[::1]:0/callback",
            b"redirect_uri=http%3A%2F%2F%5B%3A%3A1%5D%3A40000%2Fcallback",
            b"http://[::1]:40000/callback",
        ),
        (
            b"http://[::1]:1234/callback",
            b"redirect_uri=http%3A%2F%2F%5B%3A%3A1%5D%3A40000%2Fcallback",
            b"http://[::1]:40000/callback",
        ),
    ]
}

#[test]
fn a_varying_redirect_takes_the_bound_port() {
    const BOUND_PORT: u16 = 40000; // The listener's port for these rows.
    for (uri, encoded, bound_uri) in varying_redirects() {
        let mut registration = with_terms(terms(false, true));
        registration.redirect_uri = boxed(uri);
        let mut client = Client::new(limits()).expect("client");
        let Some(Request::Visit { url }) = step(&mut client, start(registration, None, Some(BOUND_PORT))) else {
            panic!("visit")
        };
        assert!(bytes::find(&url, encoded).is_some(), "{uri:?}");
        let request = request(step(
            &mut client,
            Event::Redirected {
                client_id: None,
                uri: boxed(bound_uri),
                state: state(),
                code: Some(boxed(b"one-use")),
                error: None,
                now: time(2),
            },
        ));
        assert!(bytes::find(&request.body, encoded).is_some(), "{uri:?}");
    }
}

#[test]
fn a_sign_in_refuses_a_port_the_redirect_does_not_take() {
    for (varying, port) in [(true, None), (true, Some(0)), (false, Some(40000))] {
        let registration = with_terms(terms(false, varying));
        let mut client = Client::new(limits()).expect("client");
        assert_eq!(
            failure(step(&mut client, start(registration, None, port))),
            Failure::InvalidRedirect,
            "{varying} {port:?}"
        );
    }
}

fn nonce_request(length: Option<usize>, enabled: bool) -> Option<Request> {
    let registration = with_terms(terms(enabled, false));
    let mut client = Client::new(limits()).expect("client");
    let mut nonce = None;
    if let Some(length) = length {
        nonce = Some(boxed([b'n'; NONCE_CAP + 1].get(..length).expect("nonce row fits")));
    }
    step(&mut client, start(registration, nonce, None))
}

#[test]
fn a_nonce_is_bounded_and_needs_openid() {
    for (length, enabled, expected) in [
        (None, true, Err(Failure::Limit)),
        (Some(NONCE_MIN - 1), true, Err(Failure::Limit)),
        (Some(NONCE_MIN), true, Ok(())),
        (Some(NONCE_CAP), true, Ok(())),
        (Some(NONCE_CAP + 1), true, Err(Failure::Limit)),
        (Some(NONCE_MIN), false, Err(Failure::Limit)),
    ] {
        let admission = match nonce_request(length, enabled) {
            Some(Request::Visit { .. }) => Ok(()),
            Some(Request::Failed { failure }) => Err(failure),
            Some(Request::Http(_) | Request::Tokens { .. } | Request::Revoked { .. }) | None => {
                panic!("unexpected request")
            }
        };
        assert_eq!(admission, expected, "nonce length {length:?}, openid {enabled}");
    }
}

#[test]
fn an_admitted_nonce_is_sent_in_the_visit_url() {
    let Some(Request::Visit { url }) = nonce_request(Some(NONCE_MIN), true) else { panic!("visit") };
    assert!(bytes::find(&url, b"&nonce=").is_some());
}

#[test]
fn an_empty_parameter_value_is_carried_in_the_url() {
    let mut client = Client::new(limits()).expect("client");
    let Some(Request::Visit { url }) = step(&mut client, start(parameters(1), None, None)) else { panic!("visit") };
    assert!(url.ends_with(b"&p="));
}
