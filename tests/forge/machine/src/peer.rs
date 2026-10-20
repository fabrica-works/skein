//! One byte peer; it reads a complete request before answering its script.
use skein_world::stream::Wire;
#[derive(Debug)]
pub(crate) struct Peer {
    pub wire: Wire,
    pub address: skein_io::Addr,
    pub connected: bool,
    pub remaining: usize,
    pub awaiting_request: bool,
    consumed: usize,
    pub reset: bool,
}
impl Peer {
    pub fn new(seed: u64, address: skein_io::Addr) -> Self {
        Self {
            wire: Wire::new(seed),
            address,
            connected: false,
            remaining: 0,
            awaiting_request: true,
            consumed: 0,
            reset: false,
        }
    }
    pub fn pending_bytes(&self) -> usize {
        self.wire.received.len() - self.consumed
    }
    pub fn truncate(&mut self, bytes: usize) {
        self.wire.received.truncate(self.consumed + bytes);
    }
    pub fn request(&mut self) -> Option<Vec<u8>> {
        let pending = &self.wire.received[self.consumed..];
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
