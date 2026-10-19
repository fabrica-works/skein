//! The component's small limits and configured forge.
use crate::Limits;
/// Check and construction return the same refusal before any io is begun.
pub fn admission(settings: Settings, limits: &Limits, expected: Result<(), crate::Unusable>, label: &[u8]) {
    let shared = shared();
    assert_eq!(crate::limits::check(limits, &shared, &settings), expected, "{label:?}: check");
    let constructed = match Component::new(settings, limits, &shared) {
        Ok(_) => Ok(()),
        Err(reason) => Err(reason),
    };
    assert_eq!(constructed, expected, "{label:?}: construction");
}

use skein_lib::Duration;
pub const WORD: &[u8] = b"kit";
pub const USER_AGENT: &[u8] = b"kit/0.0.0";
pub const OP_BYTES: u32 = 512;
// Twice the operation bound, the owner's word, and marker framing (forge.md 6).
pub const BODY_ROOM: u32 = 2 * OP_BYTES + 3 + 15;
pub const BODY_ROOM_SHORT: u32 = BODY_ROOM - 1;
pub const MAX_WORD_BODY_ROOM: u32 = 2 * OP_BYTES + 16 + 15;
pub const WRITER: u64 = 2;
pub fn limits() -> Limits {
    Limits {
        forges: 2,
        token_bytes: 64,
        connections: 2,
        head_bytes: 4096,
        headers: 32,
        body_bytes_in: 1_048_576,
        depth: 16,
        chunk: 1024,
        string_bytes: 128,
        body_bytes: BODY_ROOM,
        inner: 8,
        target_bytes: 1024,
        repositories: 2,
        users: 8,
        deadline: Duration::from_secs(10),
        idle: Duration::from_secs(5),
        start_backoff: Duration::from_secs(1),
        start_backoff_max: Duration::from_secs(8),
        hooks: 4,
        hook_head_bytes: 8192,
        hook_body_bytes: 1_048_576,
        hook_deadline: Duration::from_secs(10),
        hook_secret_bytes: 128,
        tls: skein_tls::client::Limits { read: 16384, send: 16384, records: skein_tls::client::MAX_RECORD },
    }
}

use crate::{
    Answered, Component, Event, Settings, Shared,
    settings::{Forge, Kind, Transport},
    start::Start,
};
use alloc::boxed::Box;
use core::net::{Ipv4Addr, SocketAddr};
use skein_forge::api;
use skein_lib::{Env, List, Queue, Secret, Time, Wall};
pub const FORGE: u16 = 1;
pub const REPO: api::Repository = api::Repository { forge: 1, repository: 1 };
pub const TOKEN: &[u8] = b"0123456789abcdef0123456789abcdef01234567";
pub fn shared() -> Shared {
    Shared {
        calls: 2,
        op_bytes: OP_BYTES,
        answer_bytes: 65_536,
        rows: 8,
        lifetime: Duration::from_secs(10),
        name_bytes: 128,
        repositories: 2,
        io: skein_io::Limits {
            sockets: 8,
            names: 0,
            refusals: 2,
            intake: 32_768,
            receive: 4096,
            output: 32_768,
            sends: 4,
            accepts: 1,
            backlog: 1,
            close_timeout: Duration::from_secs(1),
            retry: Duration::from_millis(10),
        },
    }
}
pub fn settings() -> Settings {
    let mut forges = List::with_capacity(2);
    forges
        .push(Forge {
            number: FORGE,
            kind: Kind::Forgejo,
            address: skein_io::Addr::Ip(SocketAddr::from((Ipv4Addr::LOCALHOST, 3000))),
            transport: Transport::Plaintext { name: None },
            credential: skein_forge::credential::Credential::Token(Secret::new(Box::from(TOKEN))),
            user: WRITER,
        })
        .expect("one forge");
    Settings { forges, webhooks: None, word: Box::from(WORD), user_agent: Box::from(USER_AGENT) }
}
#[derive(Debug)]
pub struct Harness {
    pub component: Component,
    pub env: Env<Limits>,
    pub up: Queue<Event>,
    pub io: Queue<skein_io::Request>,
    pub answers: skein_lib::Map<u64, Answered>,
    pub peers: skein_lib::Map<skein_lib::Token, Peer>,
    pub opened: u32,
    pub stopped: Option<(u16, crate::Stopped)>,
    pub closed: bool,
}
#[derive(Debug)]
pub struct Peer {
    forge: u16,
    socket: skein_lib::Token,
    wire: skein_world::stream::Wire,
    connected: bool,
}
impl Harness {
    pub fn new(settings: Settings) -> Self {
        Self::with(limits(), shared(), settings)
    }
    pub fn with(limits: Limits, shared: Shared, settings: Settings) -> Self {
        let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
        let mut h = Self {
            component: Component::new(settings, &env.limits, &shared).expect("fixture limits agree"),
            env,
            up: Queue::with_capacity(32),
            io: Queue::with_capacity(32),
            answers: skein_lib::Map::with_capacity(32),
            peers: skein_lib::Map::with_capacity(32),
            opened: 0,
            stopped: None,
            closed: false,
        };
        h.component.begin(&h.env, &mut h.up, &mut h.io);
        h.drain();
        h
    }
    pub fn call(&mut self, call: u64, repository: api::Repository, op: api::Op) {
        self.send(crate::Request::Call { call: skein_lib::Token::new(call), repository, op });
    }
    pub fn known(&mut self, repository: api::Repository, row: skein_forge::forgejo::plans::tables::Repository) {
        self.component.tables().keep_repository(repository, row);
    }
    pub fn send(&mut self, request: crate::Request) {
        self.component.down(&self.env, request, &mut self.up, &mut self.io);
    }
    pub fn run(&mut self, span: Duration) {
        self.env.now = self.env.now.saturating_add(span);
        self.component.fire(&self.env, &mut self.up, &mut self.io);
        self.drain();
        self.component.reclaim();
        for _ in 0..self.up.len() {
            match self.up.pop().expect("queued event") {
                Event::Answered(answer) => {
                    assert!(self.answered(answer.call.raw()).is_none(), "one terminal per call");
                    drop(self.answers.insert(answer.call.raw(), answer).expect("fixture answers fit"));
                }
                Event::Stopped { forge, why } => {
                    assert!(self.stopped.is_none(), "start stopped once");
                    self.stopped = Some((forge, why));
                }
                Event::Hint(_) | Event::ListenerLost => panic!("webhooks absent"),
                Event::Closed => {
                    assert!(!self.closed, "closed once");
                    self.closed = true;
                }
            }
        }
    }
    pub fn answered(&self, call: u64) -> Option<&Answered> {
        self.answers.get(&call)
    }
    pub fn start(&self, forge: u16) -> Start {
        self.component.start(forge).expect("fixture forge configured")
    }
    fn newest(&self, forge: u16) -> skein_lib::Token {
        let mut found = None;
        for (owner, peer) in &self.peers {
            if peer.forge == forge {
                found = Some(*owner);
            }
        }
        found.expect("forge has a connection")
    }
    fn owners(&self) -> List<skein_lib::Token> {
        let mut owners = List::with_capacity(32);
        for (owner, _) in &self.peers {
            owners.push(*owner).expect("fixture peers fit");
        }
        owners
    }
    fn event(&mut self, event: skein_io::Event) {
        self.component.up(&self.env, event, &mut self.up, &mut self.io);
    }
    pub fn connected(&mut self, forge: u16) {
        let owner = self.newest(forge);
        self.peers.get_mut(&owner).expect("registered connect").connected = true;
        self.event(skein_io::Event::Connected { owner });
        self.drain();
    }
    pub fn refuse(&mut self, forge: u16) {
        let owner = self.newest(forge);
        self.event(skein_io::Event::Failed { owner, error: skein_io::Error::Refused });
        self.event(skein_io::Event::Closed { owner });
        drop(self.peers.remove(&owner));
        self.drain();
    }
    pub fn answer(&mut self, forge: u16, bytes: &[u8]) {
        let owner = self.newest(forge);
        self.peers.get_mut(&owner).expect("registered connect").wire.write(bytes);
        self.drain();
    }
    pub fn reset(&mut self, forge: u16) {
        let owner = self.newest(forge);
        self.event(skein_io::Event::Stream {
            owner,
            up: skein_lib::stream::Up::Failed(skein_lib::stream::Fault::Reset),
        });
        self.drain();
    }
    pub fn request_lines(&self, forge: u16) -> List<Box<[u8]>> {
        let mut lines = List::with_capacity(16);
        for (_, peer) in &self.peers {
            if peer.forge != forge {
                continue;
            }
            let mut start = 0;
            for (at, byte) in peer.wire.received.iter().enumerate() {
                if *byte == b'\n' {
                    let line = peer.wire.received.get(start..at).expect("line positions");
                    if line.starts_with(b"GET ") {
                        lines.push(Box::from(line.strip_suffix(b"\r").unwrap_or(line))).expect("fixture lines fit");
                    }
                    start = at.saturating_add(1);
                }
            }
        }
        lines
    }
    pub fn request_bytes(&self, forge: u16) -> Box<[u8]> {
        let mut length = 0_usize;
        for (_, peer) in &self.peers {
            if peer.forge == forge {
                length = length.checked_add(peer.wire.received.len()).expect("bounded requests");
            }
        }
        let mut out = skein_lib::Writer::new(length);
        for (_, peer) in &self.peers {
            if peer.forge == forge {
                out.put(&peer.wire.received).expect("requests measured");
            }
        }
        out.finish()
    }
    pub fn connections_opened(&self) -> u32 {
        self.opened
    }
    pub fn stopped(&self) -> Option<(u16, crate::Stopped)> {
        self.stopped
    }
    pub fn ready() -> Self {
        let mut h = Self::new(settings());
        h.connected(FORGE);
        h.answer(FORGE, &user_ok());
        h.answer(FORGE, &settings_ok());
        h
    }
    fn drain(&mut self) {
        let mut quiet = 0_u32;
        for _ in 0..1024_u32 {
            for _ in 0..self.io.len() {
                match self.io.pop().expect("queued request") {
                    skein_io::Request::Connect { owner, .. } => {
                        self.opened = self.opened.saturating_add(1);
                        let socket = skein_lib::Token::new(owner.raw().saturating_add(100));
                        drop(
                            self.peers
                                .insert(
                                    owner,
                                    Peer {
                                        forge: FORGE,
                                        socket,
                                        wire: skein_world::stream::Wire::new(owner.raw()),
                                        connected: false,
                                    },
                                )
                                .expect("fixture peers fit"),
                        );
                        self.event(skein_io::Event::Connecting { owner, socket });
                    }
                    skein_io::Request::Stream { stream, down } => {
                        for owner in self.owners().into_boxed() {
                            let peer = self.peers.get_mut(&owner).expect("fixture peer");
                            if peer.socket == stream {
                                peer.wire.take(down);
                                break;
                            }
                        }
                    }
                    skein_io::Request::Close { entity } | skein_io::Request::Abort { entity } => {
                        let mut owner = None;
                        for (token, peer) in &self.peers {
                            if peer.socket == entity {
                                owner = Some(*token);
                                break;
                            }
                        }
                        if let Some(owner) = owner {
                            self.event(skein_io::Event::Closed { owner });
                            drop(self.peers.remove(&owner));
                        }
                    }
                    skein_io::Request::Listen { .. }
                    | skein_io::Request::Bind { .. }
                    | skein_io::Request::Reject { .. }
                    | skein_io::Request::Output { .. }
                    | skein_io::Request::Spawn { .. }
                    | skein_io::Request::Signal { .. }
                    | skein_io::Request::Usage { .. }
                    | skein_io::Request::Window { .. } => unreachable!("forge only uses sockets"),
                }
            }
            let mut events = List::with_capacity(32);
            let mut moved = false;
            for owner in self.owners().into_boxed() {
                let peer = self.peers.get_mut(&owner).expect("fixture peer");
                let trace = peer.wire.trace.len();
                if peer.connected
                    && let Some(up) = peer.wire.answer()
                {
                    events.push(skein_io::Event::Stream { owner, up }).expect("fixture events fit");
                }
                moved |= peer.wire.trace.len() != trace;
            }
            moved |= !events.is_empty();
            for event in events.into_boxed() {
                self.event(event);
            }
            if !moved && self.io.is_empty() {
                quiet = quiet.saturating_add(1);
                if quiet == 32 {
                    break;
                }
            } else {
                quiet = 0;
            }
        }
    }
}
pub fn ok(body: &[u8]) -> Box<[u8]> {
    response(200, body)
}
pub fn response(code: u16, body: &[u8]) -> Box<[u8]> {
    let decimal = skein_lib::Decimal::of(u64::try_from(body.len()).expect("body length"));
    let mut out = skein_lib::Writer::new(
        body.len()
            .saturating_add(b"HTTP/1.1 200 OK\r\nContent-Length: ".len())
            .saturating_add(decimal.as_bytes().len())
            .saturating_add(b"\r\nDate: Wed, 01 Jan 2020 00:00:00 GMT\r\n\r\n".len()),
    );
    out.put(b"HTTP/1.1 ").expect("head fits");
    out.put(skein_lib::Decimal::of(u64::from(code)).as_bytes()).expect("code fits");
    out.put(b" OK\r\nContent-Length: ").expect("head fits");
    out.put(decimal.as_bytes()).expect("length fits");
    out.put(b"\r\nDate: Wed, 01 Jan 2020 00:00:00 GMT\r\n\r\n").expect("date fits");
    out.put(body).expect("body fits");
    out.finish()
}
pub fn ok_dated(body: &[u8]) -> Box<[u8]> {
    ok(body)
}
pub fn status(code: u16) -> Box<[u8]> {
    let decimal = skein_lib::Decimal::of(u64::from(code));
    let mut out = skein_lib::Writer::new(
        b"HTTP/1.1 "
            .len()
            .saturating_add(decimal.as_bytes().len())
            .saturating_add(b" Refused\r\nContent-Length: 0\r\n\r\n".len()),
    );
    out.put(b"HTTP/1.1 ").expect("head fits");
    out.put(skein_lib::Decimal::of(u64::from(code)).as_bytes()).expect("code fits");
    out.put(b" Refused\r\nContent-Length: 0\r\n\r\n").expect("head fits");
    out.finish()
}
pub fn user_ok() -> Box<[u8]> {
    ok(b"{\"id\":2,\"login\":\"writer\"}")
}
pub fn settings_ok() -> Box<[u8]> {
    ok(b"{\"max_response_items\":2,\"default_paging_num\":30}")
}
pub fn specimen() -> skein_forge::forgejo::plans::tables::Repository {
    skein_forge::forgejo::plans::tables::Repository {
        owner: Box::from(&b"fixture"[..]),
        name: Box::from(&b"specimen"[..]),
        object_format: skein_forge::forgejo::id::ObjectFormat::Sha1,
        merge_style: skein_forge::forgejo::document::MergeStyle::Squash,
        html_url: Box::from(&b"http://127.0.0.1:3000/fixture/specimen"[..]),
    }
}

/// One connection's scripted stream, shared by its step tests.
#[derive(Debug)]
pub struct ConnectionHarness {
    pub connection: crate::connection::Connection,
    pub env: Env<Limits>,
    pub forge: Forge,
    pub io: Queue<skein_io::Request>,
    pub sent: Box<[u8]>,
}
impl ConnectionHarness {
    pub fn new() -> Self {
        let limits = limits();
        let forge = settings().forges.into_boxed().into_iter().next().expect("one forge");
        let mut connection =
            crate::connection::Connection::new(&forge, &limits, shared(), Time::from_nanos(10_000_000_000));
        connection.socket = Some(skein_lib::Token::new(100));
        let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
        connection.connect(&env);
        Self { connection, env, forge, io: Queue::with_capacity(32), sent: Box::from([]) }
    }
    pub fn pump(&mut self) {
        self.connection.pump(&self.env, &mut self.io, 16);
    }
    pub fn answer(&mut self, response: &[u8]) -> List<skein_forge::forgejo::plans::Heard> {
        let mut wire = skein_world::stream::Wire::new(1);
        let mut written = false;
        let mut heard = List::with_capacity(16);
        for _ in 0_u32..1024 {
            self.pump();
            for _ in 0..self.io.len() {
                let request = self.io.pop().expect("queued io request");
                match request {
                    skein_io::Request::Stream { down, .. } => wire.take(down),
                    skein_io::Request::Listen { .. }
                    | skein_io::Request::Connect { .. }
                    | skein_io::Request::Bind { .. }
                    | skein_io::Request::Reject { .. }
                    | skein_io::Request::Output { .. }
                    | skein_io::Request::Spawn { .. }
                    | skein_io::Request::Signal { .. }
                    | skein_io::Request::Usage { .. }
                    | skein_io::Request::Window { .. }
                    | skein_io::Request::Close { .. }
                    | skein_io::Request::Abort { .. } => unreachable!("one connection emits stream requests"),
                }
            }
            if !written && self.connection.phase == crate::connection::Phase::Reading {
                wire.write(response);
                written = true;
            }
            if let Some(event) = wire.answer() {
                self.connection.stream(&self.env, event);
            }
            if let Some(event) = self.connection.heard() {
                heard.push(event).expect("fixture events fit");
            }
        }
        self.sent = Box::from(wire.received.as_slice());
        heard
    }
}
pub fn known() -> Harness {
    let mut h = Harness::ready();
    h.known(REPO, specimen());
    h
}

pub const HOOK_SECRET: &[u8] = b"fixture-hook-secret";
pub fn hook_settings() -> Settings {
    let mut settings = settings();
    let mut secrets = List::with_capacity(2);
    secrets.push((FORGE, Secret::new(Box::from(HOOK_SECRET)))).expect("secret");
    settings.webhooks =
        Some(crate::hooks::Webhooks { listener: skein_io::Addr::Ip(([127, 0, 0, 1], 7200).into()), secrets });
    settings
}
pub const HOOK_REPOSITORY: u64 = 2;
pub const HOOK_NUMBER: u64 = 7;
pub const HOOK_HEAD: [u8; 32] = [0x77; 32];
pub fn hook_body() -> skein_forge::forgejo::document::Body {
    skein_forge::forgejo::document::Body::Kept(Box::from(b"<!-- kit:key 0102 -->\nbody".as_slice()))
}
pub fn hook_id() -> skein_forge::forgejo::id::ObjectId {
    skein_forge::forgejo::id::ObjectId { format: skein_forge::forgejo::id::ObjectFormat::Sha256, bytes: HOOK_HEAD }
}
pub fn hook_ref(reference: &[u8]) -> skein_forge::forgejo::delivery::Delivery {
    skein_forge::forgejo::delivery::Delivery::Ref {
        repository: HOOK_REPOSITORY,
        reference: Box::from(reference),
        created: false,
        ref_type: skein_forge::forgejo::delivery::RefType::Branch,
    }
}

/// Trust shared by the configured TLS names; the HTTP harness captures
/// requests before their TLS record transport (the TLS world owns encryption).
pub fn trust() -> skein_tls::Config {
    let root = std::fs::read("../../tests/tls/fixtures/root.der").expect("shared TLS root");
    skein_tls::Config::from_der(&[root.into_boxed_slice()], &[]).expect("valid root").0
}

/// Exercise request construction and HTTP serialization on a scripted carrier.
/// The supplied forge still supplies its transport name to `Connection::start`.
pub fn request_bytes(settings: Settings) -> Box<[u8]> {
    let mut h = ConnectionHarness::new();
    h.forge = settings.forges.into_boxed().into_iter().next().expect("one configured forge");
    h.connection
        .start(&h.env, &h.forge, &settings.user_agent, skein_forge::forgejo::request::Request::User, None)
        .expect("valid request");
    h.answer(&user_ok());
    h.sent
}

/// The literal value of the sole named header on the HTTP wire.
pub fn header<'a>(sent: &'a [u8], name: &[u8]) -> &'a [u8] {
    let mut found = None;
    let mut rest = sent;
    while let Some(end) = skein_lib::bytes::find(rest, b"\r\n") {
        let line = rest.get(..end).expect("line end in request");
        if let Some(value) = line.strip_prefix(name) {
            assert!(found.is_none(), "header occurs once");
            found = Some(value);
        }
        rest = rest.get(end.checked_add(2).expect("line end fits")..).expect("next line");
    }
    found.expect("header sent")
}
