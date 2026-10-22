//! Refusing proxy and bounded network evidence (lab-browser.md, 2.4).
use skein_io as io;
use skein_lib::{Token, stream};
use std::{
    collections::{BTreeMap, VecDeque},
    net::{Ipv4Addr, SocketAddr},
};
pub const OWNER: Token = Token::new(1 << 39);
pub const LINE: usize = 8192;
const REPLY: &[u8] = b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
struct Connection {
    socket: Token,
    line: Vec<u8>,
    replying: bool,
}
pub struct Proxy {
    pub address: Option<SocketAddr>,
    listener: Option<Token>,
    next: u64,
    connections: BTreeMap<Token, Connection>,
    pub requests: VecDeque<io::Request>,
    refusals: Vec<String>,
    extra: u64,
    stopping: bool,
}
impl Proxy {
    pub fn new() -> Self {
        Self {
            address: None,
            listener: None,
            next: OWNER.raw() + 1,
            connections: BTreeMap::new(),
            requests: VecDeque::from([io::Request::Listen {
                owner: OWNER,
                addr: io::Addr::Ip(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
            }]),
            refusals: Vec::new(),
            extra: 0,
            stopping: false,
        }
    }
    pub fn owns(&self, owner: Token) -> bool {
        owner == OWNER || self.connections.contains_key(&owner)
    }
    pub fn take(&mut self) -> Vec<String> {
        let mut kept = std::mem::take(&mut self.refusals);
        if self.extra != 0 {
            kept.push(format!("{} more", self.extra));
            self.extra = 0;
        }
        kept
    }
    pub fn stop(&mut self) {
        self.stopping = true;
        if let Some(entity) = self.listener {
            self.requests.push_back(io::Request::Close { entity });
        }
        for connection in self.connections.values() {
            self.requests.push_back(io::Request::Abort { entity: connection.socket });
        }
    }
    pub fn closed(&self) -> bool {
        self.stopping && self.listener.is_none() && self.connections.is_empty() && self.requests.is_empty()
    }
    pub fn event(&mut self, event: io::Event) -> Result<(), String> {
        match event {
            io::Event::Listening { listener, addr, .. } => {
                self.listener = Some(listener);
                self.address = addr.socket();
            }
            io::Event::Accepted { socket, .. } => {
                if self.stopping || self.connections.len() >= 16 {
                    self.requests.push_back(io::Request::Reject { socket });
                } else {
                    let owner = Token::new(self.next);
                    self.next += 1;
                    self.connections.insert(owner, Connection { socket, line: Vec::new(), replying: false });
                    self.requests.push_back(io::Request::Bind { socket, owner });
                    self.demand(
                        socket,
                        stream::Read::Scan { until: stream::Delimiter::new(b"\r\n").expect("CRLF"), max: 8193 },
                        0,
                    );
                }
            }
            io::Event::Stream { owner, up } => {
                let connection = self.connections.get_mut(&owner).ok_or("proxy connection missing")?;
                let socket = connection.socket;
                match up {
                    stream::Up::Bytes(bytes) => {
                        connection.line.extend_from_slice(&bytes);
                        let host = first_line(&connection.line);
                        connection.replying = true;
                        if let Some(host) = host {
                            self.record(host);
                        }
                        self.demand(socket, stream::Read::Nothing, u32::try_from(REPLY.len()).expect("reply fits"));
                    }
                    stream::Up::Room => {
                        self.requests
                            .push_back(io::Request::Stream { stream: socket, down: stream::Down::Send(REPLY.into()) });
                        self.requests.push_back(io::Request::Close { entity: socket });
                    }
                    stream::Up::End => {
                        if !connection.replying {
                            self.requests.push_back(io::Request::Close { entity: socket });
                        }
                    }
                    stream::Up::Failed(_) => self.requests.push_back(io::Request::Abort { entity: socket }),
                }
            }
            io::Event::Closed { owner } => {
                if owner == OWNER {
                    self.listener = None;
                } else {
                    self.connections.remove(&owner);
                }
            }
            io::Event::Failed { owner, error } => {
                if owner == OWNER {
                    return Err(format!("proxy listener failed: {error:?}"));
                }
            }
            other @ (io::Event::Connecting { .. }
            | io::Event::Connected { .. }
            | io::Event::Output { .. }
            | io::Event::Spawned { .. }
            | io::Event::Exited { .. }
            | io::Event::Usage { .. }
            | io::Event::Resized
            | io::Event::Window { .. }
            | io::Event::Shutdown { .. }) => return Err(format!("unexpected proxy event {other:?}")),
        }
        Ok(())
    }
    fn record(&mut self, host: String) {
        if self.refusals.len() < 64 {
            self.refusals.push(host);
        } else {
            self.extra = self.extra.saturating_add(1);
        }
    }
    fn demand(&mut self, stream: Token, read: stream::Read, room: u32) {
        self.requests.push_back(io::Request::Stream { stream, down: stream::Down::Demand { read, room } });
    }
}
pub fn first_line(line: &[u8]) -> Option<String> {
    if line.len() > LINE || !line.ends_with(b"\r\n") {
        return None;
    }
    let line = std::str::from_utf8(line).ok()?.strip_suffix("\r\n")?;
    let mut parts = line.split(' ');
    let method = parts.next()?;
    let target = parts.next()?;
    if parts.next()? != "HTTP/1.1" || parts.next().is_some() {
        return None;
    }
    if method == "CONNECT" {
        authority(target, false)
    } else {
        if method.is_empty() || !method.bytes().all(|b| b.is_ascii_uppercase()) {
            return None;
        }
        let target = target.strip_prefix("http://")?;
        let host = target.split(['/', '?', '#']).next()?;
        authority(host, true)
    }
}
fn authority(host: &str, default: bool) -> Option<String> {
    if host.is_empty() || host.contains('@') || host.chars().any(char::is_whitespace) {
        return None;
    }
    let colon = if host.starts_with('[') {
        host.find(']').and_then(|end| if host.as_bytes().get(end + 1) == Some(&b':') { Some(end + 1) } else { None })
    } else {
        host.rfind(':')
    };
    if let Some(colon) = colon {
        let port = host[colon + 1..].parse::<u16>().ok()?;
        if colon == 0 {
            return None;
        }
        Some(format!("{}:{port}", &host[..colon]))
    } else if default {
        Some(format!("{host}:80"))
    } else {
        None
    }
}
