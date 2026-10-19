//! Forge component limits (forge-connection.md, sections 2, 3.5, 7 and 8).
use skein_forge::forgejo::limits as documents;
use skein_lib::Duration;

/// The component's limits (forge-connection.md, section 7).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    /// The forges served.
    pub forges: u32,
    /// The longest API token.
    pub token_bytes: u32,
    /// Forge connections in all: the client's `calls`.
    pub connections: u32,
    /// The largest response head read (HTTP's `head`).
    pub head_bytes: u32,
    /// The most fields of a response head (HTTP's own).
    pub headers: u32,
    /// The largest response body read (JSON's `length`): time and bytes, not memory.
    pub body_bytes_in: u32,
    /// The JSON nesting followed.
    pub depth: u32,
    /// The most bytes of a string's text demanded at once (JSON's own).
    pub chunk: u32,
    /// The longest name, title, context, description, link or path kept.
    pub string_bytes: u32,
    /// The longest body kept whole: at least twice the client's `op_bytes` plus the word's length and 15.
    pub body_bytes: u32,
    /// The most elements kept of a list inside one document (06's `inner`).
    pub inner: u32,
    /// The longest request target (06's `target_bytes`).
    pub target_bytes: u32,
    /// The repositories' table: at most the forge's `repositories`.
    pub repositories: u32,
    /// The users' table.
    pub users: u32,
    /// A call's, at most the client's `lifetime`; also each start read's.
    pub deadline: Duration,
    /// How long an idle connection is kept, below the forge's keep-alive timeout.
    pub idle: Duration,
    /// A forge's start asked again after this, doubling to `start_backoff_max`.
    pub start_backoff: Duration,
    pub start_backoff_max: Duration,
    /// Webhook connections, head and body bytes, request deadline and secret bytes.
    pub hooks: u32,
    pub hook_head_bytes: u32,
    pub hook_body_bytes: u32,
    pub hook_deadline: Duration,
    pub hook_secret_bytes: u32,
    /// TLS's own.
    pub tls: skein_tls::client::Limits,
}

/// What the component is given of its neighbours' limits
/// (forge-connection.md, 12, "Shared limits").
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Shared {
    /// The forge client's: `calls`, `rows`, `answer_bytes`, `op_bytes`, `lifetime`.
    pub calls: u32,
    pub rows: u32,
    pub answer_bytes: u32,
    pub op_bytes: u32,
    pub lifetime: Duration,
    /// The owner's largest retained name.
    pub name_bytes: u32,
    /// The forge's `repositories`.
    pub repositories: u32,
    /// io's caps, which the stacked machines' demands must fit.
    pub io: skein_io::Limits,
}

/// The listings whose page size the component chooses (forge-connection.md, 3.1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Listing {
    Items,
    Reviews,
    Statuses,
    Remarks,
    Branches,
    Collaborators,
    PullFiles,
}

/// A machine stacked on another, whose demand must fit the other's cap.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Stacked {
    HttpOverTls,
    HttpOverIo,
    TlsOverIo,
    JsonOverHttp,
    ServerOverIo,
    JsonOverServer,
}

/// Why the component cannot start (protocol/configuration.md, 7:
/// "each failure naming both limits").
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Unusable {
    /// A limit is 0, or the worst case passes a `u64`.
    Zero,
    /// Owner word outside its alphabet or length: terminal before begin.
    Word,
    /// Empty or unsafe plaintext name: terminal before begin; no input bytes.
    PlaintextName { forge: u16 },
    /// The secret bound must admit at least 32 bytes.
    SecretBytes { bytes: u32 },
    /// A secret has no forge, is empty, or exceeds its byte bound.
    Secret { forge: u16 },
    /// A webhook listener must be a loopback IP socket.
    Listener,
    /// `connections` is not the client's `calls`.
    Connections { connections: u32, calls: u32 },
    /// `deadline` passes the client's `lifetime`.
    Deadline { deadline: Duration, lifetime: Duration },
    /// `body_bytes` below the largest body the owner writes, its marker included.
    BodyBytes { body_bytes: u32, needed: u64 },
    /// `string_bytes` below the forge's `name_bytes`.
    StringBytes { string_bytes: u32, name_bytes: u32 },
    /// `repositories` above the forge's.
    Repositories { repositories: u32, forge: u32 },
    /// One of the listing's largest rows does not fit the client's `answer_bytes`.
    PageSize { listing: Listing },
    /// The documents' limits refused (06's `limits::check`).
    Documents(documents::Problem),
    /// A stacked machine's demand past the cap of the stream below.
    Stack { stacked: Stacked, demand: u32, cap: u32 },
    /// More forges than `forges`.
    Forges { forges: u32 },
    /// Two forges with one number.
    Duplicate { forge: u16 },
    /// An API token empty or past `token_bytes`.
    Token { forge: u16 },
    /// A user of 0.
    User { forge: u16 },
    /// Plaintext to an address not on loopback, or an in-process name.
    Address { forge: u16 },
    /// The longest request head (target, `Host`, token, agent) past HTTP's `request`.
    Request { forge: u16 },
}

use crate::settings::{Settings, Transport};
use alloc::boxed::Box;
use core::mem::size_of;
use skein_forge::api;
use skein_json::{tokenizer, writer};
/// Checks neighbours' agreements, stacked demands, then every configured forge.
pub fn check(l: &Limits, s: &Shared, settings: &Settings) -> Result<(), Unusable> {
    for value in [
        l.forges,
        l.token_bytes,
        l.connections,
        l.head_bytes,
        l.headers,
        l.body_bytes_in,
        l.depth,
        l.chunk,
        l.string_bytes,
        l.body_bytes,
        l.inner,
        l.target_bytes,
        l.repositories,
        l.users,
        l.hooks,
        l.hook_head_bytes,
        l.hook_body_bytes,
        l.hook_secret_bytes,
        l.tls.read,
        l.tls.send,
        l.tls.records,
    ] {
        if value == 0 {
            return Err(Unusable::Zero);
        }
    }
    for value in [l.deadline, l.idle, l.start_backoff, l.start_backoff_max, l.hook_deadline] {
        if value.as_nanos() == 0 {
            return Err(Unusable::Zero);
        }
    }
    if l.hook_secret_bytes < 32 {
        return Err(Unusable::SecretBytes { bytes: l.hook_secret_bytes });
    }
    let server = hook_http(l);
    skein_http::server::worst_case(&server).ok_or(Unusable::Zero)?;
    stack(Stacked::ServerOverIo, skein_http::server::largest_read(&server), s.io.intake)?;
    stack(Stacked::ServerOverIo, skein_http::server::largest_room(&server), s.io.output)?;
    stack(Stacked::JsonOverServer, tokenizer::largest_demand(&hook_documents(l).json), server.read)?;
    if l.connections != s.calls {
        return Err(Unusable::Connections { connections: l.connections, calls: s.calls });
    }
    if l.deadline > s.lifetime {
        return Err(Unusable::Deadline { deadline: l.deadline, lifetime: s.lifetime });
    }
    if !skein_forge::marker::valid_word(&settings.word) {
        return Err(Unusable::Word);
    }
    let needed = u64::from(skein_forge::marker::length(&settings.word, s.op_bytes).ok_or(Unusable::Zero)?);
    if u64::from(l.body_bytes) < needed {
        return Err(Unusable::BodyBytes { body_bytes: l.body_bytes, needed });
    }
    if l.string_bytes < s.name_bytes {
        return Err(Unusable::StringBytes { string_bytes: l.string_bytes, name_bytes: s.name_bytes });
    }
    if l.repositories > s.repositories {
        return Err(Unusable::Repositories { repositories: l.repositories, forge: s.repositories });
    }
    if let Err(problem) = documents::check(&documents(l)) {
        return Err(Unusable::Documents(problem));
    }
    for listing in [
        Listing::Items,
        Listing::Reviews,
        Listing::Statuses,
        Listing::Remarks,
        Listing::Branches,
        Listing::Collaborators,
        Listing::PullFiles,
    ] {
        if page_size(listing, u32::MAX, l, s) == 0 {
            return Err(Unusable::PageSize { listing });
        }
    }
    for tls in [true, false] {
        let h = http(l, tls, &s.io);
        let (stacked, read, send) = if tls {
            (Stacked::HttpOverTls, l.tls.read, l.tls.send)
        } else {
            (Stacked::HttpOverIo, s.io.intake, s.io.output)
        };
        stack(stacked, skein_http::client::largest_read(&h), read)?;
        stack(stacked, skein_http::client::largest_room(&h), send)?;
    }
    stack(Stacked::TlsOverIo, skein_tls::client::LARGEST_READ, s.io.intake)?;
    stack(Stacked::TlsOverIo, skein_tls::client::largest_room(&l.tls), s.io.output)?;
    stack(Stacked::JsonOverHttp, tokenizer::largest_demand(&documents(l).json), http(l, false, &s.io).read)?;
    check_settings(l, s, settings)?;
    worst_case(l, s, settings.forges.len(), crate::settings::owned_bytes(settings).ok_or(Unusable::Zero)?)
        .ok_or(Unusable::Zero)?;
    Ok(())
}
fn check_settings(l: &Limits, s: &Shared, settings: &Settings) -> Result<(), Unusable> {
    if settings.forges.len() > l.forges {
        return Err(Unusable::Forges { forges: l.forges });
    }
    for (at, forge) in settings.forges.as_slice().iter().enumerate() {
        for before in settings.forges.as_slice().get(..at).expect("prior forge positions") {
            if before.number == forge.number {
                return Err(Unusable::Duplicate { forge: forge.number });
            }
        }
        let skein_forge::credential::Credential::Token(token) = &forge.credential;
        if token.is_empty() || token.len() > usize::try_from(l.token_bytes).expect("u32 fits") {
            return Err(Unusable::Token { forge: forge.number });
        }
        if forge.user == 0 {
            return Err(Unusable::User { forge: forge.number });
        }
        let tls = match &forge.transport {
            Transport::Plaintext { name } => {
                if let Some(name) = name
                    && !plaintext_name(name)
                {
                    return Err(Unusable::PlaintextName { forge: forge.number });
                }
                false
            }
            Transport::Tls { .. } => true,
        };
        match forge.address {
            skein_io::Addr::Ip(ip) => {
                if !tls && !ip.ip().is_loopback() {
                    return Err(Unusable::Address { forge: forge.number });
                }
            }
            skein_io::Addr::InProcess(_) => return Err(Unusable::Address { forge: forge.number }),
        }
        // Longest method, target, seven fields and final empty line. Decimal lengths
        // use u64's longest spelling; headers here reserve their complete framing.
        let head = u64::from(l.target_bytes)
            .checked_add(
                u64::try_from(crate::settings::host(forge).ok_or(Unusable::Zero)?.len()).ok().ok_or(Unusable::Zero)?,
            )
            .ok_or(Unusable::Zero)?;
        let head = head
            .checked_add(u64::try_from(token.len()).ok().ok_or(Unusable::Zero)?)
            .ok_or(Unusable::Zero)?
            .checked_add(192)
            .ok_or(Unusable::Zero)?;
        if head > u64::from(http(l, tls, &s.io).request) {
            return Err(Unusable::Request { forge: forge.number });
        }
    }
    if let Some(hooks) = &settings.webhooks {
        if hooks.secrets.len() > l.forges {
            return Err(Unusable::Forges { forges: l.forges });
        }
        match hooks.listener {
            skein_io::Addr::Ip(ip) if ip.ip().is_loopback() => {}
            skein_io::Addr::Ip(_) | skein_io::Addr::InProcess(_) => return Err(Unusable::Listener),
        }
        for (forge, secret) in &hooks.secrets {
            let mut known = false;
            for configured in &settings.forges {
                known |= configured.number == *forge;
            }
            if !known || secret.is_empty() || secret.len() > usize::try_from(l.hook_secret_bytes).expect("u32 fits") {
                return Err(Unusable::Secret { forge: *forge });
            }
        }
    }
    Ok(())
}
fn stack(stacked: Stacked, demand: u32, cap: u32) -> Result<(), Unusable> {
    if demand > cap { Err(Unusable::Stack { stacked, demand, cap }) } else { Ok(()) }
}
/// A listing's largest fixed row and owned payloads, measured as the client measures them.
#[must_use]
pub fn page_size(listing: Listing, cap: u32, l: &Limits, s: &Shared) -> u32 {
    let Some(row) = largest_row(listing, l) else {
        return 0;
    };
    let room = u64::from(s.answer_bytes).checked_div(row).unwrap_or(0);
    s.rows.min(cap).min(u32::try_from(room).unwrap_or(u32::MAX))
}
fn largest_row(listing: Listing, l: &Limits) -> Option<u64> {
    let string = u64::from(l.string_bytes);
    let body = u64::from(l.body_bytes);
    let key = body / 2; // Every valid marker key is within the retained body's framing.
    let (fixed, payload) = match listing {
        Listing::Items => (
            size_of::<api::Summary>(),
            body.checked_add(key)?.checked_add(string)?.checked_add(
                u64::from(l.inner).checked_mul(string.checked_add(u64::try_from(size_of::<Box<[u8]>>()).ok()?)?)?,
            )?,
        ),
        Listing::Reviews => (size_of::<api::Review>(), body.checked_add(key)?),
        Listing::Statuses => (size_of::<api::Status>(), string.checked_mul(3)?),
        Listing::Remarks => (size_of::<api::Remark>(), body.checked_add(string)?),
        Listing::Branches => (size_of::<Box<[u8]>>(), string),
        Listing::Collaborators => (size_of::<api::Collaborator>(), 0),
        Listing::PullFiles => (size_of::<api::File>(), string.checked_add(body.checked_mul(2)?)?),
    };
    u64::try_from(fixed).ok()?.checked_add(payload)
}
/// Derived Forgejo document limits, with body strings capped independently of incoming bytes.
#[must_use]
pub fn documents(l: &Limits) -> documents::Limits {
    documents::Limits {
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
/// The body stream's demands are chunked; heads use the stream's complete send cap.
#[must_use]
pub fn http(l: &Limits, tls: bool, io: &skein_io::Limits) -> skein_http::client::Limits {
    skein_http::client::Limits {
        request: if tls { l.tls.send } else { io.output },
        head: l.head_bytes,
        headers: l.headers,
        read: l.chunk.max(5),
        send: if tls { l.tls.send } else { io.output },
    }
}
/// The webhook server reads bounded pieces and answers with no response body.
#[must_use]
pub fn hook_http(l: &Limits) -> skein_http::server::Limits {
    skein_http::server::Limits {
        head: l.hook_head_bytes,
        headers: l.headers,
        body: u64::from(l.hook_body_bytes),
        read: l.chunk.max(5),
        response: 256,
        send: 1,
    }
}
/// Webhook documents have their own incoming body bound.
#[must_use]
pub fn hook_documents(l: &Limits) -> documents::Limits {
    let mut docs = documents(l);
    docs.json.length = l.hook_body_bytes;
    docs
}
/// Conservative complete heap, pricing TLS on every connection because this interface
/// knows the forge count, not their transports. Raw growth includes its doubling overlap.
#[must_use]
pub fn worst_case(l: &Limits, s: &Shared, forges: u32, owned_bytes: u64) -> Option<u64> {
    use crate::{
        boundary::Answered,
        component::{Held, Waiting},
        settings::Forge,
        start::Start,
    };
    use skein_forge::forgejo::plans::tables::{RepositoryEntry, UserEntry};
    use skein_lib::{Id, Intake, List, Map, Queue, Slab, Token};
    let docs = documents(l);
    let raw = u64::from(s.answer_bytes).checked_mul(2)?;
    // Conservatively covers the boxed decoder state and both builder/delivered
    // rows: 32 scalar strings, two bodies, three lists of texts/users, and the
    // walker's depth stack. These envelopes exceed every 06.x decoder's fields.
    let inner = u64::from(l.string_bytes)
        .checked_mul(3)?
        .checked_add(u64::try_from(size_of::<skein_forge::forgejo::document::User>()).ok()?.checked_mul(2)?)?
        .checked_add(u64::try_from(size_of::<Box<[u8]>>()).ok()?.checked_mul(2)?)?;
    let decoder = u64::from(l.string_bytes)
        .checked_mul(32)?
        .checked_add(u64::from(l.inner).checked_mul(inner)?)?
        .checked_add(u64::from(l.body_bytes).checked_mul(2)?)?
        .checked_add(u64::from(l.depth).checked_mul(128)?)?
        .checked_add(4096)?;
    let socket = Intake::worst_case(s.io.intake)?
        .checked_add(u64::from(s.io.receive))?
        .checked_add(u64::from(s.io.output))?
        .checked_add(Queue::<Box<[u8]>>::worst_case(s.io.sends)?)?;
    let connection = skein_http::client::worst_case(&http(l, true, &s.io))?
        .max(skein_http::client::worst_case(&http(l, false, &s.io))?)
        .checked_add(skein_tls::client::worst_case(&l.tls)?)?
        .checked_add(tokenizer::worst_case(&docs.json)?)?
        .checked_add(decoder)?
        .checked_add(raw)?
        .checked_add(crate::connection::routing_bytes()?)?
        .checked_add(u64::from(l.tls.send.max(s.io.output)))?
        .checked_add(u64::from(l.body_bytes).checked_mul(6)?.checked_add(1024)?)?
        .checked_add(socket)?;
    let hooks = hook_heap(l, decoder, socket, forges)?;
    let calls = Slab::<Waiting>::worst_case(s.calls)?
        .checked_add(Map::<Token, Token>::worst_case(l.connections)?)?
        .checked_add(List::<(Token, Id<Waiting>)>::worst_case(s.calls)?)?
        .checked_add(Map::<Token, Id<Waiting>>::worst_case(s.calls)?)?
        .checked_add(
            u64::from(s.calls).checked_mul(
                u64::from(s.op_bytes)
                    .checked_mul(3)?
                    .checked_add(u64::try_from(size_of::<skein_forge::forgejo::plans::reads::Plan>()).ok()?)?
                    .checked_add(u64::try_from(size_of::<skein_forge::forgejo::plans::writes::Plan>()).ok()?)?
                    .checked_add(List::<u64>::worst_case(s.op_bytes.checked_div(8)?)?)?
                    .checked_add(List::<Box<[u8]>>::worst_case(s.op_bytes.checked_div(8)?.checked_add(l.inner)?)?)?
                    .checked_add(
                        u64::from(s.op_bytes.checked_div(8)?.checked_add(l.inner)?)
                            .checked_mul(u64::from(l.string_bytes))?,
                    )?
                    .checked_add(u64::from(s.answer_bytes))?
                    .checked_add(List::<skein_forge::forgejo::document::ChangedFile>::worst_case(s.rows)?)?
                    .checked_add(u64::from(s.rows).checked_mul(u64::from(l.string_bytes).checked_mul(2)?)?)?
                    .checked_add(List::<api::File>::worst_case(s.rows)?)?
                    .checked_add(List::<api::Commit>::worst_case(s.rows)?)?
                    .checked_add(u64::from(l.string_bytes).checked_mul(4)?)?
                    .checked_add(u64::from(l.string_bytes).checked_mul(u64::from(l.inner).checked_add(5)?)?)?
                    .checked_add(
                        u64::from(l.inner)
                            .checked_mul(u64::try_from(size_of::<skein_forge::forgejo::document::User>()).ok()?)?,
                    )?,
            )?,
        )?;
    let held = Map::<Token, Held>::worst_case(s.calls)?
        .checked_add(u64::from(s.calls).checked_mul(u64::from(s.answer_bytes))?)?;
    let repositories = Map::<api::Repository, RepositoryEntry>::worst_case(l.repositories)?
        .checked_add(u64::from(l.repositories).checked_mul(u64::from(l.string_bytes).checked_mul(3)?)?)?;
    let users = Map::<(u16, u64), UserEntry>::worst_case(l.users)?
        .checked_add(u64::from(l.users).checked_mul(u64::from(l.string_bytes))?)?;
    let configured =
        List::<Forge>::worst_case(l.forges)?.checked_add(List::<Start>::worst_case(l.forges)?)?.checked_add(
            u64::from(forges).checked_mul(
                u64::from(l.token_bytes)
                    .checked_add(u64::from(l.tls.send.max(s.io.output)))?
                    .checked_add(skein_tls::client::worst_case(&l.tls)?)?,
            )?,
        )?;
    connection
        .checked_mul(u64::from(l.connections))?
        .checked_add(Map::<Token, Id<crate::connection::Connection>>::worst_case(l.connections)?)?
        .checked_add(Slab::<crate::connection::Connection>::worst_case(l.connections)?)?
        .checked_add(List::<Option<Token>>::worst_case(l.forges)?)?
        .checked_add(List::<Option<skein_forge::forgejo::plans::start::Plan>>::worst_case(l.forges)?)?
        .checked_add(hooks)?
        .checked_add(calls)?
        .checked_add(held)?
        .checked_add(repositories)?
        .checked_add(users)?
        .checked_add(configured)?
        .checked_add(u64::try_from(size_of::<Box<[u8]>>()).ok()?.checked_mul(2)?)?
        .checked_add(owned_bytes)?
        .checked_add(16)?
        .checked_add(u64::try_from(size_of::<Answered>()).ok()?)
}

fn hook_heap(l: &Limits, decoder: u64, socket: u64, forges: u32) -> Option<u64> {
    use skein_lib::{List, Queue};
    skein_http::server::worst_case(&hook_http(l))?
        .checked_add(tokenizer::worst_case(&hook_documents(l).json)?)?
        .checked_add(decoder)?
        .checked_add(socket)?
        .checked_add(u64::try_from(size_of::<skein_lib::sha256::Hmac>()).ok()?)?
        .checked_add(u64::try_from(size_of::<api::Hint>()).ok()?)?
        .checked_add(u64::from(l.string_bytes).checked_add(u64::from(l.body_bytes))?)?
        .checked_add(Queue::<skein_http::server::Event>::worst_case(4)?)?
        .checked_add(Queue::<skein_lib::stream::Down>::worst_case(4)?)?
        .checked_add(Queue::<tokenizer::Event>::worst_case(1)?)?
        .checked_add(Queue::<skein_lib::stream::Down>::worst_case(1)?)?
        .checked_add(u64::from(l.hook_head_bytes))?
        .checked_add(u64::from(l.chunk).checked_mul(2)?)?
        .checked_mul(u64::from(l.hooks))?
        .checked_add(crate::hooks::storage(l.hooks)?)?
        .checked_add(List::<(u16, skein_lib::Secret)>::worst_case(l.forges)?)?
        .checked_add(u64::from(l.hook_secret_bytes).checked_mul(u64::from(forges))?)
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
