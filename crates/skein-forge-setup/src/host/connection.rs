//! One request per connection, with its answer decoded as bytes arrive (6.3).
use super::{
    Limits, limits,
    settings::{self, Forge},
};
use crate::reconcile::Reply;
use alloc::boxed::Box;
use skein_forge::{
    credential::Credential,
    forgejo::{
        client as documents,
        decode::Step,
        document::{Document, Row},
        request,
    },
};
use skein_http::client as http;
use skein_json::tokenizer;
use skein_lib::{
    Env, List, Queue, Secret, Token, bytes,
    stream::{Down, Read, Up},
};

pub(super) struct Connection {
    pub(super) run: Token,
    pub(super) socket: Option<Token>,
    pub(super) closed: bool,
    pub(super) closing: Option<bool>,
    close_sent: bool,
    pub(super) result: Option<Reply>,
    answered: bool,
    http: http::Client,
    http_up: Queue<http::Event>,
    http_down: Queue<Down>,
    json: tokenizer::Tokenizer,
    json_up: Queue<tokenizer::Event>,
    json_down: Queue<Down>,
    json_next: Option<tokenizer::Request>,
    decoder: Option<documents::Decoder>,
    document: Option<Document>,
    rows: List<Row>,
    request: request::Request,
    call: Option<http::Call>,
    upload: Option<Box<[u8]>>,
    uploaded: usize,
}
impl Connection {
    pub(super) fn new(
        run: Token,
        forge: &Forge,
        agent: &[u8],
        request: &request::Request,
        token: &Secret,
        l: &Limits,
    ) -> Option<Self> {
        let encoded = documents::encode(request, &limits::documents(l)).ok()?;
        let mut headers = List::with_capacity(4);
        headers.push(field(b"Host", &settings::host(forge)?)).expect("four headers");
        let auth = skein_forge::forgejo::credential::authorization(&Credential::Token(token.clone()))?;
        headers.push(field(b"Authorization", auth.expose())).expect("four headers");
        headers.push(field(b"User-Agent", agent)).expect("four headers");
        let body = match &encoded.body {
            Some(body) => {
                headers.push(field(b"Content-Type", b"application/json")).expect("four headers");
                http::Body::Length(u64::try_from(body.len()).expect("bounded body"))
            }
            None => http::Body::None,
        };
        let method = match encoded.method {
            request::Method::Get => skein_http::Method::Get,
            request::Method::Post => skein_http::Method::Post,
            request::Method::Put => skein_http::Method::Put,
            request::Method::Patch => skein_http::Method::Patch,
            request::Method::Delete => skein_http::Method::Delete,
        };
        Some(Self {
            run,
            socket: None,
            closed: false,
            closing: None,
            close_sent: false,
            result: None,
            answered: false,
            http: http::Client::new(&limits::http(l)),
            http_up: Queue::with_capacity(8),
            http_down: Queue::with_capacity(8),
            json: tokenizer::Tokenizer::new(&limits::documents(l).json),
            json_up: Queue::with_capacity(8),
            json_down: Queue::with_capacity(8),
            json_next: None,
            decoder: None,
            document: None,
            // Each row consumes at least one input byte; the tokenizer bounds the page.
            rows: List::with_capacity(l.body_bytes_in),
            request: request.clone(),
            call: Some(http::Call { method, target: encoded.target, headers: headers.into_boxed(), body, close: true }),
            upload: encoded.body,
            uploaded: 0,
        })
    }
    pub(super) fn connect(&mut self, env: &Env<Limits>) {
        if self.closing.is_some() {
            return;
        }
        if let Some(call) = self.call.take() {
            self.http_down(env, http::Request::Call(call));
            if self.upload.is_some() {
                self.http_down(
                    env,
                    http::Request::Upload(Down::Demand { read: Read::Nothing, room: limits::http(&env.limits).send }),
                );
            }
        }
    }
    pub(super) fn stream(&mut self, env: &Env<Limits>, event: Up) {
        if self.closing.is_some() {
            return;
        }
        let h = Env { now: env.now, wall: env.wall, limits: limits::http(&env.limits) };
        http::up(&mut self.http, &h, event, &mut self.http_up, &mut self.http_down);
    }
    fn http_down(&mut self, env: &Env<Limits>, request: http::Request) {
        let h = Env { now: env.now, wall: env.wall, limits: limits::http(&env.limits) };
        http::down(&mut self.http, &h, request, &mut self.http_up, &mut self.http_down);
    }
    fn json_down(&mut self, env: &Env<Limits>, request: tokenizer::Request) {
        let j = Env { now: env.now, wall: env.wall, limits: limits::documents(&env.limits).json };
        tokenizer::down(&mut self.json, &j, request, &mut self.json_up, &mut self.json_down);
    }
    pub(super) fn fail(&mut self) {
        if !self.answered {
            self.finish(Reply::Down);
        }
    }
    fn finish(&mut self, reply: Reply) {
        self.result = Some(reply);
        self.answered = true;
        self.closing = Some(false);
        self.upload = None;
        self.call = None;
    }
    pub(super) fn abort(&mut self) {
        self.answered = true;
        self.closing = Some(true);
        self.result = None;
        self.upload = None;
        self.call = None;
    }
    /// Routes one owned movement, with room reserved for one io request.
    pub(super) fn pump(&mut self, env: &Env<Limits>, io: &mut Queue<skein_io::Request>) -> bool {
        if let Some(abort) = self.closing {
            if !self.closed
                && !self.close_sent
                && let Some(socket) = self.socket
            {
                self.close_sent = true;
                io.push(if abort {
                    skein_io::Request::Abort { entity: socket }
                } else {
                    skein_io::Request::Close { entity: socket }
                });
                return true;
            }
            return false;
        }
        if let Some(down) = self.http_down.pop() {
            if !self.closed
                && let Some(socket) = self.socket
            {
                io.push(skein_io::Request::Stream { stream: socket, down });
            }
            return true;
        }
        if let Some(down) = self.json_down.pop() {
            self.http_down(env, http::Request::Body(down));
            return true;
        }
        if let Some(next) = self.json_next.take() {
            self.json_down(env, next);
            return true;
        }
        if let Some(event) = self.json_up.pop() {
            self.json_event(env, &event);
            return true;
        }
        if let Some(event) = self.http_up.pop() {
            self.http_event(env, event);
            return true;
        }
        if self.closed && !self.answered {
            self.fail();
            return true;
        }
        false
    }
    pub(super) fn ready(&self) -> bool {
        if self.closing.is_some() {
            return !self.closed && !self.close_sent && self.socket.is_some();
        }
        !self.http_down.is_empty()
            || !self.http_up.is_empty()
            || !self.json_down.is_empty()
            || !self.json_up.is_empty()
            || self.json_next.is_some()
            || self.closed
    }
    fn http_event(&mut self, env: &Env<Limits>, event: http::Event) {
        match event {
            http::Event::Response(response) => {
                if (200..300).contains(&response.status) {
                    self.decoder = documents::decoder(&self.request, &limits::documents(&env.limits));
                    match &self.decoder {
                        Some(decoder) => self.json_next = Some(documents::first(decoder)),
                        None => self.finish(Reply::Empty),
                    }
                } else {
                    self.finish(Reply::Status(response.status));
                }
            }
            http::Event::Body(event) => {
                let j = Env { now: env.now, wall: env.wall, limits: limits::documents(&env.limits).json };
                tokenizer::up(&mut self.json, &j, event, &mut self.json_up, &mut self.json_down);
            }
            http::Event::Upload(event) => match event {
                Up::Room => self.upload(env),
                Up::Failed(_) | Up::Bytes(_) | Up::End => self.fail(),
            },
            http::Event::Done(_) => {
                let reply = match self.document.take() {
                    Some(Document::Listed { skipped: 0 } | Document::RepoSearch { ok: true }) => {
                        let rows = core::mem::replace(&mut self.rows, List::with_capacity(0));
                        Reply::Rows(rows.into_boxed())
                    }
                    Some(document) => Reply::Document(document),
                    None => Reply::Down,
                };
                self.finish(reply);
            }
            http::Event::Failed(_) => self.fail(),
            http::Event::Closed => (),
        }
    }
    fn json_event(&mut self, env: &Env<Limits>, event: &tokenizer::Event) {
        let Some(decoder) = &mut self.decoder else {
            return;
        };
        match documents::step(decoder, &limits::documents(&env.limits), event) {
            Step::Demand(next) => self.json_next = Some(next),
            Step::Row(row, next) => {
                if self.rows.push(row).is_err() {
                    self.fail();
                } else {
                    self.json_next = Some(next);
                }
            }
            Step::Done(document) => self.document = Some(document),
            Step::Malformed(_) => self.fail(),
        }
    }
    fn upload(&mut self, env: &Env<Limits>) {
        let Some(body) = &self.upload else {
            return;
        };
        let end = self
            .uploaded
            .saturating_add(usize::try_from(limits::http(&env.limits).send).expect("u32 fits"))
            .min(body.len());
        let piece = bytes::copy_of(body.get(self.uploaded..end).expect("bounded upload"));
        self.uploaded = end;
        let done = end == body.len();
        self.http_down(env, http::Request::Upload(Down::Send(piece)));
        if done {
            self.upload = None;
            self.http_down(env, http::Request::Upload(Down::Finish));
        } else {
            self.http_down(
                env,
                http::Request::Upload(Down::Demand { read: Read::Nothing, room: limits::http(&env.limits).send }),
            );
        }
    }
}
fn field(name: &[u8], value: &[u8]) -> skein_http::Header {
    skein_http::Header { name: bytes::copy_of(name), value: bytes::copy_of(value) }
}
impl core::fmt::Debug for Connection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Connection")
            .field("run", &self.run)
            .field("closing", &self.closing)
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}
