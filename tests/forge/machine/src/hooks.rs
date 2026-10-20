//! A scripted socket world for the real webhook component.
use crate::fixtures;
use skein_fake_forge_protocol::delivery::Post;
use skein_forge_connection::{Component, Event, Limits, Request, Settings, hooks::Hinted};
use skein_lib::{Duration, Env, Queue, Time, Token, Wall, bytes};
use skein_world::stream::Wire;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct Peer {
    pub wire: Wire,
    pub owner: Option<Token>,
    pub closed: bool,
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
            self.up(skein_io::Event::Closed { owner });
        }
        self.settle();
    }
    pub fn lose(&mut self) {
        self.up(skein_io::Event::Failed { owner: self.owner, error: skein_io::Error::Refused });
        self.up(skein_io::Event::Closed { owner: self.owner });
        self.settle();
    }
    pub fn run_for(&mut self, duration: Duration) {
        self.env.now = self.env.now.saturating_add(duration);
        self.entry += 1;
        self.component.fire(&self.env, &mut self.above, &mut self.below);
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
    fn settle(&mut self) {
        let mut quiet = 0;
        for _ in 0..4096 {
            let mut moved = false;
            while let Some(request) = self.below.pop() {
                moved = true;
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
                        }
                        peer.wire.take(down);
                    }
                    skein_io::Request::Close { entity } | skein_io::Request::Abort { entity } => {
                        if entity == Token::new(90) {
                            self.up(skein_io::Event::Closed { owner: self.owner });
                        } else {
                            self.close(entity);
                        }
                    }
                    skein_io::Request::Reject { socket } => {
                        self.peers.get_mut(&socket).expect("accepted").closed = true;
                    }
                    skein_io::Request::Connect { .. } => {} // Starts are unrelated to deliveries in this world.
                    skein_io::Request::Output { .. }
                    | skein_io::Request::Spawn { .. }
                    | skein_io::Request::Signal { .. }
                    | skein_io::Request::Usage { .. }
                    | skein_io::Request::Window { .. } => panic!("hook socket io only"),
                }
            }
            while let Some(event) = self.above.pop() {
                moved = true;
                match event {
                    Event::Hint(hint) => {
                        self.ordered &= self.answers.get(&hint.connection).is_some_and(|entry| *entry <= self.entry);
                        self.hints.push(hint);
                    }
                    Event::ListenerLost => self.lost += 1,
                    Event::Closed => self.closed = true,
                    Event::Answered(_) | Event::Stopped { .. } => panic!("no outbound replies"),
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
