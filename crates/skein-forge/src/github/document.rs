//! Fields kept from GitHub's first documents (forge-github.md, section 6).
use crate::api;
use alloc::boxed::Box;
/// The document a decoder expects.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Repository,
    Branch,
    BranchRows,
    Error,
}
/// A repository's routing, owner and settings row.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
#[expect(clippy::struct_excessive_bools, reason = "GitHub reports the owner kind and three independent merge settings")]
pub struct Repository {
    pub id: u64,
    pub owner: Box<[u8]>,
    pub owner_id: u64,
    pub organization: bool,
    pub name: Box<[u8]>,
    pub default_branch: Box<[u8]>,
    pub html_url: Box<[u8]>,
    pub merge: bool,
    pub squash: bool,
    pub rebase: bool,
}
/// A named branch and its commit.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Branch {
    pub name: Box<[u8]>,
    pub commit: api::Commit,
}
/// A streamed listing row.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Row {
    Branch(Box<[u8]>),
}
/// GitHub's message and bounded error messages.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Refusal {
    pub message: Box<[u8]>,
    pub errors: Box<[Box<[u8]>]>,
}
/// A whole document, or the end of a streamed listing.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Document {
    Repository(Repository),
    Branch(Branch),
    Rows,
    Error(Refusal),
}
