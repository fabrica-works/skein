//! Forge connection (forge-connection.md, sections 3 and 8).
use alloc::boxed::Box;
use skein_forge::forgejo::plans::{Body, Failure, Fields, Heard, plan::Range};
use skein_forge::forgejo::{decode, header};

/// A forge connection's state; the component arms its deadline.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Phase {
    /// Nothing held.
    Closed,
    /// io's connect, until the call's deadline (or a start read's).
    Connecting,
    /// TLS's handshake, until the same deadline.
    Handshaking,
    /// The request going down; `written` once its first byte went.
    Sending { written: bool },
    /// The head, then the body as the decoder demands.
    Reading,
    /// Kept for its forge until `idle`.
    Idle,
}

use crate::{
    grow::{Grow, Tail},
    limits::{self, Limits, Shared},
    settings::{Forge, Transport},
};
use skein_forge::forgejo::{client as documents, request};
use skein_http::client as http;
use skein_json::tokenizer;
use skein_lib::{
    Env, List, Queue, Time, Token, Writer, bytes,
    stream::{Down, Read, Up},
};
/// One exchange's response body state.
#[derive(Debug)]
enum Receiving {
    Json(documents::Decoder),
    Raw { grow: Grow, max: u32, remaining: Option<u64> },
    Tail { tail: Tail, read: u64, remaining: Option<u64> },
    Empty,
}
/// One connection's machines, socket identity, request and pending output.
pub(crate) struct Connection {
    pub(crate) forge: u16,
    pub(crate) phase: Phase,
    pub(crate) deadline: Option<Time>,
    pub(crate) socket: Option<Token>,
    pub(crate) closing: bool,
    pub(crate) written: bool,
    http: http::Client,
    tls: Option<skein_tls::client::Client>,
    json: tokenizer::Tokenizer,
    shared: Shared,
    http_up: Queue<http::Event>,
    http_down: Queue<Down>,
    tls_up: Queue<skein_tls::client::Event>,
    tls_down: Queue<Down>,
    json_up: Queue<tokenizer::Event>,
    json_down: Queue<Down>,
    json_next: Option<tokenizer::Request>,
    heard: Queue<Heard>,
    request: Option<request::Request>,
    receive: Receiving,
    done: Option<Body>,
    after: u64,
    upload: Option<Box<[u8]>>,
    uploaded: u32,
    raw_range: Range,
    raw_cap: u32,
}
impl Connection {
    pub fn new(forge: &Forge, l: &Limits, shared: Shared, deadline: Time) -> Self {
        let tls = match &forge.transport {
            Transport::Plaintext { .. } => None,
            Transport::Tls { trust, server_name } => {
                Some(skein_tls::client::Client::new(trust, server_name.clone(), &l.tls))
            }
        };
        Self {
            forge: forge.number,
            phase: Phase::Connecting,
            deadline: Some(deadline),
            socket: None,
            closing: false,
            written: false,
            http: http::Client::new(&limits::http(l, tls.is_some(), &shared.io)),
            tls,
            json: tokenizer::Tokenizer::new(&limits::documents(l).json),
            shared,
            http_up: Queue::with_capacity(8),
            http_down: Queue::with_capacity(8),
            tls_up: Queue::with_capacity(8),
            tls_down: Queue::with_capacity(8),
            json_up: Queue::with_capacity(8),
            json_down: Queue::with_capacity(8),
            json_next: None,
            heard: Queue::with_capacity(16),
            request: None,
            receive: Receiving::Empty,
            done: None,
            after: 0,
            upload: None,
            uploaded: 0,
            raw_range: Range::Head(0),
            raw_cap: shared.answer_bytes,
        }
    }
    pub fn start(
        &mut self,
        env: &Env<Limits>,
        forge: &Forge,
        user_agent: &[u8],
        request: request::Request,
        range: Option<Range>,
    ) -> Result<(), documents::Refusal> {
        let encoded = documents::encode(&request, &limits::documents(&env.limits))?;
        let mut headers = List::with_capacity(5);
        headers
            .push(field(b"Host", &crate::settings::host(forge).expect("checked forge host")))
            .expect("five header fields");
        let auth =
            skein_forge::forgejo::credential::authorization(&forge.access.as_ref().expect("held access").credential)
                .expect("checked credential length");
        headers.push(field(b"Authorization", auth.expose())).expect("five header fields");
        headers.push(field(b"User-Agent", user_agent)).expect("five header fields");
        let body = match &encoded.body {
            Some(body) => {
                headers.push(field(b"Content-Type", b"application/json")).expect("five header fields");
                http::Body::Length(u64::try_from(body.len()).expect("bounded body"))
            }
            None => http::Body::None,
        };
        if let Some(range) = range {
            let (prefix, max) = match range {
                Range::Head(n) => (b"bytes=0-".as_slice(), n),
                Range::Tail(n) => (b"bytes=-".as_slice(), n),
            };
            let decimal = skein_lib::Decimal::of(u64::from(max));
            let mut value = Writer::new(decimal.as_bytes().len().saturating_add(prefix.len()));
            value.put(prefix).expect("range prefix measured");
            value.put(decimal.as_bytes()).expect("range measured");
            headers
                .push(skein_http::Header { name: Box::from(&b"Range"[..]), value: value.finish() })
                .expect("five header fields");
        }
        self.request = Some(request);
        self.upload = encoded.body;
        self.uploaded = 0;
        self.raw_range = range.unwrap_or(Range::Head(self.raw_cap));
        self.done = None;
        self.written = false;
        self.after = 0;
        self.phase = Phase::Sending { written: false };
        let method = match encoded.method {
            request::Method::Get => skein_http::Method::Get,
            request::Method::Post => skein_http::Method::Post,
            request::Method::Put => skein_http::Method::Put,
            request::Method::Patch => skein_http::Method::Patch,
            request::Method::Delete => skein_http::Method::Delete,
        };
        self.http_down(
            env,
            http::Request::Call(http::Call {
                method,
                target: encoded.target,
                headers: headers.into_boxed(),
                body,
                close: false,
            }),
        );
        if self.upload.is_some() {
            self.http_down(
                env,
                http::Request::Upload(Down::Demand { read: Read::Nothing, room: self.http_env(env).limits.send }),
            );
        }
        Ok(())
    }
    pub fn connect(&mut self, env: &Env<Limits>) {
        match &mut self.tls {
            Some(tls) => {
                self.phase = Phase::Handshaking;
                let tls_env = Env { now: env.now, wall: env.wall, limits: env.limits.tls };
                skein_tls::client::down(
                    tls,
                    &tls_env,
                    skein_tls::client::Request::Handshake,
                    &mut self.tls_up,
                    &mut self.tls_down,
                );
            }
            None => {
                self.phase = Phase::Idle;
            }
        }
    }
    pub fn stream(&mut self, env: &Env<Limits>, event: Up) {
        if self.closing || self.phase == Phase::Closed {
            return;
        }
        match &mut self.tls {
            Some(tls) => {
                let tls_env = Env { now: env.now, wall: env.wall, limits: env.limits.tls };
                skein_tls::client::up(tls, &tls_env, event, &mut self.tls_up, &mut self.tls_down);
            }
            None => {
                let h = self.http_env(env);
                http::up(&mut self.http, &h, event, &mut self.http_up, &mut self.http_down);
            }
        }
    }
    fn http_env(&self, env: &Env<Limits>) -> Env<http::Limits> {
        Env { now: env.now, wall: env.wall, limits: limits::http(&env.limits, self.tls.is_some(), &self.shared.io) }
    }
    fn http_down(&mut self, env: &Env<Limits>, request: http::Request) {
        let h = self.http_env(env);
        http::down(&mut self.http, &h, request, &mut self.http_up, &mut self.http_down);
    }
    fn json_down(&mut self, env: &Env<Limits>, request: tokenizer::Request) {
        let j = Env { now: env.now, wall: env.wall, limits: limits::documents(&env.limits).json };
        tokenizer::down(&mut self.json, &j, request, &mut self.json_up, &mut self.json_down);
    }
    /// Routes at most a fixed number of child-machine movements. Remaining work stays owned.
    pub fn pump(&mut self, env: &Env<Limits>, io: &mut Queue<skein_io::Request>, budget: u32) {
        let mut emitted = 0_u32;
        for _ in 0_u32..64 {
            if emitted >= budget || !self.heard.is_empty() || self.closing || self.phase == Phase::Closed {
                break;
            }
            if let Some(down) = self.tls_down.pop() {
                self.send(down, io);
                emitted = emitted.saturating_add(1);
                continue;
            }
            if let Some(event) = self.tls_up.pop() {
                self.tls_event(env, event);
                continue;
            }
            if let Some(down) = self.http_down.pop() {
                match &mut self.tls {
                    Some(tls) => {
                        let e = Env { now: env.now, wall: env.wall, limits: env.limits.tls };
                        skein_tls::client::down(
                            tls,
                            &e,
                            skein_tls::client::Request::Stream(down),
                            &mut self.tls_up,
                            &mut self.tls_down,
                        );
                    }
                    None => {
                        self.send(down, io);
                        emitted = emitted.saturating_add(1);
                    }
                }
                continue;
            }
            if let Some(down) = self.json_down.pop() {
                self.http_down(env, http::Request::Body(down));
                continue;
            }
            if let Some(request) = self.json_next.take() {
                self.json_down(env, request);
                continue;
            }
            if let Some(event) = self.json_up.pop() {
                self.json_event(env, event);
                continue;
            }
            if let Some(event) = self.http_up.pop() {
                self.http_event(env, event);
                continue;
            }
            break;
        }
    }
    fn send(&mut self, down: Down, io: &mut Queue<skein_io::Request>) {
        if sending(&down) && self.request.is_some() {
            self.written = true;
            self.phase = if self.upload.is_some() { Phase::Sending { written: true } } else { Phase::Reading };
        }
        if let Some(socket) = self.socket {
            io.push(skein_io::Request::Stream { stream: socket, down });
        }
    }
    fn tls_event(&mut self, env: &Env<Limits>, event: skein_tls::client::Event) {
        match event {
            skein_tls::client::Event::Ready(_) => {
                self.phase = Phase::Idle;
            }
            skein_tls::client::Event::Stream(event) => {
                let h = self.http_env(env);
                http::up(&mut self.http, &h, event, &mut self.http_up, &mut self.http_down);
            }
            skein_tls::client::Event::Failed(_) => {
                self.failed(if self.written { Failure::Seen } else { Failure::Unseen });
            }
            skein_tls::client::Event::Closed => {}
        }
    }
    fn http_event(&mut self, env: &Env<Limits>, event: http::Event) {
        match event {
            http::Event::Response(response) => self.response(env, response),
            http::Event::Body(event) => match &mut self.receive {
                Receiving::Json(_) => {
                    let j = Env { now: env.now, wall: env.wall, limits: limits::documents(&env.limits).json };
                    tokenizer::up(&mut self.json, &j, event, &mut self.json_up, &mut self.json_down);
                }
                Receiving::Empty => {
                    drop(event);
                }
                Receiving::Tail { tail, read, remaining } => match event {
                    Up::Bytes(piece) => {
                        let length = u64::try_from(piece.len()).expect("Bounded piece");
                        *read = read.saturating_add(length);
                        if let Some(left) = remaining {
                            *left = left.saturating_sub(length);
                        }
                        if *read > u64::from(env.limits.body_bytes_in) {
                            self.receive = Receiving::Empty;
                            self.heard.push(Heard::Done(Body::Malformed(decode::Malformed::Bound { field: b"log" })));
                            self.phase = Phase::Closed;
                            self.deadline = None;
                        } else {
                            tail.write(&piece);
                            self.raw_demand(env);
                        }
                    }
                    Up::End => {}
                    Up::Failed(_) => self.failed(Failure::Seen),
                    Up::Room => unreachable!("Tail bodies only read"),
                },
                Receiving::Raw { grow, max, remaining } => match event {
                    Up::Bytes(piece) => {
                        if let Some(left) = remaining {
                            *left = left.saturating_sub(u64::try_from(piece.len()).expect("bounded piece"));
                        }
                        let more = grow.write(&piece).is_err() || grow.len() > *max;
                        if more {
                            let grow = match core::mem::replace(&mut self.receive, Receiving::Empty) {
                                Receiving::Raw { grow, .. } => grow,
                                Receiving::Json(_) | Receiving::Tail { .. } | Receiving::Empty => {
                                    unreachable!("raw body")
                                }
                            };
                            self.heard.push(Heard::Done(Body::Raw { more: true, bytes: grow.finish() }));
                            self.phase = Phase::Closed;
                            self.deadline = None;
                        } else {
                            self.raw_demand(env);
                        }
                    }
                    Up::End => {}
                    Up::Failed(_) => self.failed(Failure::Seen),
                    Up::Room => unreachable!("raw bodies only read"),
                },
            },
            http::Event::Upload(event) => self.upload(env, event),
            http::Event::Done(reuse) => {
                let done = match core::mem::replace(&mut self.receive, Receiving::Empty) {
                    Receiving::Json(_) => {
                        self.done.take().unwrap_or(Body::Malformed(decode::Malformed::Missing { field: b"" }))
                    }
                    Receiving::Empty => self.done.take().unwrap_or(Body::Empty),
                    Receiving::Raw { grow, max, .. } => Body::Raw { more: grow.len() > max, bytes: grow.finish() },
                    Receiving::Tail { tail, .. } => Body::Raw { more: tail.dropped(), bytes: tail.finish() },
                };
                let close = reuse == http::Reuse::Close
                    || match &done {
                        Body::Malformed(_) | Body::Raw { more: true, .. } => true,
                        Body::Document(_) | Body::Raw { more: false, .. } | Body::Empty => false,
                    };
                self.heard.push(Heard::Done(done));
                self.request = None;
                self.upload = None;
                self.phase = if close { Phase::Closed } else { Phase::Idle };
                self.deadline = if close { None } else { Some(env.now.saturating_add(env.limits.idle)) };
            }
            http::Event::Failed(error) => self.failed(match error {
                http::Error::Closed(_) | http::Error::Refused(_) => Failure::Unseen,
                http::Error::Truncated { .. }
                | http::Error::Stream(_)
                | http::Error::Status
                | http::Error::Version
                | http::Error::Header
                | http::Error::HeadTooLong
                | http::Error::TooManyHeaders
                | http::Error::Framing
                | http::Error::ChunkSize
                | http::Error::Chunk
                | http::Error::Trailer
                | http::Error::Upgrade => Failure::Seen,
            }),
            http::Event::Closed => {}
        }
    }
    fn response(&mut self, env: &Env<Limits>, response: http::Response) {
        let date = match response.header(b"Date") {
            Some(date) => skein_forge::forgejo::time::http_date(date),
            None => None,
        };
        let retry_after = match response.header(b"Retry-After") {
            Some(value) => header::retry_after(value, date.unwrap_or(env.wall)),
            None => None,
        };
        let has_more = match response.header(b"X-HasMore") {
            Some(value) => header::has_more(value),
            None => None,
        };
        let content_range = match response.header(b"Content-Range") {
            Some(value) => header::content_range(value),
            None => None,
        };
        let close = match response.header(b"Connection") {
            Some(value) => value.eq_ignore_ascii_case(b"close"),
            None => false,
        };
        self.heard.push(Heard::Head {
            status: response.status,
            fields: Fields { date, retry_after, has_more, content_range, close },
        });
        let l = limits::documents(&env.limits);
        let request = self.request.as_ref().expect("head belongs to a request");
        if response.status == 401 || response.status == 403 || response.status == 404 {
            self.receive = Receiving::Empty;
            self.http_down(env, http::Request::Discard);
            return;
        }
        let decoder =
            if response.status >= 300 { Some(documents::error_decoder(&l)) } else { documents::decoder(request, &l) };
        match decoder {
            Some(mut decoder) => {
                documents::keep_after(&mut decoder, self.after);
                self.receive = Receiving::Json(decoder);
                self.json = tokenizer::Tokenizer::new(&l.json);
                self.json_down(env, tokenizer::Request::Next);
            }
            None => {
                if raw_request(request) {
                    let remaining = match response.framing {
                        http::Framing::Length(length) => Some(length),
                        http::Framing::Empty => Some(0),
                        http::Framing::Chunked | http::Framing::UntilEnd => None,
                    };
                    self.receive = match self.raw_range {
                        Range::Head(max) => Receiving::Raw {
                            grow: Grow::new(env.limits.chunk, max.saturating_add(1).min(self.raw_cap)),
                            max,
                            remaining,
                        },
                        Range::Tail(max) => Receiving::Tail {
                            tail: Tail::new(env.limits.chunk, max.min(self.raw_cap)),
                            read: 0,
                            remaining,
                        },
                    };
                    self.raw_demand(env);
                } else {
                    self.receive = Receiving::Empty;
                    self.http_down(env, http::Request::Discard);
                }
            }
        }
    }
    fn raw_demand(&mut self, env: &Env<Limits>) {
        let count = match &self.receive {
            Receiving::Raw { grow, remaining, .. } => match remaining {
                Some(left) => grow.room().min(env.limits.chunk).min(u32::try_from(*left).unwrap_or(u32::MAX)).max(1),
                None => 1,
            },
            Receiving::Tail { remaining, .. } => match remaining {
                Some(left) => env.limits.chunk.min(u32::try_from(*left).unwrap_or(u32::MAX)).max(1),
                None => 1,
            },
            Receiving::Json(_) | Receiving::Empty => return,
        };
        self.http_down(env, http::Request::Body(Down::Demand { read: Read::Fill(count), room: 0 }));
    }
    fn upload(&mut self, env: &Env<Limits>, event: Up) {
        match event {
            Up::Room => {
                let Some(body) = &self.upload else {
                    return;
                };
                let start = usize::try_from(self.uploaded).expect("bounded position");
                let end = body
                    .len()
                    .min(start.saturating_add(usize::try_from(self.http_env(env).limits.send).expect("send fits")));
                let piece = bytes::copy_of(body.get(start..end).expect("bounded position"));
                self.uploaded = u32::try_from(end).expect("bounded body");
                let finished = end == body.len();
                self.http_down(env, http::Request::Upload(Down::Send(piece)));
                if finished {
                    self.upload = None;
                    self.phase = Phase::Reading;
                    self.http_down(env, http::Request::Upload(Down::Finish));
                } else {
                    self.http_down(
                        env,
                        http::Request::Upload(Down::Demand {
                            read: Read::Nothing,
                            room: self.http_env(env).limits.send,
                        }),
                    );
                }
            }
            Up::Failed(_) | Up::End => {
                self.upload = None;
            }
            Up::Bytes(_) => unreachable!("uploads only write"),
        }
    }
    fn json_event(&mut self, env: &Env<Limits>, event: tokenizer::Event) {
        let Some(decoder) = self.decoder() else {
            return;
        };
        let step = documents::step(decoder, &limits::documents(&env.limits), &event);
        match step {
            decode::Step::Demand(request) => self.json_down(env, request),
            decode::Step::Row(row, request) => {
                self.heard.push(Heard::Row(row));
                self.json_next = Some(request);
            }
            decode::Step::Done(document) => {
                self.done = Some(Body::Document(document));
            }
            decode::Step::Malformed(decode::Malformed::Json(tokenizer::Error::Stream(_))) => self.failed(Failure::Seen),
            decode::Step::Malformed(error) => {
                self.receive = Receiving::Empty;
                self.phase = Phase::Closed;
                self.deadline = None;
                self.heard.push(Heard::Done(Body::Malformed(error)));
            }
        }
    }
    pub fn failed(&mut self, failure: Failure) {
        if self.phase == Phase::Closed {
            return;
        }
        self.phase = Phase::Closed;
        self.deadline = None;
        self.heard.push(Heard::Failed(failure));
    }
    pub fn timeout(&mut self) {
        self.failed(if self.written { Failure::Seen } else { Failure::Unseen });
    }
    pub fn in_flight(&self) -> bool {
        self.request.is_some()
            || match self.phase {
                Phase::Connecting | Phase::Handshaking => true,
                Phase::Closed | Phase::Sending { .. } | Phase::Reading | Phase::Idle => false,
            }
    }
    pub fn close(&mut self, abort: bool, io: &mut Queue<skein_io::Request>) {
        if self.closing {
            return;
        }
        self.phase = Phase::Closed;
        self.deadline = None;
        if let Some(socket) = self.socket {
            self.closing = true;
            io.push(if abort {
                skein_io::Request::Abort { entity: socket }
            } else {
                skein_io::Request::Close { entity: socket }
            });
        }
    }
    pub fn heard(&mut self) -> Option<Heard> {
        self.heard.pop()
    }
    pub fn ready(&self) -> bool {
        if self.phase == Phase::Closed {
            return !self.heard.is_empty();
        }
        !self.http_up.is_empty()
            || !self.http_down.is_empty()
            || !self.tls_up.is_empty()
            || !self.tls_down.is_empty()
            || !self.json_up.is_empty()
            || !self.json_down.is_empty()
            || !self.heard.is_empty()
            || self.json_next.is_some()
    }
    pub fn keep_after(&mut self, after: u64) {
        self.after = after;
        if let Some(decoder) = self.decoder() {
            documents::keep_after(decoder, after);
        }
    }
    /// The answer's remaining payload room, set by the holding plan before its raw read.
    pub fn raw_room(&mut self, room: u32) {
        self.raw_cap = room;
    }
    pub fn cut(&mut self, _env: &Env<Limits>) {
        if let Some(decoder) = self.decoder() {
            let request = documents::cut(decoder);
            self.json_next = Some(request);
        }
    }
}

fn field(name: &[u8], value: &[u8]) -> skein_http::Header {
    skein_http::Header { name: bytes::copy_of(name), value: bytes::copy_of(value) }
}

impl core::fmt::Debug for Connection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Connection")
            .field("forge", &self.forge)
            .field("phase", &self.phase)
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}
/// The fixed routing queues, allocated once per connection.
pub(crate) fn routing_bytes() -> Option<u64> {
    Queue::<http::Event>::worst_case(8)?
        .checked_add(Queue::<Down>::worst_case(8)?.checked_mul(3)?)?
        .checked_add(Queue::<skein_tls::client::Event>::worst_case(8)?)?
        .checked_add(Queue::<tokenizer::Event>::worst_case(8)?)?
        .checked_add(Queue::<Heard>::worst_case(16)?)
}
impl Connection {
    fn decoder(&mut self) -> Option<&mut documents::Decoder> {
        match &mut self.receive {
            Receiving::Json(decoder) => Some(decoder),
            Receiving::Raw { .. } | Receiving::Tail { .. } | Receiving::Empty => None,
        }
    }
}
fn sending(down: &Down) -> bool {
    match down {
        Down::Send(_) => true,
        Down::Demand { .. } | Down::Finish => false,
    }
}
fn raw_request(request: &request::Request) -> bool {
    match request {
        request::Request::Raw { .. } | request::Request::JobLog { .. } => true,
        request::Request::User
        | request::Request::ApiSettings
        | request::Request::RepositoryById { .. }
        | request::Request::Repository { .. }
        // user/repos is used by the setup host, not the connection.
        | request::Request::UserRepos { .. }
        | request::Request::UserSearch { .. }
        | request::Request::Issues { .. }
        | request::Request::Issue { .. }
        | request::Request::Comments { .. }
        | request::Request::Pull { .. }
        | request::Request::PullFor { .. }
        | request::Request::CombinedStatus { .. }
        | request::Request::Reviews { .. }
        | request::Request::ReviewComments { .. }
        | request::Request::Branch { .. }
        | request::Request::Branches { .. }
        | request::Request::Protection { .. }
        | request::Request::Protections { .. }
        | request::Request::Collaborators { .. }
        | request::Request::Permission { .. }
        | request::Request::Pulls { .. }
        | request::Request::PullFiles { .. }
        | request::Request::Compare { .. }
        | request::Request::Runs { .. }
        | request::Request::Run { .. }
        | request::Request::Job { .. }
        | request::Request::Jobs { .. }
        | request::Request::CreateIssue { .. }
        | request::Request::OpenPull { .. }
        | request::Request::PostComment { .. }
        | request::Request::PostReview { .. }
        | request::Request::EditIssue { .. }
        | request::Request::RequestReviewers { .. }
        | request::Request::RemoveReviewers { .. }
        | request::Request::Merge { .. }
        | request::Request::Update { .. }
        | request::Request::PostStatus { .. }
        | request::Request::CreateBranch { .. }
        | request::Request::Tokens { .. }
        | request::Request::CreateToken { .. }
        | request::Request::DeleteToken { .. }
        | request::Request::UserOf { .. }
        | request::Request::Org { .. }
        | request::Request::CreateOrg { .. }
        | request::Request::Teams { .. }
        | request::Request::CreateTeam { .. }
        | request::Request::EditTeam { .. }
        | request::Request::TeamMembers { .. }
        | request::Request::OrgHooks { .. }
        | request::Request::CreateOrgHook { .. }
        | request::Request::EditOrgHook { .. }
        | request::Request::DeleteOrgHook { .. }
        | request::Request::AddMember { .. }
        // setup requests are the setup host's, not the connection's.
        | request::Request::AddCollaborator { .. }
        | request::Request::RepoHooks { .. }
        | request::Request::CreateRepoHook { .. }
        | request::Request::EditRepoHook { .. }
        | request::Request::DeleteRepoHook { .. }
        | request::Request::CreateUser { .. }
        | request::Request::DeleteBranch { .. } => false,
    }
}
