use super::*;

const ISSUED_ID_CAP: usize = 64; // limits().document.issued_client_bytes.
const ID_TOKEN_HINT: &[u8] = b"header.payload.signature";
const LOGIN_HINT: &[u8] = b"user-42";

fn issued() -> Registration {
    let mut registration = registration(false, WireFormat::Form);
    let mut additions = terms::terms(false, false);
    additions.issued_client_id = true;
    additions.parameters = Box::new([
        Parameter { name: boxed(b"every"), value: boxed(b"yes"), sent: Sent::EverySignIn },
        Parameter { name: boxed(b"new"), value: boxed(b"yes"), sent: Sent::NewRegistration },
    ]);
    registration.terms = Some(additions);
    registration
}
fn begin(registration: Registration, held: bool) -> Event {
    Event::SignIn {
        registration,
        key: 7,
        generation: 0,
        state: state(),
        verifier: None,
        nonce: None,
        port: None,
        issued_client_id: if held { Some(boxed(b"client-1")) } else { None },
        id_token: if held { Some(boxed(ID_TOKEN_HINT)) } else { None },
        subject: if held { Some(boxed(LOGIN_HINT)) } else { None },
        now: time(1),
    }
}
fn redirect(client_id: Option<Box<[u8]>>) -> Event {
    Event::Redirected {
        uri: boxed(b"https://web.example/callback"),
        state: state(),
        code: Some(boxed(b"one-use")),
        error: None,
        client_id,
        now: time(2),
    }
}
fn after_visit(registration: Registration) -> Client {
    let mut client = Client::new(limits()).expect("client");
    drop(step(&mut client, begin(registration, false)));
    client
}
fn refresh(registration: Registration, client_id: Option<Box<[u8]>>) -> Event {
    Event::Refresh {
        registration,
        prior: RefreshState {
            key: 7,
            generation: 1,
            refresh_token: boxed(b"seed"),
            issued_client_id: client_id,
            granted_scopes: None,
            subject: None,
            id_token: None,
        },
        now: time(1),
    }
}

#[test]
fn each_form_sends_what_it_must() {
    for (held, expected) in [
        (false, b"https://issuer.example/authorize?response_type=code&client_id=client&redirect_uri=https%3A%2F%2Fweb.example%2Fcallback&state=0123456789abcdef&scope=read%20profile&every=yes&new=yes".as_slice()),
        (true, b"https://issuer.example/authorize?response_type=code&client_id=client-1&redirect_uri=https%3A%2F%2Fweb.example%2Fcallback&state=0123456789abcdef&scope=read%20profile&every=yes&id_token_hint=header.payload.signature&login_hint=user-42"),
    ] {
        let mut client = Client::new(limits()).expect("client");
        let Some(Request::Visit { url }) = step(&mut client, begin(issued(), held)) else { panic!("visit") };
        assert!(url.as_ref() == expected, "held record: {held}");
    }
}

#[test]
fn a_redirect_without_the_issued_id_is_incomplete() {
    for client_id in [None, Some(boxed(b""))] {
        let mut client = after_visit(issued());
        assert_eq!(failure(step(&mut client, redirect(client_id))), Failure::Incomplete);
    }
}

#[test]
fn an_issued_client_id_is_bounded_by_its_cap() {
    for (length, expected) in [(ISSUED_ID_CAP, Ok(())), (ISSUED_ID_CAP + 1, Err(Failure::Limit))] {
        let mut client = after_visit(issued());
        let value = boxed([b'c'; ISSUED_ID_CAP + 1].get(..length).expect("id fixture"));
        let result = match step(&mut client, redirect(Some(value))) {
            Some(Request::Http(request)) => {
                let prefix = b"client_id=";
                let start = bytes::find(&request.body, prefix)
                    .expect("client id")
                    .checked_add(prefix.len())
                    .expect("prefix fits");
                let rest = request.body.get(start..).expect("client id value");
                let end = bytes::find(rest, b"&").unwrap_or(rest.len());
                assert_eq!(end, ISSUED_ID_CAP);
                Ok(())
            }
            Some(Request::Failed { failure }) => Err(failure),
            Some(Request::Visit { .. } | Request::Tokens { .. }) | None => panic!("redirect terminal or exchange"),
        };
        assert_eq!(result, expected, "length: {length}");
    }
}

#[test]
fn every_later_request_carries_the_issued_id() {
    for refreshing in [false, true] {
        let mut client = if refreshing { Client::new(limits()).expect("client") } else { after_visit(issued()) };
        let event =
            if refreshing { refresh(issued(), Some(boxed(b"client-1"))) } else { redirect(Some(boxed(b"client-1"))) };
        let http = request(step(&mut client, event));
        assert!(bytes::find(&http.body, b"client_id=client-1&").is_some());
        let Some(Request::Tokens { record, .. }) = step(&mut client, answered(http.id, 200, &token_body(), 3)) else {
            panic!("tokens")
        };
        assert!(record.issued_client_id == Some(boxed(b"client-1")));
    }
}

#[test]
fn a_refresh_without_an_issued_id_is_refused_unsent() {
    let mut client = Client::new(limits()).expect("client");
    assert_eq!(failure(step(&mut client, refresh(issued(), None))), Failure::Refused);
}

#[test]
fn an_added_error_name_signs_in_again() {
    for status in [400, 401, 403, 429, 500] {
        let mut registration = issued();
        registration.terms.as_mut().expect("terms").sign_in_again = Box::new([boxed(b"refresh_gone")]);
        let mut client = Client::new(limits()).expect("client");
        let http = request(step(&mut client, refresh(registration, Some(boxed(b"client-1")))));
        assert_eq!(
            failure(step(&mut client, answered(http.id, status, br#"{"error":"refresh_gone"}"#, 2))),
            Failure::Refused,
            "status: {status}"
        );
    }
}

#[test]
fn tokens_name_the_missing_required_scopes() {
    for (scope, expected) in
        [(Some(b"read profile".as_slice()), b"".as_slice()), (Some(b"read".as_slice()), b"profile"), (None, b"")]
    {
        let mut registration = issued();
        registration.terms.as_mut().expect("terms").required_scopes = Some(boxed(b"read profile"));
        let mut client = after_visit(registration);
        let http = request(step(&mut client, redirect(Some(boxed(b"client-1")))));
        let Some(Request::Tokens { record, missing }) =
            step(&mut client, answered(http.id, 200, &token_body_with_scope(scope), 3))
        else {
            panic!("tokens")
        };
        assert_eq!(missing.as_ref(), expected);
        assert_eq!(record.generation, 1);
    }
}
