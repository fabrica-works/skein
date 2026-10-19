//! Credential lists and account stories through the shared Client and byte peer.
use skein_fake_llm_domain::api::{self, Finish, Line, Script, Turn};
use skein_fake_llm_protocol::provider::{self, Presented};
use skein_lib::Duration;
use skein_llm::{Block, Credential, Failure, Provider, Report, Scheme, client};
use skein_llm_world::{anthropic_call, fake::Exchange, limits, public_call};

const CUE: &[u8] = b"accounts-script";
const ANSWER: &[u8] = b"answered";

fn credential(index: u32) -> Credential {
    match index {
        0 => Credential::key(b"account-key".as_slice().into()),
        1 => Credential::bearer(b"account-bearer".as_slice().into()),
        _ => panic!("the fixture has two credentials"),
    }
}

fn issued(index: u32) -> Credential {
    let value: &[u8] = match index {
        0 => b"issued-first",
        1 => b"issued-second",
        2 => b"issued-third",
        _ => panic!("the fixture has three issued tokens"),
    };
    Credential::bearer(value.into())
}

fn scripts(lines: Vec<Line>) -> Box<[Script]> {
    Box::new([Script {
        cue: CUE.into(),
        turns: Box::new([Turn { lines: lines.into_boxed_slice(), finish: Finish::Stop, tokens: 1 }]),
    }])
}

fn answered() -> Line {
    Line::Text { text: ANSWER.into() }
}

fn exchange(provider: Provider, sent: Option<Credential>, first: Option<Credential>, lines: Vec<Line>) -> Exchange {
    let mut input = match provider {
        Provider::Anthropic => anthropic_call(1),
        Provider::OpenAi => public_call(1),
        Provider::OpenAiCodex => panic!("the story uses public or Anthropic routes"),
    };
    input.credential = sent;
    input.prompt.instructions = CUE.into();
    let endpoint = input.endpoint.clone();
    let bounds = limits();
    let machine = client::Client::prepare(input, &bounds).expect("account Client admission");
    Exchange::prepared(machine, endpoint, first, bounds, scripts(lines))
}

fn world(provider: Provider, sent: u32, lines: Vec<Line>) -> Exchange {
    let mut world = exchange(provider, Some(credential(sent)), Some(credential(0)), lines);
    assert_eq!(world.accept(credential(1)), Ok(1));
    world
}

#[derive(Debug, PartialEq, Eq)]
enum Terminal {
    Answer(Vec<u8>),
    Failed { class: Failure, report: Report },
}

fn terminal(world: &Exchange) -> Terminal {
    world
        .seen
        .iter()
        .find_map(|event| match event {
            client::Event::Completed { completion, .. } => {
                let [Block::Text { text, .. }] = completion.content.as_ref() else {
                    panic!("one scripted text answer")
                };
                Some(Terminal::Answer(text.to_vec()))
            }
            client::Event::Failed { failure, report, .. } => {
                Some(Terminal::Failed { class: *failure, report: report.clone() })
            }
            client::Event::Block { .. }
            | client::Event::Delta { .. }
            | client::Event::Cancelled { .. }
            | client::Event::Reusable
            | client::Event::Close
            | client::Event::Closed => None,
        })
        .expect("one Client terminal")
}

fn failed(class: Failure, code: &[u8], request: &[u8], field: &[u8]) -> Terminal {
    Terminal::Failed { class, report: Report { code: code.into(), request: request.into(), field: field.into() } }
}

fn failing(error: api::Error, indices: &[u32], field: &[u8]) -> Line {
    Line::Fail {
        error,
        report: api::Report { code: Box::new([]), request: b"script-request".as_slice().into(), field: field.into() },
        credentials: indices.into(),
    }
}

#[test]
fn both_schemes_on_one_endpoint_are_answered_by_index() {
    for provider in [Provider::Anthropic, Provider::OpenAi] {
        for (sent, scheme) in [(0, Scheme::Key), (1, Scheme::Bearer)] {
            let mut world = world(provider, sent, vec![answered()]);
            world.start();
            world.run();
            assert_eq!(world.presented, [Presented { route: provider, scheme: Some(scheme), index: Some(sent) }]);
            assert_eq!(terminal(&world), Terminal::Answer(ANSWER.to_vec()));
        }
    }
}

#[test]
fn a_failing_line_naming_one_credential_lets_another_through() {
    for (sent, expected) in [
        (0, failed(Failure::Unsupported, b"subscription_sharing_unsupported_capability", b"script-request", b"top_p")),
        (1, Terminal::Answer(ANSWER.to_vec())),
    ] {
        let mut world =
            world(Provider::OpenAi, sent, vec![failing(api::Error::Unsupported, &[0], b"top_p"), answered()]);
        world.start();
        world.run();
        assert_eq!(terminal(&world), expected, "index {sent}");
        assert_eq!(world.presented[0].index, Some(sent));
    }
}

#[test]
fn a_call_with_no_credential_is_observed_as_the_scheme_none() {
    for (provider, code) in
        [(Provider::Anthropic, b"authentication_error".as_slice()), (Provider::OpenAi, b"invalid_api_key".as_slice())]
    {
        for (first, expected) in [
            (None, Terminal::Answer(ANSWER.to_vec())),
            (Some(credential(0)), failed(Failure::Unauthorized, code, b"req_1", b"")),
        ] {
            let mut world = exchange(provider, None, first, vec![answered()]);
            world.start();
            world.run();
            assert_eq!(world.presented, [Presented { route: provider, scheme: None, index: None }]);
            assert_eq!(terminal(&world), expected, "{provider:?}");
        }
    }
}

struct UnknownRow {
    provider: Provider,
    sent: u32,
    scheme: Scheme,
    code: &'static [u8],
}

fn unknown_rows() -> [UnknownRow; 4] {
    [
        UnknownRow { provider: Provider::Anthropic, sent: 0, scheme: Scheme::Key, code: b"authentication_error" },
        UnknownRow { provider: Provider::Anthropic, sent: 1, scheme: Scheme::Bearer, code: b"authentication_error" },
        UnknownRow { provider: Provider::OpenAi, sent: 0, scheme: Scheme::Bearer, code: b"invalid_api_key" },
        UnknownRow { provider: Provider::OpenAi, sent: 1, scheme: Scheme::Bearer, code: b"invalid_api_key" },
    ]
}

#[test]
fn a_credential_the_peer_does_not_hold_is_unauthorized() {
    for UnknownRow { provider, sent, scheme, code } in unknown_rows() {
        let mut world = exchange(provider, Some(credential(sent)), Some(issued(0)), vec![answered()]);
        world.start();
        world.run();
        assert_eq!(world.presented, [Presented { route: provider, scheme: Some(scheme), index: None }]);
        assert_eq!(terminal(&world), failed(Failure::Unauthorized, code, b"req_1", b""));
    }
}

struct ObservationRow {
    sent: Option<Credential>,
    first: Option<Credential>,
    queries: Vec<Option<u32>>,
}

fn observation_rows() -> [ObservationRow; 6] {
    [
        ObservationRow { sent: Some(credential(0)), first: Some(credential(0)), queries: vec![Some(0)] },
        ObservationRow { sent: Some(credential(1)), first: Some(credential(1)), queries: vec![Some(0)] },
        ObservationRow { sent: Some(credential(0)), first: Some(issued(0)), queries: vec![] },
        ObservationRow { sent: Some(credential(1)), first: Some(issued(0)), queries: vec![] },
        ObservationRow { sent: None, first: None, queries: vec![None] },
        ObservationRow { sent: None, first: Some(credential(0)), queries: vec![] },
    ]
}

#[test]
fn no_observation_holds_a_credentials_value() {
    for provider in [Provider::Anthropic, Provider::OpenAi] {
        for ObservationRow { sent, first, queries } in observation_rows() {
            let mut world = exchange(provider, sent, first, vec![answered()]);
            world.start();
            world.run();
            let fields: Vec<_> = world
                .heads
                .iter()
                .flat_map(|head| head.iter())
                .filter(|header| {
                    header.is(b"authorization") || header.is(b"x-api-key") || header.is(b"chatgpt-account-id")
                })
                .map(|header| header.name.as_ref())
                .collect();
            let indices: Vec<_> = world.queries.iter().map(|query| query.credential).collect();
            assert_eq!((fields, indices), (Vec::<&[u8]>::new(), queries));
        }
    }
}

fn issued_world(provider: Provider, sent: u32, lines: Vec<Line>) -> Exchange {
    let mut world = exchange(provider, Some(issued(sent)), Some(issued(0)), lines);
    for index in 1..3 {
        assert_eq!(world.accept(issued(index)), Ok(index));
    }
    world
}

#[test]
fn an_issuers_tokens_are_accepted_as_bearers_before_serving() {
    for provider in [Provider::Anthropic, Provider::OpenAi] {
        for sent in 0..3 {
            let mut world = issued_world(provider, sent, vec![answered()]);
            world.start();
            world.run();
            assert_eq!(
                world.presented,
                [Presented { route: provider, scheme: Some(Scheme::Bearer), index: Some(sent) }]
            );
            assert_eq!(terminal(&world), Terminal::Answer(ANSWER.to_vec()));
            assert_eq!(world.accept(issued(0)), Err(provider::Error::Limits));
        }
    }
}

#[test]
fn a_revoked_token_stays_accepted_until_a_line_names_it() {
    for (provider, code) in
        [(Provider::Anthropic, b"authentication_error".as_slice()), (Provider::OpenAi, b"invalid_api_key".as_slice())]
    {
        for (sent, lines, expected) in [
            (1, vec![answered()], Terminal::Answer(ANSWER.to_vec())),
            (
                1,
                vec![failing(api::Error::Unauthorized, &[1], b""), answered()],
                failed(Failure::Unauthorized, code, b"script-request", b""),
            ),
            (2, vec![failing(api::Error::Unauthorized, &[1], b""), answered()], Terminal::Answer(ANSWER.to_vec())),
        ] {
            let mut world = issued_world(provider, sent, lines);
            world.start();
            world.run();
            assert_eq!(
                world.presented,
                [Presented { route: provider, scheme: Some(Scheme::Bearer), index: Some(sent) }]
            );
            assert_eq!(terminal(&world), expected, "{provider:?} index {sent}");
        }
    }
}

#[test]
fn the_public_routes_failures_reach_the_client_as_scripted() {
    let delay = Duration::from_secs(2);
    for (error, expected) in [
        (
            api::Error::Exhausted { retry_after: None },
            failed(
                Failure::Exhausted { retry_after: None },
                b"subscription_sharing_usage_limit_exceeded",
                b"script-request",
                b"top_p",
            ),
        ),
        (
            api::Error::Exhausted { retry_after: Some(delay) },
            failed(
                Failure::Exhausted { retry_after: Some(delay) },
                b"subscription_sharing_usage_limit_exceeded",
                b"script-request",
                b"top_p",
            ),
        ),
        (
            api::Error::RateLimited { retry_after: delay },
            failed(Failure::RateLimited { retry_after: delay }, b"rate_limit_exceeded", b"script-request", b"top_p"),
        ),
        (
            api::Error::Refused,
            failed(Failure::Refused, b"subscription_sharing_user_not_eligible", b"script-request", b"top_p"),
        ),
        (
            api::Error::Unsupported,
            failed(Failure::Unsupported, b"subscription_sharing_unsupported_capability", b"script-request", b"top_p"),
        ),
    ] {
        let mut world = world(Provider::OpenAi, 1, vec![failing(error, &[1], b"top_p")]);
        world.start();
        world.run();
        assert_eq!(terminal(&world), expected, "{error:?}");
    }
}

#[test]
fn a_failure_after_text_ends_the_stream_on_the_public_route() {
    let mut world = world(Provider::OpenAi, 1, vec![answered(), failing(api::Error::Unsupported, &[1], b"top_p")]);
    world.start();
    world.run();
    let text: Vec<_> = world
        .seen
        .iter()
        .filter_map(|event| match event {
            client::Event::Delta { delta: skein_llm::Delta::Text { text, .. }, .. } => Some(text.as_ref()),
            client::Event::Delta { .. }
            | client::Event::Block { .. }
            | client::Event::Completed { .. }
            | client::Event::Failed { .. }
            | client::Event::Cancelled { .. }
            | client::Event::Reusable
            | client::Event::Close
            | client::Event::Closed => None,
        })
        .flatten()
        .copied()
        .collect();
    assert_eq!(text, ANSWER);
    assert_eq!(
        terminal(&world),
        failed(Failure::Unsupported, b"subscription_sharing_unsupported_capability", b"script-request", b"top_p")
    );
}
