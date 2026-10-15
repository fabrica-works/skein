//! The notes example view (examples.md, 9.2).
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
extern crate alloc;

#[rustfmt::skip]
#[path = "generated/v1.rs"]
pub mod records;
pub mod limits;
pub mod places;
mod state;
mod view;
pub use places::Place;
pub use state::{Duties, Exchange, Record, Report, State, StreamDuty};

#[cfg(test)]
mod tests;
