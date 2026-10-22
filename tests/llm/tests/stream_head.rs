//! Public Responses stream admission and bounded response-head diagnostics.
use skein_llm::client::Evidence;
use skein_llm::{Credential, Failure, Provider, client};
use skein_llm_world::{TERMINAL, World, events, limits_for, public_call, response, run_call};

fn stream(headers: &str, body: &[u8], chunked: bool, credential: Option<Credential>) -> World {
    let mut input = public_call(1);
    input.credential = credential;
    run_call(input, limits_for(Provider::OpenAi), response(200, headers, body, chunked), 23)
}

#[test]
fn public_responses_accepts_missing_type_for_every_credential_kind() {
    for chunked in [false, true] {
        for credential in [
            Some(Credential::bearer(b"fixture-token".to_vec().into())),
            Some(Credential::key(b"fixture-key".to_vec().into())),
            None,
        ] {
            let world = stream("", &events(&[TERMINAL]), chunked, credential);
            world.assert_once();
            assert!(world.seen.iter().any(|event| matches!(event, client::Event::Completed { .. })));
            assert!(world.seen.iter().any(|event| matches!(event, client::Event::Reusable)));
        }
    }
}

#[test]
fn wrong_types_name_only_the_bounded_media_type() {
    for (headers, expected) in [
        (
            "Content-Type: application/json; diagnostic=private-parameter\r\n",
            "response is not an event stream (content-type: application/json)",
        ),
        ("Content-Type: text/html; charset=utf-8\r\n", "response is not an event stream (content-type: text/html)"),
        (
            "Content-Type: text/event-stream\r\ncontent-type: text/event-stream\r\n",
            "response is not an event stream (content-type: duplicate)",
        ),
        ("Content-Type: not a media type\r\n", "response is not an event stream (content-type: invalid)"),
    ] {
        let world = stream(headers, &events(&[TERMINAL]), true, None);
        world.assert_once();
        assert!(world.seen.iter().any(|event| matches!(
            event,
            client::Event::Failed { failure: Failure::Protocol, evidence: Evidence::Response { status: 200 }, detail, .. }
                if detail.as_ref() == expected.as_bytes()
        )), "{expected}");
    }
}

#[test]
fn a_missing_type_still_requires_a_terminal_event() {
    for body in [b"{}".as_slice(), b"<!doctype html>".as_slice(), b"data: not-json\n\n".as_slice()] {
        let world = stream("", body, false, None);
        world.assert_once();
        assert!(world.seen.iter().any(|event| matches!(
            event,
            client::Event::Failed { failure: Failure::Protocol, evidence: Evidence::Response { status: 200 }, .. }
        )));
    }
}

#[test]
fn media_type_diagnostics_hold_at_the_bound() {
    // The diagnostic retains at most 64 media-type bytes, excluding parameters.
    for (headers, expected) in [
        (
            "Content-Type: application/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n",
            "response is not an event stream (content-type: application/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa)",
        ),
        (
            "Content-Type: application/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\n",
            "response is not an event stream (content-type: too long)",
        ),
    ] {
        let world = stream(headers, &events(&[TERMINAL]), false, None);
        world.assert_once();
        assert!(world.seen.iter().any(|event| matches!(
            event,
            client::Event::Failed { failure: Failure::Protocol, detail, .. }
                if detail.as_ref() == expected.as_bytes()
        )));
    }
}

#[test]
fn explicit_event_stream_type_accepts_case_and_parameters() {
    let world = stream("Content-Type: Text/Event-Stream; charset=utf-8\r\n", &events(&[TERMINAL]), true, None);
    world.assert_once();
    assert!(world.seen.iter().any(|event| matches!(event, client::Event::Completed { .. })));
}

#[test]
fn an_endpoint_override_keeps_the_openai_stream_contract() {
    let mut input = public_call(1);
    input.endpoint.authority = b"fixture.test".to_vec().into();
    input.endpoint.target = b"/responses".to_vec().into();
    let world = run_call(input, limits_for(Provider::OpenAi), response(200, "", &events(&[TERMINAL]), true), 23);
    world.assert_once();
    assert!(world.seen.iter().any(|event| matches!(event, client::Event::Completed { .. })));
}
