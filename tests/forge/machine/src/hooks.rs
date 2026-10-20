//! A scripted socket world for the real webhook component.
use crate::fixtures;
use skein_fake_forge_protocol::delivery::Post;
use skein_forge_connection::{Component, Event, Limits, Request, Settings, hooks::Hinted};
use skein_lib::{
    Duration, Env, Queue, Time, Token, Wall, bytes,
    stream::{Fault, Up},
};
use skein_world::stream::Wire;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct Peer {
    pub wire: Wire,
    pub owner: Option<Token>,
    pub closed: bool,
}
#[derive(Clone, Copy)]
enum Settlement {
    Automatic,
    Held,
}

pub struct Listener {
    component: Component,
    env: Env<Limits>,
    above: Queue<Event>,
    below: Queue<skein_io::Request>,
    owner: Token,
    peers: BTreeMap<Token, Peer>,
    next: u64,
    seed: u64,
    hints: Vec<Hinted>,
    entry: u64,
    answers: BTreeMap<Token, u64>,
    ordered: bool,
    pub lost: u32,
    pub closed: bool,
    settlement: Settlement,
    settling: BTreeMap<Token, (Token, Time)>,
    response_cut: Option<(usize, bool)>,
    response_bytes: usize,
    response_count: u64,
    hint_count: u64,
    event_order: Vec<&'static str>,
    expiries: u64,
    closed_events: u64,
}
impl Listener {
    #[must_use]
    pub fn new(seed: u64, limits: Limits, settings: Settings) -> Self {
        let mut w = Self {
            component: Component::new(settings, &limits, &fixtures::shared()).expect("hook component"),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            above: Queue::with_capacity(64),
            below: Queue::with_capacity(64),
            owner: Token::new(1_u64 << 59_u32),
            peers: BTreeMap::new(),
            next: 100,
            seed,
            hints: Vec::new(),
            entry: 0,
            answers: BTreeMap::new(),
            ordered: true,
            lost: 0,
            closed: false,
            settlement: Settlement::Automatic,
            settling: BTreeMap::new(),
            response_cut: None,
            response_bytes: 0,
            response_count: 0,
            hint_count: 0,
            event_order: Vec::new(),
            expiries: 0,
            closed_events: 0,
        };
        w.component.begin(&w.env, &mut w.above, &mut w.below);
        w.settle();
        w
    }
    fn up(&mut self, event: skein_io::Event) {
        self.entry += 1;
        self.component.up(&self.env, event, &mut self.above, &mut self.below);
    }
    pub fn accept(&mut self) -> Token {
        let socket = Token::new(self.next);
        self.next += 1;
        self.peers
            .insert(socket, Peer { wire: Wire::new(self.seed.wrapping_add(self.next)), owner: None, closed: false });
        self.up(skein_io::Event::Accepted {
            owner: self.owner,
            socket,
            peer: skein_io::Addr::Ip(([127, 0, 0, 1], 9000).into()),
        });
        self.settle();
        socket
    }
    pub fn send(&mut self, socket: Token, post: &Post) {
        self.write(socket, &post.head);
        self.write(socket, &post.body);
    }
    pub fn write(&mut self, socket: Token, bytes: &[u8]) {
        self.peers.get_mut(&socket).expect("peer").wire.write(bytes);
        self.settle();
    }
    pub fn request(&mut self, request: Request) {
        self.entry += 1;
        self.component.down(&self.env, request, &mut self.above, &mut self.below);
        self.settle();
    }
    pub fn refuse(&mut self) {
        let hint = self.hints.pop().expect("a hint");
        self.request(Request::RefusedHint(hint));
    }
    pub fn close(&mut self, socket: Token) {
        let peer = self.peers.get_mut(&socket).expect("peer");
        peer.closed = true;
        if let Some(owner) = peer.owner {
            if matches!(self.settlement, Settlement::Held) {
                self.settling.insert(socket, (owner, self.env.now.saturating_add(fixtures::shared().io.close_timeout)));
            } else {
                self.up(skein_io::Event::Closed { owner });
            }
        }
        self.settle();
    }
    pub fn lose(&mut self) {
        self.up(skein_io::Event::Failed { owner: self.owner, error: skein_io::Error::Refused });
        self.up(skein_io::Event::Closed { owner: self.owner });
        self.settle();
    }
    pub fn run_for(&mut self, duration: Duration) {
        self.advance_to(self.env.now.saturating_add(duration));
    }
    /// Moves the virtual clock to an exact deadline and settles bounded work.
    pub fn advance_to(&mut self, time: Time) {
        assert!(time >= self.env.now);
        self.env.now = time;
        let expired: Vec<_> =
            self.settling.iter().filter(|(_, (_, due))| *due <= time).map(|(socket, _)| *socket).collect();
        for socket in expired {
            let (owner, _) = self.settling.remove(&socket).expect("closing socket");
            self.expiries += 1;
            self.up(skein_io::Event::Closed { owner });
        }
        self.entry += 1;
        self.component.fire(&self.env, &mut self.above, &mut self.below);
        self.settle();
    }
    /// Holds io close acknowledgments until released or their io timeout.
    pub fn hold_settlement(&mut self) {
        self.settlement = Settlement::Held;
    }
    /// Acknowledges every requested close without advancing time.
    pub fn release_settlement(&mut self) {
        self.settlement = Settlement::Automatic;
        for (_, (owner, _)) in std::mem::take(&mut self.settling) {
            self.up(skein_io::Event::Closed { owner });
        }
        self.settle();
    }
    /// Independent owner-event history.
    #[must_use]
    pub fn event_order(&self) -> &[&'static str] {
        &self.event_order
    }
    /// Responses queued, hints offered, close requests/expiries, and unsettled sockets.
    #[must_use]
    pub fn counts(&self) -> [u64; 5] {
        [
            self.response_count,
            self.hint_count,
            self.expiries,
            u64::try_from(self.peers.values().filter(|peer| !peer.closed).count() + self.settling.len())
                .expect("small socket count"),
            self.closed_events,
        ]
    }
    /// Delivers a peer EOF or reset after the preceding bytes have settled.
    pub fn fault(&mut self, socket: Token, reset: bool) {
        let owner = self.peers.get(&socket).expect("peer").owner.expect("bound");
        self.up(skein_io::Event::Stream { owner, up: if reset { Up::Failed(Fault::Reset) } else { Up::End } });
        self.settle();
    }
    #[must_use]
    pub fn status(&self, socket: Token) -> Option<u16> {
        self.statuses(socket).last().copied()
    }
    #[must_use]
    pub fn statuses(&self, socket: Token) -> Vec<u16> {
        self.peers
            .get(&socket)
            .expect("peer")
            .wire
            .received
            .windows(12)
            .filter_map(|line| {
                if line.starts_with(b"HTTP/1.1 ") {
                    core::str::from_utf8(&line[9..12]).expect("status").parse().ok()
                } else {
                    None
                }
            })
            .collect()
    }
    #[must_use]
    pub fn hints(&self) -> &[Hinted] {
        &self.hints
    }
    #[must_use]
    pub fn hinted_after_answer(&self) -> bool {
        self.ordered
    }
    #[must_use]
    pub fn peer(&self, socket: Token) -> &Peer {
        self.peers.get(&socket).expect("peer")
    }
    #[must_use]
    pub fn debug(&self) -> String {
        format!("{:?} {:?} {:?}", self.component, self.above, self.below)
    }
    fn io(&mut self, request: skein_io::Request) {
        match request {
            skein_io::Request::Listen { owner, addr } => {
                self.owner = owner;
                self.up(skein_io::Event::Listening { owner, listener: Token::new(90), addr });
            }
            skein_io::Request::Bind { owner, socket } => {
                self.peers.get_mut(&socket).expect("accepted").owner = Some(owner);
            }
            skein_io::Request::Stream { stream, down } => {
                let peer = self.peers.get_mut(&stream).expect("hook peer");
                if matches!(&down, skein_lib::stream::Down::Send(bytes) if bytes.starts_with(b"HTTP/1.1 ")) {
                    self.answers.insert(peer.owner.expect("bound peer"), self.entry);
                    self.response_count += 1;
                }
                let sent = match &down {
                    skein_lib::stream::Down::Send(bytes) => bytes.len(),
                    skein_lib::stream::Down::Demand { .. } | skein_lib::stream::Down::Finish => 0,
                };
                peer.wire.take(down);
                self.response_bytes += sent;
                if let Some((at, reset)) = self.response_cut
                    && self.response_bytes >= at
                    && sent > 0
                {
                    peer.wire.received.truncate(at);
                    self.response_cut = None;
                    self.fault(stream, reset);
                }
            }
            skein_io::Request::Close { entity } | skein_io::Request::Abort { entity } => {
                if entity == Token::new(90) {
                    if matches!(self.settlement, Settlement::Held) {
                        self.settling.insert(
                            entity,
                            (self.owner, self.env.now.saturating_add(fixtures::shared().io.close_timeout)),
                        );
                    } else {
                        self.up(skein_io::Event::Closed { owner: self.owner });
                    }
                } else {
                    if matches!(self.settlement, Settlement::Automatic) {
                        self.expiries += 1;
                    }
                    self.close(entity);
                }
            }
            skein_io::Request::Reject { socket } => {
                self.peers.get_mut(&socket).expect("accepted").closed = true;
            }
            skein_io::Request::Connect { owner, .. } => {
                self.up(skein_io::Event::Failed { owner, error: skein_io::Error::Refused });
                self.up(skein_io::Event::Closed { owner });
            }
            skein_io::Request::Output { .. }
            | skein_io::Request::Spawn { .. }
            | skein_io::Request::Signal { .. }
            | skein_io::Request::Usage { .. }
            | skein_io::Request::Window { .. } => panic!("hook socket io only"),
        }
    }
    fn settle(&mut self) {
        let mut quiet = 0;
        for _ in 0..4096 {
            let mut moved = false;
            while let Some(request) = self.below.pop() {
                moved = true;
                self.io(request);
            }
            while let Some(event) = self.above.pop() {
                moved = true;
                match event {
                    Event::Hint(hint) => {
                        self.ordered &= self.answers.get(&hint.connection).is_some_and(|entry| *entry <= self.entry);
                        self.hints.push(hint);
                        self.hint_count += 1;
                        self.event_order.push("hint");
                    }
                    Event::ListenerLost => {
                        self.lost += 1;
                        self.event_order.push("lost");
                    }
                    Event::Closed => {
                        self.closed = true;
                        self.closed_events += 1;
                        self.event_order.push("closed");
                    }
                    Event::GitToken { .. } | Event::Connected { .. } | Event::Answered(_) | Event::Stopped { .. } => {
                        panic!("no outbound replies")
                    }
                }
            }
            let sockets: Vec<_> = self.peers.keys().copied().collect();
            for socket in sockets {
                let peer = self.peers.get_mut(&socket).expect("peer");
                let trace = peer.wire.trace.len();
                let event = if !peer.closed && peer.owner.is_some() { peer.wire.answer() } else { None };
                moved |= trace != peer.wire.trace.len();
                if let Some(up) = event {
                    let owner = peer.owner.expect("bound peer");
                    self.up(skein_io::Event::Stream { owner, up });
                    moved = true;
                }
            }
            if self.component.next_deadline().is_some_and(|time| time <= self.env.now) {
                self.entry += 1;
                self.component.fire(&self.env, &mut self.above, &mut self.below);
                moved = true;
            }
            self.component.reclaim();
            if moved {
                quiet = 0;
            } else {
                quiet += 1;
                if quiet == 32 {
                    return;
                }
            }
        }
        panic!("hook world did not settle");
    }
}
/// A signed request, using the same signer as the fake protocol's delivery renderer.
#[must_use]
pub fn signed(path: &[u8], event: &[u8], body: &[u8]) -> Post {
    let signature =
        skein_fake_forge_protocol::delivery::sign(&skein_lib::Secret::new(Box::from(fixtures::HOOK_SECRET)), body);
    let mut head = format!("POST {} HTTP/1.1\r\nHost: 127.0.0.1:7200\r\nContent-Length: {}\r\nX-Forgejo-Event: {}\r\nX-Forgejo-Signature: ",
        core::str::from_utf8(path).expect("path"), body.len(), core::str::from_utf8(event).expect("event")).into_bytes();
    head.extend_from_slice(&signature);
    head.extend_from_slice(b"\r\n\r\n");
    Post { head: head.into_boxed_slice(), body: bytes::copy_of(body) }
}
/// The fake protocol renderer's real push payload and signed request.
#[must_use]
pub fn push(branch: &[u8], commit: u64) -> Post {
    let limits = fixtures::fake_limits();
    let service =
        skein_fake_forge_protocol::Service::new(fixtures::hooked_protocol_config(), &limits, 7).expect("service");
    service.delivery(&fixtures::push_hook(branch, commit), &limits).expect("delivery")
}

/// EOF/reset at every signed request and generated response boundary.
#[must_use]
pub fn listener_faults(seed: u64) -> Vec<[u64; 6]> {
    let post = push(b"main", 7);
    let mut request = post.head.into_vec();
    request.extend_from_slice(&post.body);
    let mut control = Listener::new(seed, fixtures::hook_limits(), fixtures::hook_settings());
    let socket = control.accept();
    control.write(socket, &request);
    assert_eq!(control.status(socket), Some(204));
    assert_eq!(&control.counts()[..2], &[1, 1]);
    let response = control.peer(socket).wire.received.clone();
    let mut rows = Vec::new();
    for response_fault in [false, true] {
        let length = if response_fault { response.len() } else { request.len() };
        for reset in [false, true] {
            for at in 0..=length {
                let mut world = Listener::new(seed, fixtures::hook_limits(), fixtures::hook_settings());
                let socket = world.accept();
                if response_fault {
                    world.response_cut = Some((at, reset));
                    world.write(socket, &request);
                } else {
                    world.write(socket, &request[..at]);
                    world.fault(socket, reset);
                }
                let unverified = if !response_fault && at < request.len() { world.hint_count } else { 0 };
                world.request(Request::Close);
                world.request(Request::Offer);
                let counts = world.counts();
                assert_eq!(world.event_order.last(), Some(&"closed"));
                rows.push([
                    unverified,
                    counts[0].saturating_sub(1),
                    counts[1].saturating_sub(1),
                    u64::from(!world.ordered),
                    counts[3],
                    counts[4],
                ]);
            }
        }
    }
    rows
}
