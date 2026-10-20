//! GitHub's HTTP over the fake forge (fake-forge.md, section 10):
//! App authentication, installations, reads and writes over the shared domain.
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
mod writes;

#[cfg(test)]
mod tests;
