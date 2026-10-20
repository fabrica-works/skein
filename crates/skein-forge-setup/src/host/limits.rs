//! Admission and stack bounds for the host (forge-setup.md, 6.3).
use super::settings::{self, Settings, Transport};
use skein_forge::forgejo::limits as docs;
use skein_json::{tokenizer, writer};
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    /// The most configured forges, with unique numbers.
    pub forges: u32,
    /// Live runs at once, over every forge (6.3).
    pub runs: u32,
    /// The longest token or secret item (6.4), at least 64 bytes.
    pub token_bytes: u32,
    /// The most bytes of response heads, their framing included.
    pub head_bytes: u32,
    /// The most fields retained from a response head.
    pub headers: u32,
    /// The most incoming JSON body bytes, before decoding.
    pub body_bytes_in: u32,
    /// The most nested JSON containers.
    pub depth: u32,
    /// The body stream's largest piece.
    pub chunk: u32,
    /// The most bytes retained in a document's scalar string.
    pub string_bytes: u32,
    /// The most bytes retained in a document's body string.
    pub body_bytes: u32,
    /// The most elements retained in a document's inner list.
    pub inner: u32,
    /// The longest encoded request target.
    pub target_bytes: u32,
    /// The owner's io stream limits, checked against HTTP's demands.
    pub io: skein_io::Limits,
}
/// The documents' limits, with body strings bounded independently.
#[must_use]
pub fn documents(l: &Limits) -> docs::Limits {
    docs::Limits {
        json: tokenizer::Limits {
            depth: l.depth,
            string: l.body_bytes.max(l.string_bytes),
            number: 32,
            chunk: l.chunk,
            length: l.body_bytes_in,
        },
        writer: writer::Limits { depth: l.depth, length: l.body_bytes_in },
        string_bytes: l.string_bytes,
        body_bytes: l.body_bytes,
        inner: l.inner,
        target_bytes: l.target_bytes,
    }
}
/// HTTP's limits on plaintext, with complete heads sent within io's cap.
#[must_use]
pub fn http(l: &Limits) -> skein_http::client::Limits {
    skein_http::client::Limits {
        request: l.io.output,
        head: l.head_bytes,
        headers: l.headers,
        read: l.chunk.max(5),
        send: l.io.output,
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stacked {
    HttpOverIo,
    JsonOverHttp,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unusable {
    Zero,
    TokenBytes { token_bytes: u32 },
    Documents(docs::Problem),
    Stack { stacked: Stacked, demand: u32, cap: u32 },
    Forges { forges: u32 },
    Duplicate { forge: u16 },
    Address { forge: u16 },
    PlaintextName { forge: u16 },
    Request { forge: u16 },
    Given { forge: u16, refused: crate::given::Refused },
}
pub(super) fn check(l: &Limits, settings: &Settings) -> Result<(), Unusable> {
    for value in [
        l.forges,
        l.runs,
        l.token_bytes,
        l.head_bytes,
        l.headers,
        l.body_bytes_in,
        l.depth,
        l.chunk,
        l.string_bytes,
        l.body_bytes,
        l.inner,
        l.target_bytes,
    ] {
        if value == 0 {
            return Err(Unusable::Zero);
        }
    }
    if !l.io.is_usable() {
        return Err(Unusable::Zero);
    }
    if l.token_bytes < 64 {
        return Err(Unusable::TokenBytes { token_bytes: l.token_bytes });
    }
    if let Err(problem) = docs::check(&documents(l)) {
        return Err(Unusable::Documents(problem));
    }
    let h = http(l);
    stack(Stacked::HttpOverIo, skein_http::client::largest_read(&h), l.io.intake)?;
    stack(Stacked::HttpOverIo, skein_http::client::largest_room(&h), l.io.output)?;
    stack(Stacked::JsonOverHttp, tokenizer::largest_demand(&documents(l).json), h.read)?;
    if settings.forges.len() > l.forges {
        return Err(Unusable::Forges { forges: l.forges });
    }
    for (at, forge) in settings.forges.as_slice().iter().enumerate() {
        let number = forge.given.forge;
        for before in settings.forges.as_slice().get(..at).expect("prior forge positions") {
            if before.given.forge == number {
                return Err(Unusable::Duplicate { forge: number });
            }
        }
        let Transport::Plaintext { name } = &forge.transport;
        if let Some(name) = name
            && !plaintext_name(name)
        {
            return Err(Unusable::PlaintextName { forge: number });
        }
        match forge.address {
            skein_io::Addr::Ip(ip) if ip.ip().is_loopback() => (),
            skein_io::Addr::Ip(_) | skein_io::Addr::InProcess(_) => return Err(Unusable::Address { forge: number }),
        }
        let host = settings::host(forge).ok_or(Unusable::Request { forge: number })?;
        let head = u64::from(l.target_bytes)
            .saturating_add(u64::try_from(host.len()).expect("host length fits"))
            .saturating_add(u64::from(l.token_bytes))
            .saturating_add(u64::try_from(settings.user_agent.len()).expect("agent length fits"))
            .saturating_add(192);
        if !header_value(&settings.user_agent) || head > u64::from(h.request) {
            return Err(Unusable::Request { forge: number });
        }
        if let Err(refused) = crate::given::check(&forge.given) {
            return Err(Unusable::Given { forge: number, refused });
        }
    }
    Ok(())
}
fn stack(stacked: Stacked, demand: u32, cap: u32) -> Result<(), Unusable> {
    if demand > cap { Err(Unusable::Stack { stacked, demand, cap }) } else { Ok(()) }
}
fn plaintext_name(name: &[u8]) -> bool {
    if name.is_empty() {
        return false;
    }
    for byte in name {
        if *byte <= b' ' || *byte >= 127 || b"/:[]".contains(byte) {
            return false;
        }
    }
    true
}
fn header_value(value: &[u8]) -> bool {
    if value.is_empty() {
        return false;
    }
    for byte in value {
        if (*byte < 0x20 && *byte != b'\t') || *byte == 0x7f {
            return false;
        }
    }
    true
}
