//! Fixed pages for the Chromium suite, served by skein-http over skein-io.

use std::collections::{BTreeMap, VecDeque};
use std::net::{Ipv4Addr, SocketAddr};

use skein_http::Header;
use skein_http::server::{self, Body, Response, Reuse, Server};
use skein_io::{self as io, Request};
use skein_lib::stream::{self, Read};
use skein_lib::{Env, Queue, Token};

/// Separate token space for origin listeners and their connections.
pub const ORIGINS_FIRST: u64 = 1 << 40;
pub const OWNER: Token = Token::new(ORIGINS_FIRST);
#[derive(Clone)]
pub struct Page {
    pub path: Box<[u8]>,
    pub kind: Box<[u8]>,
    pub body: Box<[u8]>,
    pub headers: Vec<skein_ui_web::transport::Header>,
}
#[derive(Clone)]
pub struct Origin {
    pub host: Box<[u8]>,
    pub pages: Vec<Page>,
}

fn limits() -> server::Limits {
    server::Limits { head: 4096, headers: 32, body: 4096, read: 1024, response: 1024, send: 4096 }
}

struct Connection {
    socket: Token,
    server: Server,
    reply: Vec<u8>,
    sent: usize,
}

pub struct Pages {
    pub addr: Option<SocketAddr>,
    listener: Option<Token>,
    next_owner: u64,
    connections: BTreeMap<Token, Connection>,
    env: Env<server::Limits>,
    above: Queue<server::Event>,
    below: Queue<stream::Down>,
    requests: VecDeque<Request>,
    stopping: bool,
    pub pages: Vec<Page>,
    owner: Token,
    address: String,
    peers: Vec<(Box<[u8]>, String)>,
    verbatim: bool,
}

impl Pages {
    #[must_use]
    pub fn new(pages: Vec<Page>, now: skein_lib::Time, wall: skein_lib::Wall) -> Pages {
        Self::numbered(pages, now, wall, 0)
    }
    /// Pages served as their bytes, no placeholder replaced: the lab's browser serves a directory as it
    /// is (lab-browser.md, 2.3). `number` gives each origin a token space of its own.
    #[must_use]
    pub fn verbatim(pages: Vec<Page>, now: skein_lib::Time, wall: skein_lib::Wall, number: u64) -> Pages {
        Pages { verbatim: true, ..Self::numbered(pages, now, wall, number) }
    }
    pub(crate) fn numbered(pages: Vec<Page>, now: skein_lib::Time, wall: skein_lib::Wall, number: u64) -> Pages {
        let owner = Token::new(ORIGINS_FIRST + number * 1_000_000);
        let mut requests = VecDeque::new();
        requests
            .push_back(Request::Listen { owner, addr: skein_io::Addr::Ip(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))) });
        Pages {
            addr: None,
            listener: None,
            next_owner: owner.raw() + 1,
            connections: BTreeMap::new(),
            env: Env { now, wall, limits: limits() },
            above: Queue::with_capacity(16),
            below: Queue::with_capacity(16),
            requests,
            stopping: false,
            pages,
            owner,
            address: String::new(),
            peers: Vec::new(),
            verbatim: false,
        }
    }

    pub(crate) fn peers(&mut self, peers: &[(Box<[u8]>, String)]) {
        self.peers = peers.to_vec();
    }
    pub(crate) fn address(&mut self, address: &str) {
        self.address.clear();
        self.address.push_str(address);
    }
    pub fn update_time(&mut self, now: skein_lib::Time, wall: skein_lib::Wall) {
        self.env.now = now;
        self.env.wall = wall;
    }

    pub fn take(&mut self) -> Option<Request> {
        self.requests.pop_front()
    }

    #[must_use]
    pub fn pending(&self) -> bool {
        !self.requests.is_empty()
    }

    #[must_use]
    pub fn owns(&self, owner: Token) -> bool {
        owner == self.owner || self.connections.contains_key(&owner)
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.stopping && self.listener.is_none() && self.connections.is_empty() && self.requests.is_empty()
    }

    pub fn stop(&mut self) {
        if self.stopping {
            return;
        }
        self.stopping = true;
        self.requests.retain(|request| !matches!(request, Request::Listen { .. }));
        if let Some(listener) = self.listener {
            self.requests.push_back(Request::Close { entity: listener });
        }
        for connection in self.connections.values() {
            self.requests.push_back(Request::Close { entity: connection.socket });
        }
    }

    pub fn event(&mut self, event: io::Event) {
        match event {
            io::Event::Listening { owner, listener, addr } if owner == self.owner => {
                self.listener = Some(listener);
                let addr = addr.socket().expect("browser pages listen on loopback");
                self.addr = Some(addr);
                if self.stopping {
                    self.requests.push_back(Request::Close { entity: listener });
                }
            }
            io::Event::Accepted { owner, socket, .. } if owner == self.owner => {
                if self.stopping
                    || self.connections.len()
                        >= usize::try_from(super::harness::capacity::ORIGIN_CONNECTIONS).expect("connection limit fits")
                {
                    self.requests.push_back(Request::Reject { socket });
                    return;
                }
                let owner = Token::new(self.next_owner);
                self.next_owner += 1;
                self.connections.insert(
                    owner,
                    Connection { socket, server: Server::new(&self.env.limits), reply: Vec::new(), sent: 0 },
                );
                self.requests.push_back(Request::Bind { socket, owner });
                self.down(owner, server::Request::Next);
            }
            io::Event::Output { .. } => panic!("page server requests only classic output"),
            io::Event::Stream { owner, up } => self.up(owner, up),
            io::Event::Closed { owner } if owner == self.owner => self.listener = None,
            io::Event::Closed { owner } => {
                self.connections.remove(&owner);
            }
            io::Event::Failed { owner, error } => panic!("page server {owner:?} failed: {error:?}"),
            other @ (io::Event::Spawned { .. }
            | io::Event::Exited { .. }
            | io::Event::Listening { .. }
            | io::Event::Accepted { .. }
            | io::Event::Connecting { .. }
            | io::Event::Connected { .. }
            | io::Event::Usage { .. }
            | io::Event::Resized
            | io::Event::Window { .. }
            | io::Event::Shutdown { .. }) => panic!("unexpected page server event: {other:?}"),
        }
    }

    fn up(&mut self, owner: Token, up: stream::Up) {
        let conn = self.connections.get_mut(&owner).expect("page connection");
        server::up(&mut conn.server, &self.env, up, &mut self.above, &mut self.below);
        self.drain(owner);
    }

    fn down(&mut self, owner: Token, request: server::Request) {
        let conn = self.connections.get_mut(&owner).expect("page connection");
        server::down(&mut conn.server, &self.env, request, &mut self.above, &mut self.below);
        self.drain(owner);
    }

    fn drain(&mut self, owner: Token) {
        let socket = self.connections.get(&owner).expect("page connection").socket;
        while let Some(down) = self.below.pop() {
            self.requests.push_back(Request::Stream { stream: socket, down });
        }
        while let Some(event) = self.above.pop() {
            match event {
                server::Event::Call(call) => {
                    let page = self.pages.iter().find(|page| page.path == call.target);
                    let favicon = call.target.as_ref() == b"/favicon.ico";
                    let status = page
                        .map_or(if favicon { 204 } else { 404 }, |page| if page.kind.is_empty() { 204 } else { 200 });
                    let body = page.map_or_else(
                        || if favicon { Vec::new() } else { b"not found".to_vec() },
                        |page| {
                            if self.verbatim {
                                page.body.to_vec()
                            } else {
                                super::harness::substitute(&page.body, Some(&self.address), &self.peers).into_vec()
                            }
                        },
                    );
                    let mut headers = Vec::new();
                    if let Some(page) = page {
                        if !page.kind.is_empty() {
                            headers.push(Header { name: b"Content-Type".as_slice().into(), value: page.kind.clone() });
                        }
                        headers.extend(
                            page.headers
                                .iter()
                                .map(|(name, value)| Header { name: name.clone(), value: value.clone() }),
                        );
                    }
                    let conn = self.connections.get_mut(&owner).expect("page connection");
                    conn.reply.clone_from(&body);
                    conn.sent = 0;
                    self.down(owner, server::Request::Discard);
                    self.down(
                        owner,
                        server::Request::Respond(Response {
                            status,
                            headers: headers.into_boxed_slice(),
                            body: if status == 204 {
                                Body::None
                            } else {
                                Body::Length(u64::try_from(body.len()).expect("page length fits"))
                            },
                            close: true,
                        }),
                    );
                    if !body.is_empty() {
                        self.down(
                            owner,
                            server::Request::Reply(stream::Down::Demand {
                                read: Read::Nothing,
                                room: (u32::try_from(body.len()).expect("page length fits")).min(self.env.limits.send),
                            }),
                        );
                    }
                }
                server::Event::Reply(stream::Up::Room) => {
                    let conn = self.connections.get_mut(&owner).expect("page connection");
                    let end = (conn.sent + usize::try_from(self.env.limits.send).expect("send size fits"))
                        .min(conn.reply.len());
                    let bytes = Box::from(&conn.reply[conn.sent..end]);
                    conn.sent = end;
                    self.down(owner, server::Request::Reply(stream::Down::Send(bytes)));
                    if end == self.connections.get(&owner).expect("page connection").reply.len() {
                        self.down(owner, server::Request::Reply(stream::Down::Finish));
                    } else {
                        self.down(
                            owner,
                            server::Request::Reply(stream::Down::Demand {
                                read: Read::Nothing,
                                room: self.env.limits.send,
                            }),
                        );
                    }
                }
                server::Event::Done(Reuse::Close) | server::Event::Ended | server::Event::Failed(_) => {
                    self.down(owner, server::Request::Close);
                    self.requests.push_back(Request::Close { entity: socket });
                }
                server::Event::Closed | server::Event::Body(_) => {}
                server::Event::Done(Reuse::Keep) => panic!("page responses close their socket"),
                server::Event::Reply(other @ (stream::Up::Bytes(_) | stream::Up::End | stream::Up::Failed(_))) => {
                    panic!("unexpected page reply: {other:?}")
                }
                server::Event::Refused(reason) => panic!("page response refused: {reason:?}"),
            }
        }
    }
}
