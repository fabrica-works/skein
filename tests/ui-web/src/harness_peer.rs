//! One listener and one fixed reply, with observations the lending test keeps.
use skein_fake_peers::Listening;
use skein_io::{
    self as io,
    kernel::{Complete, Submit},
};
use skein_lib::{Duration, Env, Queue, Time, Token, Wall, stream};
use skein_world::Host;
use std::{
    cell::Cell,
    collections::BTreeMap,
    net::{Ipv4Addr, SocketAddr},
    rc::Rc,
};

const LISTENER: Token = Token::new(u64::MAX);
const REPLY: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\npeer ready";

pub struct Peer {
    io: io::Io,
    env: Env<io::Limits>,
    complete: Queue<Complete>,
    submit: Queue<Submit>,
    events: Queue<io::Event>,
    requests: Queue<io::Request>,
    listener: Option<Token>,
    address: Option<SocketAddr>,
    connections: BTreeMap<Token, Vec<u8>>,
    pub observations: Vec<Vec<u8>>,
    pub handle: Rc<Cell<usize>>,
    pub assert_request: bool,
    pub assert_shutdown: bool,
    stopping: bool,
}
impl Default for Peer {
    fn default() -> Self {
        Self::new()
    }
}
impl Peer {
    #[must_use]
    pub fn new() -> Self {
        // A listener and one connection, four simultaneous ops each plus two.
        let limits = io::Limits {
            sockets: 2,
            names: 0,
            refusals: 2,
            intake: 4096,
            receive: 4096,
            output: 4096,
            sends: 2,
            accepts: 1,
            backlog: 1,
            close_timeout: Duration::from_secs(2),
            retry: Duration::from_millis(10),
        };
        let mut requests = Queue::with_capacity(64);
        requests.push(io::Request::Listen { owner: LISTENER, addr: io::Addr::Ip((Ipv4Addr::LOCALHOST, 0).into()) });
        Self {
            io: io::Io::new(&limits),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            complete: Queue::with_capacity(64),
            submit: Queue::with_capacity(64),
            events: Queue::with_capacity(64),
            requests,
            listener: None,
            address: None,
            connections: BTreeMap::new(),
            observations: vec![],
            handle: Rc::new(Cell::new(0)),
            assert_request: false,
            assert_shutdown: false,
            stopping: false,
        }
    }
    fn demand(&mut self, socket: Token, read: stream::Read, room: u32) {
        self.requests.push(io::Request::Stream { stream: socket, down: stream::Down::Demand { read, room } });
    }
    fn event(&mut self, event: io::Event) {
        match event {
            io::Event::Listening { listener, addr, .. } => {
                self.listener = Some(listener);
                self.address = addr.socket();
            }
            io::Event::Accepted { socket, .. } => {
                self.connections.insert(socket, Vec::new());
                self.requests.push(io::Request::Bind { socket, owner: socket });
                self.demand(socket, stream::Read::Arrived { max: 4096 }, 0);
            }
            io::Event::Stream { owner, up: stream::Up::Bytes(bytes) } => {
                let request = self.connections.get_mut(&owner).expect("bound connection");
                request.extend_from_slice(&bytes);
                if request.ends_with(b"\r\n\r\n") {
                    self.observations.push(request.clone());
                    self.handle.set(self.handle.get() + 1);
                    self.demand(owner, stream::Read::Nothing, u32::try_from(REPLY.len()).expect("reply size"));
                    let assert_request = std::mem::take(&mut self.assert_request);
                    assert!(!assert_request, "fixture request assertion");
                } else {
                    self.demand(owner, stream::Read::Arrived { max: 4096 }, 0);
                }
            }
            io::Event::Stream { owner, up: stream::Up::Room } => {
                self.requests.push(io::Request::Stream { stream: owner, down: stream::Down::Send(REPLY.into()) });
                self.requests.push(io::Request::Stream { stream: owner, down: stream::Down::Finish });
                self.requests.push(io::Request::Close { entity: owner });
            }
            io::Event::Closed { owner } if owner == LISTENER => self.listener = None,
            io::Event::Closed { owner } => {
                self.connections.remove(&owner);
            }
            io::Event::Stream { up: stream::Up::End | stream::Up::Failed(_), .. } => {}
            io::Event::Failed { error, .. } => panic!("fixture io: {error:?}"),
            other => panic!("unexpected peer event: {other:?}"),
        }
    }
}
impl Listening for Peer {
    fn address(&self) -> SocketAddr {
        self.address.expect("successful Listen was driven")
    }
    fn shutdown(&mut self) {
        self.stopping = true;
        let fail = std::mem::take(&mut self.assert_shutdown);
        assert!(!fail, "fixture shutdown assertion");
    }
}
impl Host for Peer {
    fn completions(&mut self) -> &mut Queue<Complete> {
        &mut self.complete
    }
    fn submissions(&mut self) -> &mut Queue<Submit> {
        &mut self.submit
    }
    fn iterate(&mut self, now: Time, wall: Wall) {
        self.env.now = now;
        self.env.wall = wall;
        while self.io.is_ready() {
            io::resume(&mut self.io, &self.env, &mut self.events, &mut self.submit);
        }
        while let Some(complete) = self.complete.pop() {
            io::up(&mut self.io, &self.env, complete, &mut self.events, &mut self.submit);
        }
        while self.io.is_due(now) {
            io::fire(&mut self.io, &self.env, &mut self.events, &mut self.submit);
        }
        while let Some(event) = self.events.pop() {
            self.event(event);
        }
        if self.stopping
            && let Some(listener) = self.listener.take()
        {
            self.requests.push(io::Request::Close { entity: listener });
        }
        while self.io.takes() {
            let Some(request) = self.requests.pop() else { break };
            io::down(&mut self.io, &self.env, request, &mut self.submit);
        }
        self.io.reclaim();
    }
    fn work_pending(&self, _: Time) -> bool {
        self.io.is_ready()
            || !self.complete.is_empty()
            || !self.events.is_empty()
            || !self.requests.is_empty()
            || self.stopping && self.listener.is_some()
    }
    fn next_deadline(&self) -> Option<Time> {
        self.io.next_deadline()
    }
    fn next_policy_deadline(&self) -> Option<Time> {
        None
    }
    fn is_empty(&self) -> bool {
        self.io.is_empty()
            && self.complete.is_empty()
            && self.submit.is_empty()
            && self.events.is_empty()
            && self.requests.is_empty()
            && self.connections.is_empty()
    }
    fn worst_case(&self) -> u64 {
        io::worst_case(&self.env.limits).expect("fixture bound")
    }
    fn operations(&self) -> u32 {
        io::operations(&self.env.limits).expect("fixture operations")
    }
}
