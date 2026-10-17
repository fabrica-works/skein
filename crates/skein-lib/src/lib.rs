//! The building blocks every step crate shares (programming-model.md, 10.2):
//! typed handles and the slabs that issue them, bounded queues, lists, maps,
//! sets and stacks, the per-layer deadline table, the stream vocabulary and
//! the intake that meets its demands, a held buffer serving a stream,
//! a reader and a writer for sized bytes
//! and the decimal digits of a count to write with them, monotonic and wall
//! time, randomness, the tokens that cross layer boundaries, and the
//! environment a step reads, and keyless state digests.
//!
//! Application code does not hand-roll data structures: what is missing goes
//! here, written once and tested hard.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod base64;
mod btree;
pub mod bytes;
mod crypto_rng;
mod deadlines;
mod decimal;
mod digest;
mod env;
mod id;
mod intake;
mod journal;
mod list;
mod map;
mod queue;
mod reader;
mod requests;
mod rng;
mod secret;
mod set;
pub mod sha256;
mod slab;
mod stack;
pub mod stream;
#[cfg(test)]
mod tests;
mod time;
mod token;
mod writer;

pub use crypto_rng::CryptoRng;
pub use deadlines::Deadlines;
pub use decimal::Decimal;
pub use digest::{StateHasher, state_digest};
pub use env::Env;
pub use id::Id;
pub use intake::Intake;
pub use journal::{Commit, Decision, Journal, JournalLimits, JournalRoom, Released};
pub use list::List;
pub use map::Map;
pub use queue::Queue;
pub use reader::Reader;
pub use requests::{
    RequestAnswered, RequestEnvelope, RequestKey, RequestLimits, RequestOut, RequestProgress, RequestRecord,
    RequestTable, RequestTake,
};
pub use rng::Rng;
pub use secret::Secret;
pub use set::Set;
pub use slab::Slab;
pub use stack::Stack;
pub use time::{Duration, Time, Wall};
pub use token::{ReplyTo, Token};
pub use writer::{Overflow, Writer};
