//! Incremental text writes, queued behind the current SSE event.
//! Contract: docs/design/fake-llm.md, sections 2.2 and 5.

use super::{Call, Event, Limits, Server, Service, State, documents, next_event};
use alloc::boxed::Box;
use skein_fake_llm_domain::api;
use skein_http::{Header, server as http, sse::writer::Outgoing};
use skein_lib::stream::Down;
use skein_lib::{Env, Id, Queue, Token, bytes};
use skein_llm::{anthropic, openai};

pub(super) struct Stream {
    pending: Queue<Option<Box<[u8]>>>,
    bytes: usize,
    usage: api::Usage,
    opening: u32,
    busy: bool,
    error: Option<api::Error>,
}

impl Stream {
    pub(super) fn new(limits: &documents::Limits) -> Self {
        Self {
            pending: Queue::with_capacity(cap(limits).checked_mul(2).expect("checked pieces bound")),
            bytes: 0,
            usage: api::Usage::NONE,
            opening: 0,
            busy: false,
            error: None,
        }
    }

    pub(super) fn sent(&mut self) {
        self.busy = false;
    }

    pub(super) fn fail(&mut self, error: api::Error) {
        self.error = Some(error);
    }

    pub(super) fn clear(&mut self) {
        while self.pending.pop().is_some() {}
        self.bytes = 0;
        self.usage = api::Usage::NONE;
        self.opening = 0;
        self.busy = false;
        self.error = None;
    }
}

pub(super) fn ready(server: &Server) -> bool {
    if let State::Pieces { .. } = server.state {
        !server.pieces.busy
            && (server.pieces.opening < 2
                || !server.pieces.pending.is_empty()
                || server.answer.is_some()
                || server.pieces.error.is_some())
    } else {
        false
    }
}

fn cap(limits: &documents::Limits) -> u32 {
    limits.anthropic.answer.max(limits.openai.answer)
}

pub(super) fn worst_case(limits: &documents::Limits) -> Option<u64> {
    // Every nonempty piece may have one preceding hold. Text payloads together
    // fit the answer cap; the queue separately prices their wrappers and holds.
    Queue::<Option<Box<[u8]>>>::worst_case(cap(limits).checked_mul(2)?)?.checked_add(u64::from(cap(limits)))
}

pub(super) fn head(server: &mut Server) {
    server.requests.push(http::Request::Respond(http::Response {
        status: 200,
        headers: Box::new([Header {
            name: bytes::copy_of(b"content-type"),
            value: bytes::copy_of(b"text/event-stream"),
        }]),
        body: http::Body::Chunked,
        close: false,
    }));
}

pub(super) fn receive(
    server: &mut Server,
    service: &mut Service,
    env: &Env<Limits>,
    piece: (Token, api::Usage, Option<Box<[u8]>>),
    above: &mut Queue<Event>,
    below: &mut Queue<Down>,
) {
    let (call, usage, text) = piece;
    let id = Id::<Call>::from_token(call);
    if server.call != Some(id) || !super::matches_waiting(server.state) {
        return;
    }
    let Some(call) = service.calls.get(id) else { return };
    if !call.active || call.server != server.owner {
        return;
    }
    let length = match &text {
        Some(text) => text.len(),
        None => 0,
    };
    let bound = usize::try_from(cap(&env.limits.documents)).expect("u32 fits usize");
    let written = match server.state {
        State::Pieces { written } => written,
        State::New
        | State::Reading
        | State::Body
        | State::Waiting
        | State::Streaming
        | State::Erring
        | State::Closing
        | State::Closed => 0,
    };
    if length > bound.saturating_sub(written).saturating_sub(server.pieces.bytes) || server.pieces.pending.room() == 0 {
        super::close(server, service, env, above, below);
        return;
    }
    if server.state == State::Waiting {
        server.state = State::Pieces { written: 0 };
        server.pieces.usage = usage;
        head(server);
    }
    server.pieces.bytes = server.pieces.bytes.checked_add(length).expect("checked answer cap");
    server.pieces.pending.push(text);
    next_event(server, service, env, above, below);
}

pub(super) fn next(
    server: &mut Server,
    service: &Service,
    env: &Env<Limits>,
) -> Result<Option<Outgoing>, documents::Error> {
    if server.pieces.busy {
        return Ok(None);
    }
    let State::Pieces { mut written } = server.state else { return Ok(None) };
    let provider = service.config.provider;
    let options = documents::Options { echo: service.config.echo, usage_fields: service.config.usage_fields };
    let event = if server.pieces.opening < 2 {
        let answer = api::Answer {
            parts: Box::new([api::Part::Text { text: Box::new([]) }]),
            finish: api::Finish::Stop,
            usage: server.pieces.usage,
        };
        let event = documents::event_with_options(
            provider,
            &answer,
            server.pieces.opening,
            server.response_id,
            server.body.as_slice(),
            options,
            &env.limits.documents,
        )?;
        server.pieces.opening = server.pieces.opening.checked_add(1).expect("two opening events");
        event
    } else if let Some(piece) = server.pieces.pending.pop() {
        match piece {
            Some(text) => {
                server.pieces.bytes = server.pieces.bytes.checked_sub(text.len()).expect("queued payload");
                written = written.checked_add(text.len()).ok_or(documents::Error::TooLarge)?;
                Some(delta(provider, text, &env.limits.documents)?)
            }
            None => hold(provider),
        }
    } else if let Some(error) = server.pieces.error.take() {
        server.state = State::Streaming;
        Some(failed(provider, error, &env.limits.documents)?)
    } else if let Some(answer) = &server.answer {
        let Some(api::Part::Text { text }) = answer.parts.first() else {
            return Err(documents::Error::Malformed);
        };
        let suffix = text.get(written..).ok_or(documents::Error::Malformed)?;
        if suffix.is_empty() {
            server.sequence = match provider {
                documents::Provider::Anthropic => 3,
                documents::Provider::OpenAi => 2,
            };
            server.state = State::Streaming;
            let event = documents::event_with_options(
                provider,
                answer,
                server.sequence,
                server.response_id,
                server.body.as_slice(),
                options,
                &env.limits.documents,
            )?;
            server.sequence = server.sequence.checked_add(1).ok_or(documents::Error::TooLarge)?;
            event
        } else {
            written = text.len();
            Some(delta(provider, bytes::copy_of(suffix), &env.limits.documents)?)
        }
    } else {
        None
    };
    if let State::Pieces { .. } = server.state {
        server.state = State::Pieces { written };
    }
    server.pieces.busy = event.is_some();
    // OpenAI holds write no event. Continue on the next resume when another
    // queued piece or the terminal is already available.
    Ok(event)
}

fn delta(
    provider: documents::Provider,
    text: Box<[u8]>,
    limits: &documents::Limits,
) -> Result<Outgoing, documents::Error> {
    let (name, data) = match provider {
        documents::Provider::Anthropic => (
            b"content_block_delta".as_slice(),
            anthropic::encode_event(
                &anthropic::Event::Delta { index: 0, delta: anthropic::Delta::Text { text } },
                &limits.anthropic,
            ),
        ),
        documents::Provider::OpenAi => (
            b"response.output_text.delta".as_slice(),
            openai::encode_event(&openai::Event::TextDelta { index: 0, content_index: 0, text }, &limits.openai),
        ),
    };
    Ok(Outgoing { name: bytes::copy_of(name), data: data.or(Err(documents::Error::TooLarge))?, id: None, retry: None })
}

fn hold(provider: documents::Provider) -> Option<Outgoing> {
    match provider {
        documents::Provider::Anthropic => Some(Outgoing {
            name: bytes::copy_of(b"ping"),
            data: bytes::copy_of(br#"{"type":"ping"}"#),
            id: None,
            retry: None,
        }),
        documents::Provider::OpenAi => None,
    }
}

fn failed(
    provider: documents::Provider,
    error: api::Error,
    limits: &documents::Limits,
) -> Result<Outgoing, documents::Error> {
    let kind = match error {
        api::Error::Overloaded => b"overloaded_error".as_slice(),
        api::Error::RateLimited { .. } | api::Error::Exhausted { .. } => b"rate_limit_error".as_slice(),
        api::Error::Unavailable => b"server_error".as_slice(),
        api::Error::ContextTooLong => b"context_length_exceeded".as_slice(),
        api::Error::Unauthorized => b"authentication_error".as_slice(),
        api::Error::InvalidRequest => b"invalid_request_error".as_slice(),
    };
    let error = openai::ProviderError {
        kind: bytes::copy_of(kind),
        message: bytes::copy_of(kind),
        resets_in_seconds: None,
        resets_at: None,
    };
    let (name, data) = match provider {
        documents::Provider::Anthropic => {
            (b"error".as_slice(), anthropic::encode_event(&anthropic::Event::Failed { error }, &limits.anthropic))
        }
        documents::Provider::OpenAi => {
            (b"response.failed".as_slice(), openai::encode_event(&openai::Event::Failed { error }, &limits.openai))
        }
    };
    Ok(Outgoing { name: bytes::copy_of(name), data: data.or(Err(documents::Error::TooLarge))?, id: None, retry: None })
}

#[cfg(test)]
mod tests {
    use super::{documents::Provider, hold};
    use skein_http::sse::writer::Outgoing;
    use skein_lib::bytes::copy_of;

    #[test]
    fn a_hold_writes_one_ping_in_anthropic_and_nothing_in_openai() {
        for (provider, expected) in [
            (
                Provider::Anthropic,
                Some(Outgoing { name: copy_of(b"ping"), data: copy_of(br#"{"type":"ping"}"#), id: None, retry: None }),
            ),
            (Provider::OpenAi, None),
        ] {
            assert_eq!(hold(provider), expected, "{provider:?}");
        }
    }
}
