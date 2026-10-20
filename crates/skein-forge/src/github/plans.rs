//! Pure GitHub call plans and their exchange events (forge-github.md, section 2).
use crate::github::{decode, document, header};
use alloc::boxed::Box;

pub mod failure;
pub mod plan;
mod reading;
pub mod reads;
pub mod tables;

#[cfg(test)]
mod tests;

/// What a finished answer body was; listing rows arrive before its document.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Body {
    Document(document::Document),
    Raw { bytes: Box<[u8]>, more: bool },
    Empty,
    Malformed(decode::Malformed),
}

/// Whether a broken exchange could have reached the server (forge.md, 3.4).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Failure {
    /// No request byte was written: `Unavailable`.
    Unseen,
    /// The server may have seen the request: `Timeout`.
    Seen,
}

/// One exchange, in order: a head, streamed rows, an end; or a transport failure.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Heard {
    /// GitHub's head has no status field, so the status travels beside it.
    Head {
        status: u16,
        head: header::Head,
    },
    Row(document::Row),
    Done(Body),
    Failed(Failure),
}
