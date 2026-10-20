//! The fake person over the browser kit (ui-web.md, 14.2).
#![forbid(unsafe_code)]
mod face;
pub mod harness;
mod keys;
pub mod origins;
pub use face::BrowserFace;
pub use harness::{Harness, Kept, Lent, Outcome, Play, Script, Setup, Step};
#[cfg(test)]
mod tests;

pub mod stories;
