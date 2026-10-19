//! Forge protocol component (forge-connection.md, sections 2, 3, 4, 7 and 8).
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
extern crate alloc;
pub mod boundary;
pub mod component;
mod connection;
pub mod grow;
pub mod hooks;
pub mod limits;
pub mod settings;
pub mod start;
pub use boundary::{Answered, Event, Request, StartRead, Stopped};
pub use component::{Component, Diagnostics, MAX_OUT};
pub use limits::{Limits, Shared, Unusable};
pub use settings::Settings;
#[cfg(test)]
mod tests;
