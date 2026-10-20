//! GitHub's documents at its pin (forge-github.md).
pub mod credential;
pub mod decode;
pub mod document;
pub use document::Reachable;
pub mod failure;
pub mod guid;
pub mod header;
pub mod id;
pub mod jwt;
pub mod limits;
pub mod pin;
pub mod plans;
pub mod redirect;
pub mod request;
pub mod server;
pub mod setup;

#[cfg(test)]
pub(crate) mod tests;
