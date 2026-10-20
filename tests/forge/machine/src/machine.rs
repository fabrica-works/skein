//! Scripted HTTP responses, seeded streams and the forge component's io.
use crate::{fixtures, peer::Peer};
use skein_forge::api;
use skein_forge_connection::{Answered, Component, Event, Limits, Request, Settings, Stopped};
use skein_lib::{
    Duration, Env, Queue, Time, Token, Wall,
    stream::{Down, Fault, Up},
};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::Path,
};
/// The request's `METHOD target` and its scripted response.
#[derive(Debug)]
pub struct Answer {
    pub request: &'static str,
    pub response: Response,
}
impl Answer {
    /// A recorded response, served byte for byte.
    #[must_use]
    pub fn case(request: &'static str, case: &'static str) -> Self {
        Self { request, response: Response::Case(case) }
    }
}
/// The bytes or fault one request receives.
#[derive(Debug)]
pub enum Response {
    /// A recorded HTTP response.
    Case(&'static str),
    /// A literal HTTP response.
    Bytes(Vec<u8>),
    /// A reset after the request but before its response.
    Reset,
    /// A recorded response's prefix, followed by the stream's end.
    Cut { case: &'static str, at: usize },
    /// Wait for the call's deadline.
    Silent,
}
/// The next connection's outcome.
#[derive(Clone, Copy, Debug)]
pub enum Connect {
    Made,
    Refused,
}
/// An outside boundary held for a deadline or owner action.
#[derive(Clone, Copy, Debug)]
pub enum Pause {
    Connecting,
    /// Bytes of the next request delivered to the peer before holding io.
    Writing(usize),
    /// Bytes of the next response delivered before the peer goes silent.
    Reading(usize),
}
/// A byte fault in one generated request or response.
#[derive(Clone, Copy, Debug)]
pub struct Cut {
    pub request: usize,
    pub response: bool,
    pub at: usize,
    pub reset: bool,
}
/// The real component, with a scripted peer and io played by the world.
pub struct Machine {
    component: Component,
    env: Env<Limits>,
    above: Queue<Event>,
    below: Queue<skein_io::Request>,
    peers: BTreeMap<Token, Peer>,
    script: VecDeque<Answer>,
    connects: VecDeque<Connect>,
    answers: BTreeMap<u64, Answered>,
    requests: Vec<Vec<u8>>,
    stopped: Option<(u16, Stopped)>,
    opened: u32,
    aborts: u32,
    closed: bool,
    seed: u64,
    pause: Option<(usize, Pause)>,
    paused: Option<Token>,
    cut: Option<Cut>,
    deliveries: Vec<Answered>,
    closes: u32,
    held_response: Option<(Token, Vec<u8>)>,
    written: Vec<usize>,
    closed_events: u32,
}
impl std::fmt::Debug for Machine {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("Machine")
            .field("now", &self.env.now)
            .field("connections", &self.opened)
            .field("requests", &self.requests.len())
            .field("answers", &self.answers.len())
            .field("stopped", &self.stopped)
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}
impl Machine {
    /// Starts a component using the world's small fixture limits.
    #[must_use]
    pub fn new(seed: u64, script: Vec<Answer>) -> Self {
        Self::with(seed, fixtures::limits(), fixtures::settings(), script)
    }
    /// Prepends the recorded start reads.
    #[must_use]
    pub fn ready(seed: u64, mut script: Vec<Answer>) -> Self {
        let mut start =
            vec![Answer::case("GET /api/v1/user", "user"), Answer::case("GET /api/v1/settings/api", "settings-api")];
        start.append(&mut script);
        Self::new(seed, start)
    }
    /// Starts with other component limits or settings; shared bounds stay the fixture's.
    #[must_use]
    pub fn with(seed: u64, limits: Limits, settings: Settings, script: Vec<Answer>) -> Self {
        let mut world = Self {
            component: Component::new(settings, &limits, &fixtures::shared()).expect("fixture bounds agree"),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            above: Queue::with_capacity(256),
            below: Queue::with_capacity(256),
            peers: BTreeMap::new(),
            script: script.into(),
            connects: VecDeque::new(),
            answers: BTreeMap::new(),
            requests: Vec::new(),
            stopped: None,
            opened: 0,
            aborts: 0,
            closed: false,
            seed,
            pause: None,
            paused: None,
            cut: None,
            deliveries: Vec::new(),
            closes: 0,
            held_response: None,
            written: Vec::new(),
            closed_events: 0,
        };
        world.component.begin(&world.env, &mut world.above, &mut world.below);
        world
    }
    /// Overrides the next connect outcomes; later ones are made.
    pub fn connects(&mut self, connects: &[Connect]) {
        self.connects.extend(connects);
    }
    /// Queues one domain operation.
    pub fn call(&mut self, call: u64, repository: api::Repository, op: api::Op) {
        self.component.down(
            &self.env,
            Request::Call { call: Token::new(call), repository, op },
            &mut self.above,
            &mut self.below,
        );
    }
    /// Ticks streams and timers until this span has elapsed on the virtual clock.
    pub fn run_for(&mut self, span: Duration) {
        let end = self.env.now.saturating_add(span);
        while self.env.now < end {
            self.tick();
            let busy = self
                .peers
                .values()
                .any(|peer| peer.awaiting_request || peer.remaining > 0 || (peer.wire.eof && !peer.wire.ended));
            self.env.now = if busy || !self.below.is_empty() {
                self.env.now.saturating_add(Duration::from_nanos(10_000)).min(end)
            } else {
                self.component
                    .next_deadline()
                    .unwrap_or(end)
                    .max(self.env.now.saturating_add(Duration::from_nanos(10_000)))
                    .min(end)
            };
        }
        self.tick();
    }
    /// Holds the next exchange at an observable io boundary.
    pub fn pause(&mut self, pause: Pause) {
        self.pause_at(self.requests.len(), pause);
    }
    /// Holds one exchange, counting the start reads from zero.
    pub fn pause_at(&mut self, request: usize, pause: Pause) {
        self.pause = Some((request, pause));
    }
    /// Cuts the named generated exchange, counting start requests too.
    pub fn cut(&mut self, cut: Cut) {
        self.cut = Some(cut);
    }
    /// Drains bounded same-time work, then fires timers at exactly this time.
    pub fn advance_to(&mut self, time: Time) {
        self.settle();
        assert!(time >= self.env.now);
        self.env.now = time;
        self.settle();
    }
    fn settle(&mut self) {
        for _ in 0..1000 {
            self.tick();
        }
    }
    /// Delivers an owner action, preserving every answer offer in the history.
    pub fn action(&mut self, request: Request) {
        if let Request::Refused(answer) | Request::Returned(answer) = &request {
            assert!(self.answers.remove(&answer.call.raw()).is_some(), "owner holds a delivered answer");
        }
        self.component.down(&self.env, request, &mut self.above, &mut self.below);
        self.settle();
    }
    /// All answer deliveries, including refused answers offered again.
    #[must_use]
    pub fn deliveries(&self) -> &[Answered] {
        &self.deliveries
    }
    /// Live socket destinations observed from io requests.
    #[must_use]
    pub fn addresses(&self) -> Vec<skein_io::Addr> {
        self.peers.values().map(|peer| peer.address).collect()
    }
    /// Component Closed events observed so far.
    #[must_use]
    pub fn closed_events(&self) -> u32 {
        self.closed_events
    }
    /// Request bytes accepted by io, including a request cut before its framing ends.
    #[must_use]
    pub fn written_bytes(&self) -> &[usize] {
        &self.written
    }
    /// Sockets whose close or abort was requested.
    #[must_use]
    pub fn closes(&self) -> u32 {
        self.closes
    }
    /// Releases the rest of a response held at its byte boundary.
    pub fn release_response(&mut self) {
        let (owner, bytes) = self.held_response.take().expect("held response");
        let peer = self.peers.get_mut(&owner).expect("held peer");
        peer.remaining += bytes.len();
        peer.wire.write(&bytes);
        self.pause = None;
    }
    fn up(&mut self, event: skein_io::Event) {
        self.component.up(&self.env, event, &mut self.above, &mut self.below);
    }
    fn tick(&mut self) {
        self.events();
        while let Some(request) = self.below.pop() {
            self.io(request);
        }
        let owners: Vec<_> = self.peers.keys().copied().collect();
        for owner in owners {
            let request = self.peers.get_mut(&owner).expect("peer").request();
            if let Some(request) = request {
                self.respond(owner, request);
            }
            if self.paused == Some(owner) {
                continue;
            }
            if self.pause.is_some_and(|(at, pause)| at == self.requests.len() && matches!(pause, Pause::Writing(0))) {
                continue;
            }
            let peer = self.peers.get_mut(&owner).expect("peer");
            if peer.connected
                && let Some(up) = peer.wire.answer()
            {
                let up = if matches!(up, Up::End) && peer.reset { Up::Failed(Fault::Reset) } else { up };
                if let Up::Bytes(bytes) = &up {
                    peer.remaining = peer.remaining.saturating_sub(bytes.len());
                }
                self.up(skein_io::Event::Stream { owner, up });
            }
        }
        if self.component.next_deadline().is_some_and(|deadline| deadline <= self.env.now) {
            self.component.fire(&self.env, &mut self.above, &mut self.below);
        }
        while let Some(request) = self.below.pop() {
            self.io(request);
        }
        self.events();
        self.component.reclaim();
    }
    fn events(&mut self) {
        while let Some(event) = self.above.pop() {
            match event {
                Event::Answered(answer) => {
                    self.deliveries.push(answer.clone());
                    assert!(self.answers.insert(answer.call.raw(), answer).is_none(), "one answer per call");
                }
                Event::Stopped { forge, why } => {
                    assert!(self.stopped.replace((forge, why)).is_none(), "one stop");
                }
                Event::Hint(_) | Event::ListenerLost => panic!("webhooks absent"),
                Event::Closed => {
                    assert!(!self.closed, "one close");
                    self.closed = true;
                    self.closed_events += 1;
                }
            }
        }
    }
    fn io(&mut self, request: skein_io::Request) {
        match request {
            skein_io::Request::Connect { owner, addr } => {
                self.opened += 1;
                self.peers.insert(owner, Peer::new(self.seed.wrapping_add(owner.raw()), addr));
                self.up(skein_io::Event::Connecting { owner, socket: owner });
                match self.connects.pop_front().unwrap_or(Connect::Made) {
                    Connect::Made => {
                        if self.pause.is_some_and(|(_, pause)| matches!(pause, Pause::Connecting)) {
                            self.paused = Some(owner);
                            return;
                        }
                        self.peers.get_mut(&owner).expect("peer").connected = true;
                        self.up(skein_io::Event::Connected { owner });
                    }
                    Connect::Refused => {
                        self.up(skein_io::Event::Failed { owner, error: skein_io::Error::Refused });
                        self.peers.remove(&owner);
                        self.up(skein_io::Event::Closed { owner });
                    }
                }
            }
            skein_io::Request::Stream { stream, down } => {
                let peer = self.peers.get_mut(&stream).expect("stream peer");
                if matches!(&down, Down::Demand { room, .. } if *room > 0) || matches!(&down, Down::Send(_)) {
                    peer.awaiting_request = true;
                }
                let sent = match &down {
                    Down::Send(bytes) => bytes.len(),
                    Down::Demand { .. } | Down::Finish => 0,
                };
                let sending = sent > 0;
                let writing = matches!(&down, Down::Demand { room, .. } if *room > 0);
                peer.wire.take(down);
                if writing
                    && let Some(cut) = self.cut
                    && !cut.response
                    && cut.request == self.requests.len()
                    && cut.at == 0
                {
                    self.cut = None;
                    self.up(skein_io::Event::Stream { owner: stream, up: cut.end() });
                    return;
                }
                if sending {
                    let ordinal = self.requests.len();
                    self.written.resize(ordinal + 1, 0);
                    self.written[ordinal] += sent;
                    if let Some((at, Pause::Writing(bytes))) = self.pause
                        && at == self.requests.len()
                    {
                        peer.truncate(bytes);
                        self.written[ordinal] = self.written[ordinal].min(bytes);
                        self.paused = Some(stream);
                    }
                    if let Some(cut) = self.cut
                        && !cut.response
                        && cut.request == self.requests.len()
                        && peer.pending_bytes() >= cut.at
                    {
                        peer.truncate(cut.at);
                        self.written[ordinal] = self.written[ordinal].min(cut.at);
                        self.cut = None;
                        self.up(skein_io::Event::Stream { owner: stream, up: cut.end() });
                    }
                }
            }
            skein_io::Request::Abort { entity } => {
                self.aborts += 1;
                self.end(entity);
            }
            skein_io::Request::Close { entity } => self.end(entity),
            other @ (skein_io::Request::Listen { .. }
            | skein_io::Request::Bind { .. }
            | skein_io::Request::Reject { .. }
            | skein_io::Request::Output { .. }
            | skein_io::Request::Spawn { .. }
            | skein_io::Request::Signal { .. }
            | skein_io::Request::Usage { .. }
            | skein_io::Request::Window { .. }) => panic!("unexpected io {other:?}"),
        }
    }
    fn end(&mut self, owner: Token) {
        if self.peers.remove(&owner).is_some() {
            self.closes += 1;
            self.up(skein_io::Event::Closed { owner });
        }
    }
    fn respond(&mut self, owner: Token, request: Vec<u8>) {
        let answer = self.script.pop_front().expect("a scripted answer for each request");
        let line = request.split(|byte| *byte == b'\r').next().expect("request line");
        let asked = line.strip_suffix(b" HTTP/1.1").expect("HTTP version");
        assert_eq!(asked, answer.request.as_bytes(), "script request");
        let ordinal = self.requests.len();
        self.requests.push(request);
        let peer = self.peers.get_mut(&owner).expect("peer");
        peer.awaiting_request = false;
        let response = match answer.response {
            Response::Case(case) => Response::Bytes(case_bytes(case)),
            other @ (Response::Bytes(_) | Response::Reset | Response::Cut { .. } | Response::Silent) => other,
        };
        if let Response::Bytes(bytes) = &response {
            if let Some((at, Pause::Reading(length))) = self.pause
                && at == ordinal
            {
                peer.remaining = length;
                peer.wire.write(&bytes[..length]);
                self.held_response = Some((owner, bytes[length..].to_vec()));
                return;
            }
            if let Some(cut) = self.cut
                && cut.response
                && cut.request == ordinal
            {
                peer.remaining = cut.at;
                peer.wire.write(&bytes[..cut.at]);
                peer.wire.eof = true;
                peer.reset = cut.reset;
                self.cut = None;
                return;
            }
        }
        match response {
            Response::Case(case) => {
                let bytes = case_bytes(case);
                peer.remaining = bytes.len();
                peer.wire.write(&bytes);
            }
            Response::Bytes(bytes) => {
                peer.remaining = bytes.len();
                peer.wire.write(&bytes);
            }
            Response::Cut { case, at } => {
                peer.remaining = at;
                peer.wire.write(&case_bytes(case)[..at]);
                peer.wire.eof = true;
            }
            Response::Reset => self.up(skein_io::Event::Stream { owner, up: Up::Failed(Fault::Reset) }),
            Response::Silent => {}
        }
    }
    /// The call's single terminal answer, if it arrived.
    #[must_use]
    pub fn answered(&self, call: u64) -> Option<&Answered> {
        self.answers.get(&call)
    }
    /// Whether startup stopped, and its reason.
    #[must_use]
    pub fn stopped(&self) -> Option<(u16, Stopped)> {
        self.stopped
    }
    /// Every complete request, in arrival order.
    #[must_use]
    pub fn requests(&self) -> &[Vec<u8>] {
        &self.requests
    }
    /// The number of connect attempts.
    #[must_use]
    pub fn connections_opened(&self) -> u32 {
        self.opened
    }
    /// The number of sockets aborted.
    #[must_use]
    pub fn aborts(&self) -> u32 {
        self.aborts
    }
    /// Whether every scripted answer was assigned to a request.
    #[must_use]
    pub fn script_done(&self) -> bool {
        self.script.is_empty()
    }
    /// Closes the component, allowing at most 1,000 ticks for its terminal.
    pub fn close(&mut self) -> bool {
        self.component.down(&self.env, Request::Close, &mut self.above, &mut self.below);
        for _ in 0..1000 {
            self.tick();
            if self.closed {
                return true;
            }
        }
        false
    }
}
/// Loads an untouched recorded HTTP response, including its headers and framing.
#[must_use]
pub fn case_bytes(case: &str) -> Vec<u8> {
    fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../conformance/transcripts").join(case).join("response"))
        .expect("recorded response")
}

impl Cut {
    fn end(self) -> Up {
        if self.reset { Up::Failed(Fault::Reset) } else { Up::End }
    }
}
