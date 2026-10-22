//! Synthetic public Responses cases implement llm.md 4.8's wire contract.
use skein_json::Kind;
use skein_lib::Rng;
use skein_llm::{Call, Credential, Error, Failure, Json, Provider, Stop, ToolChoice, client};
use skein_llm_world::{World, events, fields_holding, limits_for, public_call, rejection, response, run_call};

fn limits() -> client::Limits {
    limits_for(Provider::OpenAi)
}

fn sent(input: Call) -> World {
    run_call(input, limits(), response(401, "", b"{}", false), 1)
}

fn body(world: &World) -> &[u8] {
    let head = world.sent.windows(4).position(|bytes| bytes == b"\r\n\r\n").expect("request head") + 4;
    &world.sent[head..]
}

fn names(world: &World) -> Vec<&str> {
    std::str::from_utf8(&world.sent)
        .expect("fixture request")
        .split("\r\n\r\n")
        .next()
        .expect("request head")
        .lines()
        .filter_map(|line| line.split_once(':').map(|(name, _)| name))
        .collect()
}

fn root_fields(json: &Json) -> Vec<(&[u8], Kind)> {
    let document = json.document();
    let mut depth = 0_u32;
    let mut fields = Vec::new();
    for index in 0..document.len() {
        let record = document.token(index).expect("document record");
        match record.kind {
            Kind::ObjectStart | Kind::ArrayStart => depth += 1,
            Kind::ObjectEnd | Kind::ArrayEnd => depth -= 1,
            Kind::Key if depth == 1 => {
                fields.push((document.text(record).expect("key"), document.token(index + 1).expect("value").kind));
            }
            Kind::Key | Kind::String | Kind::Number | Kind::True | Kind::False | Kind::Null | Kind::Long => {}
        }
    }
    fields
}

#[test]
fn a_key_and_a_token_send_byte_identical_bodies() {
    let [key_body, token_body] = [Credential::key, Credential::bearer].map(|make| {
        let mut input = public_call(1);
        input.credential = Some(make(b"secret-test-token".to_vec().into()));
        let world = sent(input);
        assert_eq!(fields_holding(&world.sent, b"secret-test-token"), ["Authorization"]);
        body(&world).to_vec()
    });
    assert_eq!(key_body, token_body);
}

#[test]
fn the_public_route_sends_affinity_in_the_body_alone() {
    for (affinity, expected) in
        [(public_call(1).prompt.affinity, Some(b"42424242-4242-4242-4242-424242424242".as_slice())), (None, None)]
    {
        let mut input = public_call(1);
        input.prompt.affinity = affinity;
        let world = sent(input);
        let json = Json::from_bytes(body(&world), &limits().native().document()).expect("request document");
        let document = json.document();
        let key = (0..document.len()).find_map(|index| {
            let record = document.token(index).expect("request record");
            if record.kind == Kind::Key && document.text(record) == Some(b"prompt_cache_key") {
                document.text(document.token(index + 1).expect("cache key"))
            } else {
                None
            }
        });
        assert_eq!(key, expected);
        for name in ["session-id", "thread-id", "chatgpt-account-id"] {
            assert!(!names(&world).contains(&name), "{name}");
        }
    }
}

#[test]
fn a_call_without_a_credential_sends_no_authorization() {
    let mut input = public_call(1);
    input.credential = None;
    let measured = client::measure(&input.prompt, None, &input.endpoint, &limits()).expect("public admission");
    let world = sent(input);
    for name in ["Authorization", "x-api-key", "chatgpt-account-id"] {
        assert!(!names(&world).contains(&name), "{name}");
    }
    assert_eq!(usize::try_from(measured.body).expect("body length"), body(&world).len());
    assert_eq!(usize::try_from(measured.head).expect("head length"), world.sent.len() - body(&world).len());
}

fn drawn_call(seed: u64) -> Call {
    // These draws stay within the tiny declaration's input window.
    const DRAWN_TEXT: u64 = 16;
    let mut rng = Rng::new(seed);
    let mut input = public_call(seed);
    input.prompt.instructions = vec![b'x'; usize::try_from(rng.below(DRAWN_TEXT)).expect("tiny draw")].into();
    input.prompt.tools = Box::new([skein_llm::Tool {
        name: b"read".as_slice().into(),
        description: b"Read a file".as_slice().into(),
        schema: Json::from_bytes(b"{}", &limits().native().document()).expect("tool schema"),
    }]);
    input.prompt.choice = match rng.below(3) {
        0 => ToolChoice::Auto,
        1 => ToolChoice::None,
        _ => ToolChoice::Only(Box::new([b"read".as_slice().into()])),
    };
    if rng.chance(500) {
        input.prompt.reasoning_effort = Some(b"low".as_slice().into());
    }
    if rng.chance(500) {
        input.prompt.affinity = None;
    }
    input
}

#[test]
fn no_drawn_prompt_carries_a_refused_field() {
    // llm.md 4.8's literal refused fields, plus the forbidden retained state.
    const REFUSED: &[&[u8]] = &[
        b"background",
        b"conversation",
        b"max_output_tokens",
        b"max_tool_calls",
        b"metadata",
        b"moderation",
        b"multi_agent",
        b"prompt",
        b"prompt_cache_retention",
        b"safety_identifier",
        b"temperature",
        b"top_logprobs",
        b"top_p",
        b"truncation",
        b"user",
        b"previous_response_id",
    ];
    // Eight deterministic prompts sample the optional request fields under 0.1 s.
    const DRAWN_PROMPTS: u64 = 8;
    for seed in 1..=DRAWN_PROMPTS {
        let world = sent(drawn_call(seed));
        let json = Json::from_bytes(body(&world), &limits().native().document()).expect("request document");
        let fields = root_fields(&json);
        for refused in REFUSED {
            assert!(!fields.iter().any(|(name, _)| name == refused), "{seed}: {refused:?}");
        }
        assert!(fields.contains(&(b"store".as_slice(), Kind::False)), "{seed}");
        assert!(fields.contains(&(b"stream".as_slice(), Kind::True)), "{seed}");
    }
}

#[test]
fn an_explicit_output_cap_is_unsupported() {
    let mut input = public_call(1);
    input.prompt.max_output_tokens = Some(1);
    assert_eq!(rejection(input, &limits()), Some(Error::Unsupported));
}

#[test]
fn each_terminal_event_ends_the_call_as_the_route_says() {
    for (document, expected) in [
        (r#"{"type":"response.completed","response":{"status":"completed"}}"#, Ok(Stop::EndTurn)),
        (
            r#"{"type":"response.incomplete","response":{"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"}}}"#,
            Ok(Stop::MaxTokens),
        ),
        (
            r#"{"type":"response.incomplete","response":{"status":"incomplete","incomplete_details":{"reason":"content_filter"}}}"#,
            Ok(Stop::Refusal),
        ),
        (r#"{"type":"response.done","response":{"status":"completed"}}"#, Err(Failure::Protocol)),
        (r#"{"type":"response.created","response":{}}"#, Err(Failure::Protocol)),
    ] {
        let world = run_call(
            public_call(1),
            limits(),
            response(200, "Content-Type: text/event-stream\r\n", &events(&[document]), false),
            1,
        );
        world.assert_once();
        let terminal = world.seen.iter().find_map(|event| match event {
            client::Event::Completed { completion, .. } => Some(Ok(completion.stop)),
            client::Event::Failed { failure, .. } => Some(Err(*failure)),
            client::Event::Delta { .. }
            | client::Event::Block { .. }
            | client::Event::Cancelled { .. }
            | client::Event::Reusable
            | client::Event::Close
            | client::Event::Closed => None,
        });
        assert_eq!(terminal, Some(expected), "{document}");
    }
}

#[test]
fn the_public_request_id_reaches_the_report() {
    let world = run_call(
        public_call(1),
        limits(),
        response(401, "x-request-id: public-fixture\r\nx-oai-request-id: codex-fixture\r\n", b"{}", false),
        1,
    );
    let requests: Vec<_> =
        world
            .seen
            .iter()
            .filter_map(|event| {
                if let client::Event::Failed { report, .. } = event { Some(report.request.as_ref()) } else { None }
            })
            .collect();
    assert_eq!(requests, [b"public-fixture".as_slice()]);
}

fn plan_response_without_type_world(headers: &str) -> skein_llm_world::World {
    let rest = [
        b"data: {\"type\":\"response.created\"}\n\n".as_slice(),
        &skein_llm_world::events(&[
            skein_llm_world::TEXT_ADDED,
            skein_llm_world::TEXT_DELTA,
            skein_llm_world::TEXT_DONE,
            skein_llm_world::TERMINAL,
        ]),
    ]
    .concat();
    // The captured response's first HTTP chunk is exactly 24 bytes (hex 18).
    let mut wire =
        format!("HTTP/1.1 200 OK\r\n{headers}Transfer-Encoding: chunked\r\n\r\n18\r\nevent: response.created\n\r\n")
            .into_bytes();
    wire.extend_from_slice(format!("{:x}\r\n", rest.len()).as_bytes());
    wire.extend_from_slice(&rest);
    wire.extend_from_slice(b"\r\n0\r\n\r\n");
    let mut world = skein_llm_world::World::new(
        skein_llm_world::public_call(1),
        skein_llm_world::limits_for(skein_llm::Provider::OpenAi),
        wire,
        545,
    );
    world.fragmentation(1, 3);
    world.request(skein_llm::client::Request::Start);
    world.run();
    world.assert_once();
    world
}

#[test]
fn public_plan_accepts_event_stream_without_content_type() {
    // The handwritten chunked body must reach its terminal under one-byte cuts.
    let world = plan_response_without_type_world("");
    assert!(world.seen.iter().any(|event| matches!(
        event,
        skein_llm::client::Event::Completed { completion, .. }
            if completion.stop == skein_llm::Stop::EndTurn
    )));
}

#[test]
fn public_plan_still_accepts_case_and_parameters() {
    let world = plan_response_without_type_world("cOnTeNt-TyPe: TeXt/EvEnT-StReAm ; charset=utf-8\r\n");
    assert!(world.seen.iter().any(|event| matches!(event, skein_llm::client::Event::Completed { .. })));
}

#[test]
fn public_plan_still_rejects_explicit_wrong_and_duplicate_types() {
    for (headers, expected) in [
        ("Content-Type: application/json\r\n", "response is not an event stream (content-type: application/json)"),
        (
            "Content-Type: text/event-stream\r\ncontent-type: text/event-stream\r\n",
            "response is not an event stream (content-type: duplicate)",
        ),
    ] {
        let world = plan_response_without_type_world(headers);
        assert!(world.seen.iter().any(|event| matches!(
            event,
            skein_llm::client::Event::Failed { failure: skein_llm::Failure::Protocol, detail, .. }
                if detail.as_ref() == expected.as_bytes()
        )));
    }
}
