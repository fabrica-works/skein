//! Pure, owner-driven forge reconciliation (forge-setup.md, 4 and 6).
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
extern crate alloc;
pub mod given;
pub mod item;
pub mod limits;
pub mod reconcile;
#[cfg(test)]
mod tests;
