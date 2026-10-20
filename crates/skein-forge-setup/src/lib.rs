//! Pure, owner-driven forge reconciliation (forge-setup.md, 4 and 6).
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
extern crate alloc;
mod access;
pub mod adopt;
pub mod check;
pub mod github;
pub mod given;
pub mod host;
pub mod item;
pub mod limits;
pub mod reachable;
pub mod reconcile;
pub mod take_over;
#[cfg(test)]
mod tests;
