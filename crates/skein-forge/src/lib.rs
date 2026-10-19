//! Neutral forge vocabulary and dialects (forge.md).
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod api;
pub mod forgejo;
pub mod locate;
pub mod measure;

#[cfg(test)]
mod tests;

pub mod marker;
