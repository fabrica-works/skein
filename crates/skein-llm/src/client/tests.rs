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
}

impl World {
    fn started() -> World {
        let limits = limits();
        let client = Client::prepare(call(), &limits).expect("the call is admitted");
        let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
        let mut world = World {
            client,
            env,
            above: Queue::with_capacity(MAX_OUT.above),
            below: Queue::with_capacity(MAX_OUT.below),
            events: Vec::new(),
            read: Read::Nothing,
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
                    Down::Send(_) | Down::Finish => {}
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
