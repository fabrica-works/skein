//! Scripted failure classes through the provider byte boundary.
use skein_fake_llm_domain::api::{self, Finish, Line, Script, Turn};
use skein_lib::{Duration, Time, Wall};
use skein_llm::{Block, Failure, Provider, Report, client};
use skein_llm_world::{call, fake::Exchange, limits};

#[test]
fn a_line_naming_another_index_lets_the_call_through() {
    let mut input = call(1);
    input.prompt.instructions = b"caller-script".as_slice().into();
    let script = Script {
        cue: b"caller-script".as_slice().into(),
        turns: Box::new([Turn {
            lines: Box::new([
                Line::Fail {
                    error: api::Error::Exhausted { retry_after: Some(skein_lib::Duration::from_secs(1)) },
                    report: api::Report::none(),
                    credentials: Box::new([1]),
                },
                Line::Text { text: b"admitted".as_slice().into() },
            ]),
            finish: Finish::Stop,
            tokens: 1,
        }]),
    };
    let mut world = Exchange::new(input, limits(), Box::new([script]));
    world.start();
    world.run();
    assert_eq!(world.queries[0].credential, Some(0));
    let Some(completion) = world.seen.iter().find_map(|event| match event {
        client::Event::Completed { completion, .. } => Some(completion),
        client::Event::Delta { .. }
        | client::Event::Block { .. }
        | client::Event::Failed { .. }
        | client::Event::Cancelled { .. }
        | client::Event::Reusable
        | client::Event::Close
        | client::Event::Closed => None,
    }) else {
        panic!("{:?}", world.seen)
    };
    let [Block::Text { text, .. }] = completion.content.as_ref() else { panic!("{:?}", completion.content) };
    assert_eq!(text.as_ref(), b"admitted");
}

fn exchange(provider: Provider, lines: Box<[Line]>) -> Exchange {
    let mut input = match provider {
        Provider::OpenAiCodex => call(1),
        Provider::Anthropic => skein_llm_world::anthropic_call(1),
        Provider::OpenAi => skein_llm_world::public_call(1),
    };
    input.prompt.instructions = b"caller-script".as_slice().into();
    Exchange::new(
        input,
        limits(),
        Box::new([Script {
            cue: b"caller-script".as_slice().into(),
            turns: Box::new([Turn { lines, finish: Finish::Stop, tokens: 1 }]),
        }]),
    )
}

fn failing(error: api::Error, report: api::Report) -> Box<[Line]> {
    Box::new([Line::Fail { error, report, credentials: Box::new([0]) }])
}

fn terminal(world: &Exchange) -> (Failure, Report, client::Evidence) {
    world
        .seen
        .iter()
        .find_map(|event| {
            if let client::Event::Failed { failure, report, evidence, .. } = event {
                Some((*failure, report.clone(), *evidence))
            } else {
                None
            }
        })
        .expect("client's terminal failure")
}

type ClassRow = (api::Error, Failure, u16, &'static [u8]);
fn class_rows(provider: Provider) -> Vec<ClassRow> {
    let codex = provider == Provider::OpenAiCodex;
    let mut rows: Vec<ClassRow> = vec![
        (
            api::Error::Exhausted { retry_after: None },
            Failure::Exhausted { retry_after: None },
            if codex { 429 } else { 402 },
            if codex { b"usage_limit_reached" } else { b"billing_error" },
        ),
        (
            api::Error::RateLimited { retry_after: Duration::from_secs(2) },
            Failure::RateLimited { retry_after: Duration::from_secs(2) },
            429,
            if codex { b"rate_limit_exceeded" } else { b"rate_limit_error" },
        ),
        (
            api::Error::Refused,
            Failure::Refused,
            403,
            if codex { b"subscription_sharing_user_not_eligible" } else { b"permission_error" },
        ),
        (api::Error::Overloaded, Failure::Overloaded, if codex { 503 } else { 529 }, b"overloaded_error"),
        (
            api::Error::Unavailable,
            Failure::Unavailable,
            if codex { 503 } else { 500 },
            if codex { b"subscription_sharing_usage_unavailable" } else { b"api_error" },
        ),
        (
            api::Error::ContextTooLong,
            Failure::ContextTooLong,
            400,
            if codex { b"context_length_exceeded" } else { b"invalid_request_error" },
        ),
        (
            api::Error::InvalidRequest,
            Failure::Invalid,
            if codex { 403 } else { 400 },
            if codex { b"subscription_sharing_route_not_supported" } else { b"invalid_request_error" },
        ),
        (
            api::Error::Unauthorized,
            Failure::Unauthorized,
            401,
            if codex { b"invalid_api_key" } else { b"authentication_error" },
        ),
    ];
    if codex {
        rows.push((api::Error::Unsupported, Failure::Unsupported, 400, b"subscription_sharing_unsupported_capability"));
    }
    rows
}

#[test]
fn each_scripted_class_reaches_the_client_as_scripted() {
    for provider in [Provider::OpenAiCodex, Provider::Anthropic] {
        for (error, class, status, code) in class_rows(provider) {
            let mut world = exchange(
                provider,
                failing(
                    error,
                    api::Report {
                        code: Box::new([]),
                        request: b"req_42".as_slice().into(),
                        field: b"top_p".as_slice().into(),
                    },
                ),
            );
            world.start();
            world.run();
            assert_eq!(
                terminal(&world),
                (
                    class,
                    Report {
                        code: code.into(),
                        request: b"req_42".as_slice().into(),
                        field: if provider == Provider::Anthropic { Box::new([]) } else { b"top_p".as_slice().into() },
                    },
                    client::Evidence::Response { status }
                ),
                "{provider:?} {error:?}"
            );
        }
    }
}

#[test]
fn scripted_evidence_reaches_the_report_or_arrives_empty() {
    for provider in [Provider::OpenAiCodex, Provider::Anthropic] {
        for length in [0, 5, 128, 129] {
            let value: Box<[u8]> = vec![b'x'; length].into();
            let mut world = exchange(
                provider,
                failing(
                    api::Error::Overloaded,
                    api::Report { code: value.clone(), request: value.clone(), field: value.clone() },
                ),
            );
            world.start();
            world.run();
            let expected = if length > 128 { Box::new([]) as Box<[u8]> } else { value };
            let code: &[u8] = if length == 0 { b"overloaded_error" } else { &expected };
            let request: &[u8] = if length == 0 { b"req_1" } else { &expected };
            assert_eq!(
                terminal(&world).1,
                Report {
                    code: code.into(),
                    request: request.into(),
                    field: if provider == Provider::Anthropic { Box::new([]) } else { expected }
                },
                "{provider:?} {length}"
            );
        }
    }
}

#[test]
fn a_reset_reaches_exhausted_in_whole_seconds_rounded_up() {
    for provider in [Provider::OpenAiCodex, Provider::Anthropic] {
        let mut world = exchange(
            provider,
            failing(api::Error::Exhausted { retry_after: Some(Duration::from_millis(1500)) }, api::Report::none()),
        );
        world.at(Time::ZERO, Wall::from_nanos(100_700_000_000));
        world.start();
        world.run();
        assert_eq!(
            terminal(&world).0,
            Failure::Exhausted { retry_after: Some(Duration::from_secs(2)) },
            "{provider:?}"
        );
        assert!(!world.responses.windows(b"retry-after".len()).any(|bytes| bytes.eq_ignore_ascii_case(b"retry-after")));
    }
}

#[test]
fn exhausted_without_a_reset_reaches_the_client_without_one() {
    for provider in [Provider::OpenAiCodex, Provider::Anthropic] {
        let mut world = exchange(provider, failing(api::Error::Exhausted { retry_after: None }, api::Report::none()));
        world.start();
        world.run();
        assert_eq!(terminal(&world).0, Failure::Exhausted { retry_after: None }, "{provider:?}");
    }
}

#[test]
fn every_response_head_carries_its_routes_request_id() {
    for (provider, name) in
        [(Provider::OpenAiCodex, b"x-oai-request-id: req_1".as_slice()), (Provider::Anthropic, b"request-id: req_1")]
    {
        for lines in [
            failing(api::Error::Overloaded, api::Report::none()),
            Box::new([Line::Text { text: b"answer".as_slice().into() }]) as Box<[Line]>,
        ] {
            let mut world = exchange(provider, lines);
            world.start();
            world.run();
            assert!(world.responses.windows(name.len()).any(|bytes| bytes.eq_ignore_ascii_case(name)), "{provider:?}");
        }
    }
}

#[test]
fn a_seeded_failure_carries_its_classs_code() {
    for provider in [Provider::OpenAiCodex, Provider::Anthropic] {
        let mut world = exchange(provider, Box::new([Line::Text { text: b"unused".as_slice().into() }]));
        let mut chances = skein_llm_world::fake::config();
        chances.overloaded = 1000;
        world.failure_chances(&chances);
        world.start();
        world.run();
        assert_eq!(terminal(&world).1.code.as_ref(), b"overloaded_error", "{provider:?}");
    }
}

fn stream(world: &Exchange) -> Vec<Result<&[u8], Failure>> {
    world
        .seen
        .iter()
        .filter_map(|event| match event {
            client::Event::Delta { delta: skein_llm::Delta::Text { text, .. }, .. } => Some(Ok(text.as_ref())),
            client::Event::Failed { failure, .. } => Some(Err(*failure)),
            client::Event::Delta { .. }
            | client::Event::Block { .. }
            | client::Event::Completed { .. }
            | client::Event::Cancelled { .. }
            | client::Event::Reusable
            | client::Event::Close
            | client::Event::Closed => None,
        })
        .collect()
}

fn after_text(error: api::Error) -> Box<[Line]> {
    Box::new([
        Line::Text { text: b"before".as_slice().into() },
        Line::Fail {
            error,
            report: api::Report {
                code: b"overloaded_error".as_slice().into(),
                request: b"req_42".as_slice().into(),
                field: b"top_p".as_slice().into(),
            },
            credentials: Box::new([0]),
        },
    ])
}

#[test]
fn a_failure_after_text_ends_the_stream_as_its_route_fails() {
    let rows: [(Provider, &[u8]); 2] = [(Provider::OpenAiCodex, b"top_p"), (Provider::Anthropic, b"")];
    for (provider, field) in rows {
        let mut world = exchange(provider, after_text(api::Error::Overloaded));
        world.start();
        world.run();
        assert_eq!(
            (stream(&world), terminal(&world)),
            (
                vec![Ok(b"before".as_slice()), Err(Failure::Overloaded)],
                (
                    Failure::Overloaded,
                    Report {
                        code: b"overloaded_error".as_slice().into(),
                        request: b"req_42".as_slice().into(),
                        field: field.into()
                    },
                    client::Evidence::Response { status: 200 }
                ),
            ),
            "{provider:?}"
        );
    }
}

#[test]
fn a_failure_after_a_hold_waits_for_its_release() {
    let mut world = exchange(
        Provider::OpenAiCodex,
        Box::new([
            Line::Pieces {
                pieces: Box::new([
                    api::Piece { text: b"be".as_slice().into(), hold: None },
                    api::Piece { text: b"fore".as_slice().into(), hold: Some(b"h".as_slice().into()) },
                ]),
            },
            Line::Fail { error: api::Error::Refused, report: api::Report::none(), credentials: Box::new([0]) },
        ]),
    );
    world.start();
    while world.tick(true) {}
    assert_eq!(stream(&world), vec![Ok(b"be".as_slice())]);
    world.release(b"h");
    world.run();
    assert_eq!(stream(&world), vec![Ok(b"be".as_slice()), Ok(b"fore".as_slice()), Err(Failure::Refused)]);
}

#[test]
fn an_exhausted_reset_after_text_reaches_the_client_on_the_codex_route() {
    let mut lines = after_text(api::Error::Exhausted { retry_after: Some(Duration::from_millis(1500)) });
    let Line::Fail { report, .. } = &mut lines[1] else { panic!("fixture's failing line") };
    report.code = Box::new([]);
    let mut world = exchange(Provider::OpenAiCodex, lines);
    world.start();
    world.run();
    assert_eq!(
        stream(&world),
        vec![Ok(b"before".as_slice()), Err(Failure::Exhausted { retry_after: Some(Duration::from_secs(2)) })]
    );
    assert!(!world.responses.windows(b"retry-after".len()).any(|bytes| bytes.eq_ignore_ascii_case(b"retry-after")));
}
