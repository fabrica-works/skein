//! The scenario and settings drawn from a seed (tls.md, 5).
use crate::pki;
use skein_lib::stream::Fault;
use skein_lib::{Rng, Wall};
use skein_tls::client::{self, Certificate, Error, Limits};

/// What a run is about: the server, the client's view of it, and what the
/// two say.
#[derive(Clone, Debug)]
pub struct Scenario {
    pub server: pki::Server,
    /// The name the client asks for.
    pub name: String,
    /// The wall time the client checks the certificate at.
    pub wall: Wall,
    /// The protocols the client offers by ALPN.
    pub alpn: Vec<Vec<u8>>,
    /// What the side above sends.
    pub request: Vec<u8>,
    /// What the server sends, once it has the whole request, or at once if
    /// it is eager.
    pub response: Vec<u8>,
    pub eager: bool,
    /// Whether the server asks for a key update in the middle of its
    /// response, in TLS 1.3.
    pub key_update: bool,
    pub ending: Ending,
}

/// How the server ends, once its response is written.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ending {
    /// `close_notify`, then the stream's end.
    CloseNotify,
    /// The stream's end without `close_notify`: a truncation.
    Truncate,
    /// A byte of a record of the response changed on the way, then the
    /// stream's end.
    Corrupt,
    /// Nothing: the server waits.
    Silent,
}

impl Scenario {
    /// A request and a response of these lengths, from `rng`, to the leaf
    /// server, valid, ending with `close_notify`.
    #[must_use]
    pub fn exchange(rng: &mut Rng, request: usize, response: usize) -> Scenario {
        Scenario {
            server: pki::Server::plain(),
            name: "skein.test".into(),
            wall: pki::VALID,
            alpn: Vec::new(),
            request: text(rng, request),
            response: text(rng, response),
            eager: false,
            key_update: false,
            ending: Ending::CloseNotify,
        }
    }

    /// What the handshake must come to, when it fails.
    #[must_use]
    pub fn refusal(&self) -> Option<Error> {
        let named = match self.server.chain {
            pki::Chain::Leaf
            | pki::Chain::Untrusted
            | pki::Chain::Padded(_)
            | pki::Chain::SelfSigned
            | pki::Chain::CaAsLeaf
            | pki::Chain::ClientOnly => self.name == "skein.test" || self.name == "127.0.0.1",
            pki::Chain::Big => self.name == "big.skein.test",
        };
        // The chain is judged before the time and the name.
        let chain = match self.server.chain {
            pki::Chain::Untrusted | pki::Chain::SelfSigned => Some(Certificate::Issuer),
            pki::Chain::CaAsLeaf | pki::Chain::ClientOnly => Some(Certificate::Invalid),
            pki::Chain::Leaf | pki::Chain::Big | pki::Chain::Padded(_) => None,
        };
        let certificate = if chain.is_some() {
            chain
        } else if self.wall >= pki::EXPIRED {
            Some(Certificate::Expired)
        } else if self.wall <= pki::EARLY {
            Some(Certificate::NotYetValid)
        } else if !named {
            Some(Certificate::Name)
        } else {
            None
        };
        certificate.map(Error::Certificate)
    }
}

/// Text from `rng`: printable bytes, with line ends and quotes among them,
/// so that scans meet their delimiters.
#[must_use]
pub fn text(rng: &mut Rng, len: usize) -> Vec<u8> {
    let mut text = Vec::with_capacity(len);
    for _ in 0..len {
        let byte = match rng.below(40) {
            0 => b'\n',
            1 => b'\r',
            2 => b'"',
            _ => b'a' + u8::try_from(rng.below(26)).expect("a letter"),
        };
        text.push(byte);
    }
    text
}

/// How a world runs: the client's limits, and how its neighbours behave.
#[derive(Clone, Debug)]
pub struct Settings {
    pub limits: Limits,
    /// The side below's intake cap: at least the client's largest read.
    pub cap: u32,
    /// The side below's output cap: at least the client's largest room.
    pub output: u32,
    /// The longest piece the server's bytes arrive in.
    pub piece: u32,
    /// Per mille: how likely a piece arrives in an iteration.
    pub arrival: u32,
    /// Per mille: how likely room demanded is granted in an iteration.
    pub grant: u32,
    /// Per mille: how likely the side above acts in an iteration where it
    /// may.
    pub eagerness: u32,
    pub reads: Reads,
    /// Whether the side above finishes once its request is sent.
    pub finish: bool,
    /// When the side above closes, whatever the client is doing.
    pub close: Option<u64>,
    /// When the side above stops acting, and for how many iterations.
    pub stall: Option<(u64, u64)>,
    /// When the stream fails, if it does, and with what.
    pub failure: Option<(u64, Fault)>,
}

/// How the side above reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reads {
    /// Fills and scans of every size up to the limit, to LF, CRLF or a
    /// quote, and line scans: it may not see the last bytes, which no
    /// demand meets.
    Any,
    /// A byte at a time: it sees every byte.
    Bytes,
}

impl Settings {
    /// Neighbours that are slow and cut the bytes anywhere, under `limits`,
    /// but never fail the stream or close before the end.
    #[must_use]
    pub fn calm(rng: &mut Rng, limits: Limits) -> Settings {
        Settings {
            limits,
            cap: client::LARGEST_READ + draw(rng, 0, 4096),
            output: client::largest_room(&limits) + draw(rng, 0, 64),
            piece: draw(rng, 1, 4096),
            arrival: draw(rng, 100, 1000),
            grant: draw(rng, 100, 1000),
            eagerness: draw(rng, 100, 1000),
            reads: if rng.chance(200) { Reads::Bytes } else { Reads::Any },
            finish: rng.chance(500),
            close: None,
            stall: None,
            failure: None,
        }
    }

    /// Neighbours as [`calm`](Settings::calm), and sometimes a stream that
    /// fails, a close at any moment, or a side above that stops for a while.
    #[must_use]
    pub fn chaotic(rng: &mut Rng, limits: Limits) -> Settings {
        let mut settings = Settings::calm(rng, limits);
        if rng.chance(200) {
            settings.failure = Some((rng.below(400), fault(rng)));
        }
        if rng.chance(300) {
            settings.close = Some(rng.below(400));
        }
        if rng.chance(200) {
            settings.stall = Some((rng.below(400), rng.between(16, 256)));
        }
        settings
    }
}

/// Limits drawn at random: a read and room from a byte to a few records,
/// and records from one to several.
#[must_use]
pub fn limits(rng: &mut Rng) -> Limits {
    let most = if rng.chance(500) { 64 } else { 40_000 };
    let read = draw(rng, 1, most);
    let most = if rng.chance(500) { 64 } else { 40_000 };
    let send = draw(rng, 1, most);
    Limits { read, send, records: draw(rng, client::MAX_RECORD, 4 * client::MAX_RECORD) }
}

pub(super) fn fault(rng: &mut Rng) -> Fault {
    pick(rng, &[Fault::Reset, Fault::Invalid, Fault::Other])
}

/// One of `items`, drawn from `rng`.
pub fn pick<T: Copy>(rng: &mut Rng, items: &[T]) -> T {
    items[usize::try_from(rng.below(items.len() as u64)).expect("fits a usize")]
}

pub(super) fn draw(rng: &mut Rng, low: u32, high: u32) -> u32 {
    u32::try_from(rng.between(u64::from(low), u64::from(high))).expect("fits a u32")
}
