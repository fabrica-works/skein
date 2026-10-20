//! The fake person over the browser kit (ui-web.md, 14.2).
#![forbid(unsafe_code)]
mod face;
pub mod harness;
mod keys;
pub mod origins;
pub use face::BrowserFace;
pub use harness::{Command, Harness, Kept, Lent, Opens, Outcome, Play, Script, Sent, Setup, Stdin, Step};
#[cfg(test)]
mod tests;

pub mod stories;
