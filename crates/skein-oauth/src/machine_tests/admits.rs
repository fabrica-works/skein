//! Pure redirect admission and the event's fatal boundary (oauth.md, 3).
use super::*;

fn redirect() -> Redirect {
    Redirect {
        uri: boxed(b"http://127.0.0.1:2345/callback"),
        state: state(),
        code: Some(boxed(b"code")),
        error: None,
        client_id: Some(boxed(b"issued")),
    }
}
fn begin(issued: bool, varying: bool, openid: bool) -> Client {
    let mut registration = registration(true, WireFormat::Form);
    let mut terms = terms::terms(openid, varying);
    terms.issued_client_id = issued;
    registration.terms = Some(terms);
    if openid {
        registration.scope = boxed(b"openid");
    }
    if varying {
        registration.redirect_uri = boxed(b"http://127.0.0.1:0/callback");
    }
    let mut event = openid::sign_in(registration);
    if let Event::SignIn { port, nonce, .. } = &mut event {
        if !openid {
            *nonce = None;
        }
        *port = varying.then_some(2345);
    }
    let mut client = Client::new(limits()).expect("client");
    drop(step(&mut client, event));
    client
}
fn deliver(client: &mut Client, redirect: Redirect, now: Time) -> Option<Request> {
    step(
        client,
        Event::Redirected {
            uri: redirect.uri,
            state: redirect.state,
            code: redirect.code,
            error: redirect.error,
            client_id: redirect.client_id,
            now,
        },
    )
}

#[test]
fn a_refused_admission_leaves_the_right_redirect_to_be_taken() {
    for (name, kind, expected) in [
        ("another uri", 0_u8, Inadmissible::Address),
        ("another bound port", 1, Inadmissible::Address),
        ("code and error", 2, Inadmissible::Address),
        ("neither", 3, Inadmissible::Address),
        ("empty code", 4, Inadmissible::Address),
        ("missing issued id", 5, Inadmissible::Address),
        ("empty issued id", 6, Inadmissible::Address),
        ("state one byte off", 7, Inadmissible::State),
        ("state another length", 8, Inadmissible::State),
        ("at deadline", 9, Inadmissible::NotWaiting),
        ("past deadline", 10, Inadmissible::NotWaiting),
    ] {
        let mut client = begin(true, true, false);
        let mut wrong = redirect();
        match kind {
            0 => wrong.uri = boxed(b"http://127.0.0.1:2345/another"),
            1 => wrong.uri = boxed(b"http://127.0.0.1:2346/callback"),
            2 => wrong.error = Some(boxed(b"denied")),
            3 => wrong.code = None,
            4 => wrong.code = Some(boxed(b"")),
            5 => wrong.client_id = None,
            6 => wrong.client_id = Some(boxed(b"")),
            7 => wrong.state = boxed(b"1123456789abcdef"),
            8 => wrong.state = boxed(b"0123456789abcde"),
            9 | 10 => {}
            _ => unreachable!("fixture kind"),
        }
        let now = match kind {
            9 => time(121),
            10 => time(122),
            _ => time(2),
        };
        assert_eq!(client.admits(&wrong, now), Err(expected), "{name}");
        assert_eq!(client.admits(&redirect(), time(2)), Ok(()), "{name}");
        let _http = request(deliver(&mut client, redirect(), time(2)));
    }
}

#[test]
fn admission_outside_await_redirect_is_not_waiting_and_changes_nothing() {
    for name in ["before sign-in", "await keys", "after done", "await http", "backoff"] {
        let mut client = match name {
            "before sign-in" => Client::new(limits()).expect("client"),
            "await keys" => begin(false, false, true),
            "after done" => {
                let mut client = begin(false, false, false);
                drop(step(&mut client, Event::Cancel));
                client
            }
            "await http" | "backoff" => {
                let mut client = begin(false, false, false);
                let http = request(deliver(&mut client, redirect(), time(2)));
                if name == "backoff" {
                    drop(step(
                        &mut client,
                        Event::Http(HttpResponse {
                            id: http.id,
                            status: 0,
                            body: boxed(b""),
                            retry_after: Duration::ZERO,
                            evidence: HttpEvidence::Unsent,
                            now: time(3),
                            wall: wall(3),
                        }),
                    ));
                }
                client
            }
            _ => unreachable!("fixture phase"),
        };
        let deadline = client.next_deadline();
        assert_eq!(client.admits(&redirect(), time(2)), Err(Inadmissible::NotWaiting), "{name}");
        assert_eq!(client.next_deadline(), deadline, "{name}");
        drop(step(&mut client, Event::Cancel));
        drop(step(&mut client, Event::Reset));
        let mut event = openid::sign_in(registration(true, WireFormat::Form));
        if let Event::SignIn { nonce, .. } = &mut event {
            *nonce = None;
        }
        drop(step(&mut client, event));
        let _http = request(deliver(&mut client, redirect(), time(2)));
    }
}

#[test]
fn an_error_only_redirect_is_admitted_and_taken_as_refused() {
    let mut client = begin(true, false, false);
    let mut redirect = redirect();
    redirect.code = None;
    redirect.client_id = None;
    redirect.error = Some(boxed(b"access_denied"));
    assert_eq!(client.admits(&redirect, time(2)), Ok(()));
    assert_eq!(failure(deliver(&mut client, redirect, time(2))), Failure::Refused);
}

#[test]
fn an_empty_optional_value_is_absent_and_the_event_keeps_its_failure_vocabulary() {
    for (name, kind, expected) in [
        ("deadline", 0_u8, Failure::InvalidRedirect),
        ("address", 1, Failure::InvalidRedirect),
        ("state", 2, Failure::InvalidRedirect),
        ("neither", 3, Failure::Malformed),
        ("empty code", 4, Failure::Malformed),
        ("both", 5, Failure::Malformed),
        ("issued id", 6, Failure::Incomplete),
        ("code bound", 7, Failure::Limit),
        ("id bound", 8, Failure::Limit),
    ] {
        let mut client = begin(true, false, false);
        let mut redirect = redirect();
        match kind {
            0 => {}
            1 => redirect.uri = boxed(b"http://127.0.0.1:2346/callback"),
            2 => redirect.state = boxed(b"another"),
            3 => redirect.code = None,
            4 => redirect.code = Some(boxed(b"")),
            5 => redirect.error = Some(boxed(b"denied")),
            6 => redirect.client_id = Some(boxed(b"")),
            7 => redirect.code = Some(boxed(&[b'c'; 129])),
            8 => redirect.client_id = Some(boxed(&[b'i'; 65])),
            _ => unreachable!("fixture kind"),
        }
        let now = if kind == 0 { time(121) } else { time(2) };
        assert_eq!(failure(deliver(&mut client, redirect, now)), expected, "{name}");
    }
    let mut client = begin(false, false, false);
    let mut redirect = redirect();
    redirect.error = Some(boxed(b""));
    assert_eq!(client.admits(&redirect, time(2)), Ok(()));
    let _http = request(deliver(&mut client, redirect, time(2)));
}
