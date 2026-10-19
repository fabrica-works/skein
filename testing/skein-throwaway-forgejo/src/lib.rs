//! The checked binary, isolated instance and plain HTTP client (fake-forge.md, 6).
mod binary;
mod cli;
pub use cli::{Output, WorkPath};
mod http;
mod instance;
pub use binary::{Forgejo, Refusal};
pub use instance::Instance;
use std::{fmt, path::PathBuf};
#[derive(Debug, Clone)]
pub struct Settings {
    pub max_response_items: Option<u32>,
    pub webhook_hosts: Option<String>,
    pub actions: bool,
    pub git: PathBuf,
    pub ready_polls: u32,
    pub answer_bytes: usize,
}
impl Settings {
    #[must_use]
    pub fn small() -> Self {
        let git = "/usr/bin/git".into();
        Self {
            webhook_hosts: None,
            max_response_items: Some(2),
            actions: true,
            git,
            ready_polls: 200,
            answer_bytes: 16 * 1024 * 1024,
        }
    }
}
#[derive(Clone)]
pub struct Account {
    pub login: String,
    pub id: u64,
    pub token: Token,
    pub(crate) password: String,
}
impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Account").field(&self.login).field(&self.id).field(&self.token).finish()
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);
impl Token {
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }
    #[must_use]
    pub fn value(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Token").field("length", &self.0.len()).finish()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Init {
    Readme,
    Empty,
    Sha256,
}
#[derive(Debug, Clone)]
pub struct Made {
    pub id: u64,
    pub owner: String,
    pub name: String,
}
/// A request's byte range.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Range {
    /// The first `n + 1` bytes.
    Head(u64),
    /// The last `n` bytes.
    Tail(u64),
}
#[derive(Clone)]
pub struct Outgoing<'a> {
    pub method: &'a str,
    pub target: &'a str,
    pub token: Option<&'a Token>,
    pub body: Option<&'a [u8]>,
    pub range: Option<Range>,
}
impl fmt::Debug for Outgoing<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Outgoing").field(&self.method).field(&self.target).field(&self.token).finish()
    }
}
impl<'a> Outgoing<'a> {
    #[must_use]
    pub fn get(target: &'a str, token: Option<&'a Token>) -> Self {
        Self { method: "GET", target, token, body: None, range: None }
    }
    #[must_use]
    pub fn json(method: &'a str, target: &'a str, token: Option<&'a Token>, body: &'a [u8]) -> Self {
        Self { method, target, token, body: Some(body), range: None }
    }
}
#[derive(Debug, Clone)]
pub struct Exchange {
    pub sent: Vec<u8>,
    pub status: u16,
    pub head: Vec<u8>,
    pub body: Vec<u8>,
}
#[derive(Debug)]
pub enum Failure {
    Scratch { path: PathBuf, why: String },
    Spawn { step: &'static str, error: String },
    Exited { step: &'static str, status: String, log_tail: String },
    NotReady { polls: u32, log_tail: String },
    Io(String),
    Status { target: String, status: u16, body: String },
}
impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
#[cfg(test)]
mod tests;

/// Draws 64 random hexadecimal digits.
/// # Errors
/// Returns a failure to read the system random source.
pub fn fresh_secret() -> Result<String, Failure> {
    cli::random()
}

pub use skein_forge::forgejo::pin::{RUNNER_VERSION, names_runner};
