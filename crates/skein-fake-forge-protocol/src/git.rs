//! The private, two-sided git vocabulary (fake-forge.md, sections 3–4).
use crate::Limits;
use alloc::boxed::Box;
use skein_fake_forge_domain::{self as domain, api};
use skein_lib::List;
pub mod client;
mod read;
pub mod server;
pub(crate) mod store;
pub(crate) mod write;

pub use skein_forge::forgejo::server::Unrouted;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Name {
    Clone,
    Fetch,
    Create,
    Push,
    Branch,
    Objects,
    Commit,
    Merge,
    CommitTree,
    Ancestor,
}
impl Name {
    #[must_use]
    pub const fn segment(self) -> &'static [u8] {
        match self {
            Self::Clone => b"clone",
            Self::Fetch => b"fetch",
            Self::Create => b"create",
            Self::Push => b"push",
            Self::Branch => b"branch",
            Self::Objects => b"objects",
            Self::Commit => b"commit",
            Self::Merge => b"merge",
            Self::CommitTree => b"commit-tree",
            Self::Ancestor => b"ancestor",
        }
    }
    #[must_use]
    pub const fn repository(self) -> bool {
        match self {
            Self::Clone | Self::Fetch | Self::Create | Self::Push | Self::Branch => true,
            Self::Objects | Self::Commit | Self::Merge | Self::CommitTree | Self::Ancestor => false,
        }
    }
    #[must_use]
    pub const fn writes(self) -> bool {
        match self {
            Self::Create | Self::Push => true,
            Self::Clone
            | Self::Fetch
            | Self::Branch
            | Self::Objects
            | Self::Commit
            | Self::Merge
            | Self::CommitTree
            | Self::Ancestor => false,
        }
    }
}
#[must_use]
pub fn name(target: &[u8]) -> Option<Result<Name, Unrouted>> {
    let segment = target.strip_prefix(b"/.fake/git/")?;
    Some(match segment {
        b"clone" => Ok(Name::Clone),
        b"fetch" => Ok(Name::Fetch),
        b"create" => Ok(Name::Create),
        b"push" => Ok(Name::Push),
        b"branch" => Ok(Name::Branch),
        b"objects" => Ok(Name::Objects),
        b"commit" => Ok(Name::Commit),
        b"merge" => Ok(Name::Merge),
        b"commit-tree" => Ok(Name::CommitTree),
        b"ancestor" => Ok(Name::Ancestor),
        _ => Err(Unrouted::NotFound),
    })
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Call {
    Clone { repository: Box<[u8]> },
    Fetch { repository: Box<[u8]>, want: api::Want },
    Create { repository: Box<[u8]>, branch: Box<[u8]>, commit: u64 },
    Push { repository: Box<[u8]>, branch: Box<[u8]>, commit: u64, expected: Option<u64> },
    Branch { repository: Box<[u8]>, branch: Box<[u8]> },
    Objects { commits: Box<[u64]> },
    Commit { parent: u64, tree: Box<[api::File]>, message: Box<[u8]> },
    Merge { parent: u64, merge_parent: u64, tree: Box<[api::File]>, message: Box<[u8]> },
    CommitTree { parent: u64, merging: Option<u64>, tree: Box<[api::File]>, message: Box<[u8]> },
    Ancestor { ancestor: u64, commit: u64 },
}
impl Call {
    #[must_use]
    pub const fn name(&self) -> Name {
        match self {
            Self::Clone { .. } => Name::Clone,
            Self::Fetch { .. } => Name::Fetch,
            Self::Create { .. } => Name::Create,
            Self::Push { .. } => Name::Push,
            Self::Branch { .. } => Name::Branch,
            Self::Objects { .. } => Name::Objects,
            Self::Commit { .. } => Name::Commit,
            Self::Merge { .. } => Name::Merge,
            Self::CommitTree { .. } => Name::CommitTree,
            Self::Ancestor { .. } => Name::Ancestor,
        }
    }
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Object {
    pub commit: u64,
    pub parent: Option<u64>,
    pub merge_parent: Option<u64>,
    pub tree: Box<[api::File]>,
    pub message: Box<[u8]>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Parents {
    pub commit: u64,
    pub parent: Option<u64>,
    pub merge_parent: Option<u64>,
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Answer {
    Cloned { default: Box<[u8]>, branches: Box<[api::Head]> },
    Commit(u64),
    Created(api::Created),
    Pushed(api::Pushed),
    Stored(Option<u64>),
    Objects { objects: Box<[Object]>, graph: Box<[Parents]> },
    Ancestor(bool),
}

/// Both decoders retain compact token records and decoded text through Done.
/// Each count is capped by the body bound: even a one-byte token costs a byte
/// on the wire. No tree or object list exceeds `documents.inner`.
#[derive(Debug)]
pub(crate) struct Decoder {
    records: List<skein_json::Compact>,
    text: List<u8>,
    over: bool,
}
impl Decoder {
    fn new(limits: &Limits) -> Self {
        let cap = limits.documents.body_bytes.min(limits.documents.json.length);
        Self { records: List::with_capacity(cap), text: List::with_capacity(cap), over: false }
    }
}
/// Decoder buffers, the final compact document, decoded lists and payload,
/// transient base64 text, and one incoming tokenizer token. Both sides use
/// this same bound; the server adds it once per connection.
#[must_use]
pub fn decoder_worst_case(limits: &Limits) -> Option<u64> {
    let cap = limits.documents.body_bytes.min(limits.documents.json.length);
    let records = List::<skein_json::Compact>::worst_case(cap)?;
    let lists = List::<Object>::worst_case(limits.documents.inner)?
        .checked_add(List::<Parents>::worst_case(limits.documents.inner)?)?
        .checked_add(List::<api::Head>::worst_case(limits.documents.inner)?)?
        .checked_add(List::<api::File>::worst_case(cap)?)?;
    records
        .checked_mul(2)?
        .checked_add(u64::from(cap).checked_mul(6)?)?
        .checked_add(lists)?
        .checked_add(u64::from(limits.documents.json.string.max(limits.documents.json.number)))
}

/// Computed wire and token sizes for a maximal commit-tree and one whole
/// object. Base64 needs no JSON escapes. Commit ids reserve all 20 digits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TreeBound {
    pub request: u32,
    pub answer: u32,
    pub tokens: u32,
    pub string: u32,
}
#[must_use]
pub fn tree_bound(domain: &domain::Limits) -> Option<TreeBound> {
    use skein_lib::base64::{Alphabet::Standard, encoded_len};
    let message = domain.title_bytes.checked_add(domain.body_bytes)?.checked_add(2)?;
    let path = encoded_len(Standard, domain.name_bytes)?;
    let content = encoded_len(Standard, domain.content_bytes)?;
    let message = encoded_len(Standard, message)?;
    // {"path":"…","content":"…"}, comma included, minus the last comma.
    let entry = 25_u32.checked_add(path)?.checked_add(content)?;
    let tree = 2_u32.checked_add(domain.files.checked_mul(entry)?)?.checked_sub(u32::from(domain.files > 0))?;
    // Envelopes include quotes, separators, both parents, and an empty graph.
    let request = 83_u32.checked_add(tree)?.checked_add(message)?;
    let answer = 143_u32.checked_add(tree)?.checked_add(message)?;
    let tokens = 21_u32.checked_add(domain.files.checked_mul(6)?)?;
    Some(TreeBound { request, answer, tokens, string: path.max(content).max(message).max(12) })
}
#[must_use]
pub fn fits(protocol: &Limits, domain: &domain::Limits) -> bool {
    let Some(bound) = tree_bound(domain) else { return false };
    let documents = &protocol.documents;
    let room = documents.body_bytes.min(documents.json.length).min(documents.writer.length);
    u64::from(bound.request) <= protocol.http.body
        && bound.request <= room
        && bound.answer <= room
        && bound.string <= documents.json.string
        && bound.string <= documents.body_bytes
        && domain.name_bytes <= documents.string_bytes
        && domain.files <= documents.inner
        && bound.tokens <= documents.body_bytes.min(documents.json.length)
        && documents.json.number >= 20
        && documents.json.depth >= 5
        && documents.writer.depth >= 5
}
