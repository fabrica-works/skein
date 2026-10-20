//! One byte peer; it reads a complete request before answering its script.
use skein_lib::stream::Down;
use skein_tls_world::{pki, server::Server};
use skein_world::stream::Wire;
#[derive(Debug)]
pub(crate) struct Peer {
    pub wire: Wire,
    pub address: skein_io::Addr,
    pub connected: bool,
    pub remaining: usize,
    script: Script,
    consumed: usize,
    demand: Demand,
    pub reset: bool,
    tls: Option<Server>,
    cut: Option<crate::tls::HandshakeCut>,
    pub flight_length: Option<usize>,
    pub failure: Option<bool>,
    pub flight_status: Option<crate::tls::CutStatus>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Script {
    Waiting,
    Responding,
    Terminal,
}
#[derive(Debug)]
struct Demand {
    room_pending: bool,
    answered_tick: bool,
}
impl Peer {
    pub fn new(seed: u64, address: skein_io::Addr, tls: bool, cut: Option<crate::tls::HandshakeCut>) -> Self {
        Self {
            wire: Wire::new(seed),
            address,
            connected: false,
            remaining: 0,
            script: Script::Waiting,
            consumed: 0,
            demand: Demand { room_pending: false, answered_tick: false },
            reset: false,
            tls: tls.then(|| Server::new(pki::Server::plain().config())),
            cut,
            flight_length: None,
            failure: None,
            flight_status: None,
        }
    }
    pub fn is_tls(&self) -> bool {
        self.tls.is_some()
    }
    pub fn handshaking(&self) -> bool {
        self.tls.as_ref().is_some_and(Server::handshaking)
    }
    pub fn handshake_flight_delivered(&self) -> bool {
        self.tls.is_some() && !self.wire.received.is_empty() && self.remaining == 0
    }
    fn received(&self) -> &[u8] {
        self.tls.as_ref().map_or(&self.wire.received, |tls| &tls.received)
    }
    pub fn terminate(&mut self) {
        self.script = Script::Terminal;
    }
    pub fn responding(&mut self) {
        self.script = Script::Responding;
    }
    pub fn begin_tick(&mut self) {
        self.demand.answered_tick = false;
    }
    pub fn busy(&self) -> bool {
        self.script == Script::Waiting
            || self.demand.room_pending
            || self.remaining > 0
            || (self.wire.eof && !self.wire.ended && self.demand.answered_tick)
    }
    pub fn answered(&mut self) {
        self.demand.answered_tick = true;
        self.demand.room_pending = false;
    }
    pub fn take(&mut self, down: Down) -> usize {
        if let Down::Demand { room, .. } = &down {
            self.demand.room_pending = *room > 0;
        }
        if self.script != Script::Terminal
            && (matches!(&down, Down::Demand { room, .. } if *room > 0) || matches!(&down, Down::Send(_)))
        {
            self.script = Script::Waiting;
        }
        let before = self.received().len();
        let ciphertext = self.wire.received.len();
        self.wire.take(down);
        if let Some(tls) = &mut self.tls {
            let mut received = &self.wire.received[ciphertext..];
            if let Some(cut) = self.cut
                && !cut.response
                && !received.is_empty()
            {
                self.flight_length = Some(received.len());
                let resolved = cut.point.resolve(received);
                self.flight_status = Some(resolved);
                if let crate::tls::CutStatus::At(at) = resolved {
                    received = &received[..at];
                    self.script = Script::Terminal;
                    self.failure = Some(cut.reset);
                }
                self.cut = None;
            }
            tls.receive(received);
            let mut flight = tls.transmit();
            if let Some(cut) = self.cut
                && cut.response
                && !flight.is_empty()
            {
                self.flight_length = Some(flight.len());
                let resolved = cut.point.resolve(&flight);
                self.flight_status = Some(resolved);
                if let crate::tls::CutStatus::At(at) = resolved {
                    flight.truncate(at);
                    self.script = Script::Terminal;
                    self.wire.eof = true;
                    self.reset = cut.reset;
                }
                self.cut = None;
            }
            self.remaining += flight.len();
            self.wire.write(&flight);
        }
        self.received().len() - before
    }
    pub fn write(&mut self, bytes: &[u8]) {
        let ciphertext = match &mut self.tls {
            Some(tls) => {
                tls.write(bytes);
                tls.transmit()
            }
            None => bytes.to_vec(),
        };
        self.remaining += ciphertext.len();
        self.wire.write(&ciphertext);
    }
    pub fn pending_bytes(&self) -> usize {
        self.received().len() - self.consumed
    }
    pub fn truncate(&mut self, bytes: usize) {
        let length = self.consumed + bytes;
        match &mut self.tls {
            Some(tls) => tls.received.truncate(length),
            None => self.wire.received.truncate(length),
        }
    }
    pub fn request(&mut self) -> Option<Vec<u8>> {
        if self.script == Script::Terminal {
            return None;
        }
        let pending = &self.received()[self.consumed..];
        let end = pending.windows(4).position(|bytes| bytes == b"\r\n\r\n")? + 4;
        let mut length = 0;
        for line in pending[..end].split(|byte| *byte == b'\n') {
            if let Some(value) = line.strip_prefix(b"Content-Length: ") {
                length = std::str::from_utf8(value.strip_suffix(b"\r").unwrap_or(value))
                    .expect("length text")
                    .parse::<usize>()
                    .expect("length number");
            }
        }
        let whole = end.checked_add(length).expect("bounded body");
        if pending.len() < whole {
            return None;
        }
        let request = pending[..whole].to_vec();
        self.consumed += whole;
        Some(request)
    }
}
