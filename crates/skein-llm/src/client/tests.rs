//! A cut or malformed HTTP exchange, as the failure a caller retries or not
//! (llm.md, section 2.5).
#![expect(clippy::wildcard_enum_match_arm, reason = "ordinary tests make focused partial-variant assertions")]
#![expect(clippy::disallowed_types, reason = "ordinary test code collects the events in Vec")]
#![expect(clippy::disallowed_methods, reason = "tests slice and search the bytes they deliver")]
#![expect(clippy::arithmetic_side_effects, reason = "the test's trusted cursor uses ordinary arithmetic")]

use super::*;
use skein_lib::{Time, Wall};

fn limits() -> Limits {
    Limits {
        http: http::Limits { request: 4096, head: 4096, headers: 32, read: 256, send: 4096 },
        sse: sse::Limits { line: 4096, event: 8192, field: 128, chunk: 128 },
        request: 8192,
        retained: 8192,
        strings: 4096,
        depth: 32,
        tokens: 1024,
        output_items: 16,
        input: 2048,
        reasoning: 2048,
        answer: 8192,
        detail_bytes: 256,
        tools: 16,
        history_items: 16,
        metadata: 2048,
        receiving: 1_048_576,
        skip: 8192,
        error_bytes: 4096,
        drop_reasoning: false,
        declared_output_tokens: 4096,
    }
}

fn call() -> Call {
    Call {
        owner: Token::new(7),
        prompt: Prompt {
            model: bytes::copy_of(b"test-model"),
            instructions: Box::new([]),
            tools: Box::new([]),
            choice: crate::ToolChoice::Auto,
            messages: Box::new([crate::Message {
                role: crate::Role::User,
                content: Box::new([Block::Text { text: bytes::copy_of(b"hi"), replay: None }]),
            }]),
            reasoning_effort: None,
            affinity: None,
            max_output_tokens: None,
        },
        endpoint: Endpoint::codex(),
        credential: Some(Credential::codex(bytes::copy_of(b"secret"), bytes::copy_of(b"account"))),
    }
}

/// One call, started and sent, with the events it has raised so far.
struct World {
    client: Client,
    env: Env<Limits>,
    above: Queue<Event>,
    below: Queue<Down>,
    events: Vec<Event>,
    read: Read,
    sent: Vec<u8>,
}

impl World {
    fn started() -> World {
        World::started_with(call())
    }

    fn started_with(call: Call) -> World {
        World::started_within(call, limits())
    }

    fn started_within(call: Call, limits: Limits) -> World {
        let client = Client::prepare(call, &limits).expect("the call is admitted");
        let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
        let mut world = World {
            client,
            env,
            above: Queue::with_capacity(MAX_OUT.above),
            below: Queue::with_capacity(MAX_OUT.below),
            events: Vec::new(),
            read: Read::Nothing,
            sent: Vec::new(),
        };
        world.down(Request::Start);
        world
    }

    /// Run the client to rest, granting the room it demands as io would and
    /// noting whether it reads.
    fn settle(&mut self) {
        loop {
            while self.client.has_work() {
                resume(&mut self.client, &self.env, &mut self.above, &mut self.below);
            }
            while let Some(event) = self.above.pop() {
                self.events.push(event);
            }
            let mut room = false;
            while let Some(request) = self.below.pop() {
                match request {
                    Down::Demand { read, room: wanted } => {
                        room = wanted > 0;
                        self.read = if room { Read::Nothing } else { read };
                    }
                    Down::Send(bytes) => self.sent.extend_from_slice(&bytes),
                    Down::Finish => {}
                }
            }
            if !room || self.client.state == State::Closing || self.client.state == State::Closed {
                return;
            }
            up(&mut self.client, &self.env, Up::Room, &mut self.above, &mut self.below);
        }
    }

    fn down(&mut self, request: Request) {
        down(&mut self.client, &self.env, request, &mut self.above, &mut self.below);
        self.settle();
    }

    fn up(&mut self, event: Up) {
        up(&mut self.client, &self.env, event, &mut self.above, &mut self.below);
        self.settle();
    }

    /// Deliver `data` as the client's reads take it.
    fn feed(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            let wanted = match self.read {
                Read::Nothing => panic!("the client reads nothing, with bytes left to deliver"),
                Read::Fill(count) => usize::try_from(count).unwrap(),
                Read::Scan { until, max } => {
                    let max = usize::try_from(max).unwrap();
                    match bytes::find(&data[..data.len().min(max)], until.as_bytes()) {
                        Some(offset) => offset + until.as_bytes().len(),
                        None => max,
                    }
                }
                Read::Line { max } => {
                    let max = usize::try_from(max).unwrap();
                    data[..data.len().min(max)]
                        .iter()
                        .position(|byte| *byte == b'\r' || *byte == b'\n')
                        .map_or(max, |at| at + 1)
                }
                Read::Arrived { max } => usize::try_from(max).unwrap(),
            }
            .min(data.len());
            let (now, later) = data.split_at(wanted);
            self.up(Up::Bytes(bytes::copy_of(now)));
            data = later;
        }
    }

    fn failure(&self) -> (Failure, Evidence, &[u8]) {
        let mut failed = self.events.iter().filter_map(|event| match event {
            Event::Failed { failure, evidence, detail, .. } => Some((*failure, *evidence, &**detail)),
            _ => None,
        });
        let first = failed.next().expect("the call failed");
        assert_eq!(failed.next(), None, "one terminal failure");
        first
    }
}

const HEAD: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 1000\r\n\r\n";

#[test]
fn a_connection_closed_before_a_response_is_unavailable_possibly_sent() {
    let mut world = World::started();
    world.up(Up::End);
    let (failure, evidence, detail) = world.failure();
    assert_eq!(failure, Failure::Unavailable);
    assert_eq!(evidence, Evidence::Unknown);
    assert_eq!(detail, b"connection closed before a response");
}

#[test]
fn a_connection_cut_within_a_body_is_unavailable_with_a_response_received() {
    let mut world = World::started();
    world.down(Request::Next);
    world.feed(HEAD);
    world.feed(b"data: {");
    world.up(Up::End);
    let (failure, evidence, detail) = world.failure();
    assert_eq!(failure, Failure::Unavailable);
    assert_eq!(evidence, Evidence::Response { status: 200 });
    assert_eq!(detail, b"connection closed within the response");
}

#[test]
fn a_stream_failing_after_the_request_went_out_is_unavailable() {
    let mut world = World::started();
    world.up(Up::Failed(skein_lib::stream::Fault::Reset));
    let (failure, evidence, detail) = world.failure();
    assert_eq!(failure, Failure::Unavailable);
    assert_eq!(evidence, Evidence::Unknown);
    assert_eq!(detail, b"connection failed");
}

#[test]
fn a_malformed_status_line_is_still_a_protocol_failure() {
    let mut world = World::started();
    world.feed(b"HTTP/x.y 200 OK\r\n\r\n");
    let (failure, _, detail) = world.failure();
    assert_eq!(failure, Failure::Protocol);
    assert_eq!(detail, b"malformed HTTP response: status line");
}

/// The sent request's head, as text, and its body.
fn split(sent: &[u8]) -> (&str, &[u8]) {
    let at = bytes::find(sent, b"\r\n\r\n").expect("a whole head") + 4;
    (core::str::from_utf8(&sent[..at]).expect("a text head"), &sent[at..])
}

fn content_length(head: &str) -> usize {
    let line = head.lines().find_map(|line| line.strip_prefix("Content-Length: ")).expect("a Content-Length");
    line.parse().expect("decimal digits")
}

#[test]
fn a_codex_request_sends_its_rendered_body_gzip_encoded_with_its_length() {
    let world = World::started();
    let (head, body) = split(&world.sent);
    let rendered = translate::encode_request(Provider::OpenAiCodex, &call().prompt, &limits().native()).unwrap();
    let mut room = bytes::zeroed(skein_http::gzip::bound(rendered.len()).unwrap());
    let end = skein_http::gzip::encode(&rendered, &mut room).unwrap();
    assert!(head.contains("\r\nContent-Encoding: gzip\r\n"), "{head}");
    assert_eq!(content_length(head), body.len());
    assert_eq!(body, &room[..end], "the body is the encoder's member of the rendered body");
    let measured = world.client.measured();
    let wide = |value: u32| usize::try_from(value).unwrap();
    assert_eq!(wide(measured.body), rendered.len());
    assert_eq!(wide(measured.compressed), room.len());
    assert_eq!(world.client.request_bytes(), u64::try_from(world.sent.len()).unwrap());
    let digits = |length: usize| length.to_string().len();
    assert_eq!(wide(measured.head), head.len() - digits(body.len()) + digits(room.len()), "measured at the bound");
}

#[test]
fn a_codex_body_compresses_only_while_it_and_its_room_fit_the_request_bound() {
    let rendered = translate::encode_request(Provider::OpenAiCodex, &call().prompt, &limits().native()).unwrap();
    let rendered = u32::try_from(rendered.len()).unwrap();
    // A body within one stored block takes 10 header, 5 block and 8 trailer
    // bytes beside its own in its bound: 2 × rendered + 23 holds both.
    let both = 2 * rendered + 23;
    for (request, gzip) in [(both, true), (both - 1, false)] {
        let mut bounds = limits();
        bounds.request = request;
        let world = World::started_within(call(), bounds);
        let (head, body) = split(&world.sent);
        assert_eq!(head.contains("\r\nContent-Encoding: gzip\r\n"), gzip, "{request}: {head}");
        assert_eq!(content_length(head), body.len());
        let measured = world.client.measured();
        assert_eq!(measured.compressed, if gzip { rendered + 23 } else { 0 }, "{request}");
        if !gzip {
            let as_rendered = translate::encode_request(Provider::OpenAiCodex, &call().prompt, &bounds.native());
            assert_eq!(body, &*as_rendered.unwrap(), "a body past the bound's room is sent as rendered");
            assert_eq!(usize::try_from(measured.head).unwrap(), head.len());
        }
        assert_eq!(world.client.request_bytes(), u64::try_from(world.sent.len()).unwrap());
        // The call stays within the largest reservation and its head within
        // the largest head the endpoint's bounds measure.
        assert!(reservation(measured, &bounds).unwrap() <= largest_reservation(&bounds).unwrap());
        assert!(u64::from(measured.body) + u64::from(measured.compressed) <= u64::from(request));
        let largest_head = request_head(&Endpoint::codex(), &CredentialLimits { value: 6, account_id: 7 }, &bounds);
        assert!(measured.head <= largest_head.unwrap(), "{request}");
    }
}

#[test]
fn an_openai_request_sends_its_rendered_body_as_it_is() {
    let mut input = call();
    input.endpoint = Endpoint::openai();
    input.credential = Some(Credential::key(bytes::copy_of(b"key")));
    let world = World::started_with(input);
    let (head, body) = split(&world.sent);
    let rendered = translate::encode_request(Provider::OpenAi, &call().prompt, &limits().native()).unwrap();
    assert!(!head.to_ascii_lowercase().contains("content-encoding"), "{head}");
    assert_eq!(content_length(head), body.len());
    assert_eq!(body, &*rendered);
    let measured = world.client.measured();
    let wide = |value: u32| usize::try_from(value).unwrap();
    assert_eq!((wide(measured.head), wide(measured.body), measured.compressed), (head.len(), body.len(), 0));
    assert_eq!(world.client.request_bytes(), u64::try_from(world.sent.len()).unwrap());
}

#[test]
fn the_largest_head_and_body_have_an_exact_request_byte_measure() {
    let measured = Measured { head: u32::MAX, body: u32::MAX, compressed: 0 };
    assert_eq!(sent_bytes(measured, u64::from(u32::MAX)), 8_589_934_590);
    let gzip = Measured { head: u32::MAX, body: u32::MAX, compressed: u32::MAX };
    assert_eq!(sent_bytes(gzip, 9), 4_294_967_295 - 10 + 1 + 9, "the sent length's digits replace the bound's");
}

#[test]
fn a_caller_field_cannot_name_the_content_coding() {
    let mut input = call();
    let mut endpoint = Endpoint::codex();
    endpoint.headers = Box::new([Header { name: bytes::copy_of(b"Content-Encoding"), value: bytes::copy_of(b"br") }]);
    input.endpoint = endpoint;
    assert!(Client::prepare(input, &limits()).is_err());
}

/// Dispatch a collected SSE message while the caller has no Next outstanding.
fn dispatch_without_next(world: &mut World, source: &[u8], activity: u64) -> bool {
    let json = openai::Json::from_bytes(source, &world.env.limits.native().document()).unwrap();
    world.client.event_data = EventData::Collected(json.document().clone());
    sse_event(
        &mut world.client,
        sse::Event::Dispatched(sse::Dispatch { name: bytes::copy_of(b"message"), id: Box::new([]) }),
        &world.env,
        &mut world.above,
        &mut world.below,
    );
    assert!(!world.client.asked, "the caller has not demanded its next output");
    assert!(world.above.is_empty(), "dispatch holds output until Next");
    assert_eq!(world.client.activity(), activity, "activity is counted before output is drained: {source:?}");
    let mut completed = false;
    while let Some(output) = world.client.outputs.pop() {
        match output {
            dialect::Output::Completed { .. } => completed = true,
            dialect::Output::Failed { failure, .. } => panic!("expected a valid stream, got {failure:?}"),
            dialect::Output::Part(Err(error)) => panic!("expected a valid part, got {error:?}"),
            dialect::Output::Part(Ok(_))
            | dialect::Output::TextDelta { .. }
            | dialect::Output::ArgumentsDelta { .. }
            | dialect::Output::ReasoningDelta { .. }
            | dialect::Output::Progress => {}
        }
    }
    completed
}

#[test]
fn responses_activity_counts_output_at_dispatch_and_ignores_status_and_extension_events() {
    for endpoint in [Endpoint::openai(), Endpoint::codex()] {
        let mut input = call();
        input.endpoint = endpoint;
        input.credential = Some(Credential::key(bytes::copy_of(b"key")));
        if input.endpoint.provider == Provider::OpenAiCodex {
            input.credential = call().credential;
        }
        let mut world = World::started_with(input);
        world.feed(HEAD);
        assert_eq!(world.client.activity(), 0, "the response head carries no model output");
        for source in [
            br#"{"type":"ping"}"#.as_slice(),
            br#"{"type":"response.created"}"#,
            br#"{"type":"response.in_progress"}"#,
            br#"{"type":"future.extension"}"#,
            br#"{"type":"response.content_part.added"}"#,
            br#"{"type":"response.output_text.done"}"#,
            br#"{"type":"response.content_part.done"}"#,
            br#"{"type":"response.output_item.added","output_index":0,"item":{"id":"msg_1","type":"message"}}"#,
        ] {
            assert!(!dispatch_without_next(&mut world, source, 0));
        }
        assert!(!dispatch_without_next(
            &mut world,
            br#"{"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":"Hello"}"#,
            1,
        ));
        assert!(!dispatch_without_next(&mut world, br#"{"type":"response.in_progress"}"#, 1));
        assert!(!dispatch_without_next(
            &mut world,
            br#"{"type":"response.output_item.done","output_index":0,"item":{"id":"msg_1","type":"message","content":[{"type":"output_text","text":"Hello"}]}}"#,
            2,
        ));
        assert!(dispatch_without_next(
            &mut world,
            br#"{"type":"response.completed","response":{"status":"completed"}}"#,
            3,
        ));
        assert!(!dispatch_without_next(&mut world, br#"{"type":"future.extension"}"#, 3));
    }
}

#[test]
fn anthropic_activity_counts_output_at_dispatch_and_ignores_pings() {
    let mut input = call();
    input.endpoint = Endpoint::anthropic();
    input.credential = Some(Credential::key(bytes::copy_of(b"key")));
    let mut world = World::started_with(input);
    world.feed(HEAD);
    assert_eq!(world.client.activity(), 0);
    for source in [
        br#"{"type":"ping"}"#.as_slice(),
        br#"{"type":"future.extension"}"#,
        br#"{"type":"message_start","message":{"type":"message","role":"assistant","content":[]}}"#,
        br#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
    ] {
        assert!(!dispatch_without_next(&mut world, source, 0));
    }
    assert!(!dispatch_without_next(
        &mut world,
        br#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#,
        1,
    ));
    assert!(!dispatch_without_next(&mut world, br#"{"type":"ping"}"#, 1));
    assert!(!dispatch_without_next(&mut world, br#"{"type":"content_block_stop","index":0}"#, 2));
    assert!(!dispatch_without_next(&mut world, br#"{"type":"message_delta","delta":{"stop_reason":"end_turn"}}"#, 2,));
    assert!(dispatch_without_next(&mut world, br#"{"type":"message_stop"}"#, 3));
    assert!(!dispatch_without_next(&mut world, br#"{"type":"ping"}"#, 3));
}

#[test]
fn a_done_sentinel_counts_the_terminal_it_decodes() {
    let mut world = World::started();
    world.down(Request::Next);
    world.feed(HEAD);
    world.feed(b"data: [DONE]\n\n");
    assert_eq!(world.client.activity(), 1);
    assert_eq!(world.failure().0, Failure::Protocol, "an incomplete stream ends in failure");
}

#[test]
fn a_malformed_message_fails_without_counting_activity() {
    let mut world = World::started();
    world.down(Request::Next);
    world.feed(HEAD);
    world.feed(b"data: {\"type\":4}\n\n");
    assert_eq!(world.client.activity(), 0);
    assert_eq!(world.failure().0, Failure::Protocol);
}
