//! One call's exchanges and its single terminal answer (forge.md, 3.2).
use super::{Heard, reads, tables::Tables, writes};
use crate::{api, github::request};
use alloc::boxed::Box;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Exchange {
    pub request: request::Request,
    /// Raw retention to apply while reading through the exchange body.
    pub retention: Option<Retention>,
    /// Selects comparison fields without changing request kind dispatch.
    pub comparison: Comparison,
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Step {
    Send(Exchange),
    Done(Result<api::Answer, api::Error>),
}
impl Step {
    #[must_use]
    pub fn send(request: request::Request) -> Self {
        Self::Send(Exchange { request, retention: None, comparison: Comparison::Full })
    }
}

/// The owner's checked values for this call; body bytes are bounded by its decoder.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Bounds {
    pub page_size: u32,
    pub rows: u32,
    pub answer_bytes: u32,
    pub body_bytes: u32,
}
#[derive(Debug)]
pub enum Plan {
    Read(Box<reads::Plan>),
    Write(Box<writes::Plan>),
}
impl Plan {
    #[must_use]
    pub fn new(repository: api::Repository, op: api::Op) -> Self {
        match op {
            api::Op::Read(read) => Self::Read(Box::new(reads::Plan::new(repository, read))),
            api::Op::Write(write) => Self::Write(Box::new(writes::Plan::new(repository, write))),
        }
    }
    pub fn begin(&mut self, bounds: Bounds, word: &[u8], tables: &mut Tables) -> Result<Step, api::Error> {
        match self {
            Self::Read(plan) => plan.begin(bounds, word, tables),
            Self::Write(plan) => plan.begin(bounds, word, tables),
        }
    }
    /// `Err` rejects an out-of-order call; a plan's verdict is `Step::Done`.
    pub fn heard(
        &mut self,
        heard: Heard,
        bounds: Bounds,
        word: &[u8],
        tables: &mut Tables,
    ) -> Result<Option<Step>, api::Error> {
        match self {
            Self::Read(plan) => plan.heard(heard, bounds, word, tables),
            Self::Write(plan) => plan.heard(heard, bounds, word, tables),
        }
    }
    /// Takes the exchange the caller retained with `set_pending`, as in Forgejo.
    pub fn pending(&mut self) -> Option<Exchange> {
        match self {
            Self::Read(plan) => plan.pending(),
            Self::Write(plan) => plan.pending(),
        }
    }
    /// The caller retains a sent exchange here if it may need to resend it.
    pub fn set_pending(&mut self, exchange: Exchange) {
        match self {
            Self::Read(plan) => plan.set_pending(exchange),
            Self::Write(plan) => plan.set_pending(exchange),
        }
    }
}

/// Raw bodies are read through within the component's input bound.
/// Head and tail retain at most the cap; Whole reports overflow through `Body::Raw.more`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Retention {
    Head { bytes: u32 },
    Tail { bytes: u32 },
    Whole { bytes: u32 },
}

/// A pull's merge-base lookup skips comparison completeness fields and lists.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Comparison {
    Full,
    MergeBase,
}
impl Exchange {
    /// Selects the successful response decoder; error responses use `Kind::Error`.
    #[must_use]
    pub fn decoder(&self, limits: &crate::github::limits::Limits) -> Option<crate::github::decode::Decoder> {
        let kind = crate::github::decode::kind(&self.request)?;
        if kind == crate::github::document::Kind::Compare && self.comparison == Comparison::MergeBase {
            Some(crate::github::decode::merge_base_decoder(limits))
        } else {
            Some(crate::github::decode::decoder(kind, limits))
        }
    }
}
