//! GitHub's HTTP over the fake forge (fake-forge.md, section 10):
//! its App's keys, token check, installations and their tokens.
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;
mod answers;
pub mod app;
pub mod installations;
pub mod keys;
pub mod manifest;
pub mod pages;
mod reads;
mod routes;
pub mod service;

#[cfg(test)]
mod tests;
