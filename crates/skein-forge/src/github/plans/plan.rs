//! One call's exchanges and its single terminal answer (forge.md, 3.2).
use super::{Heard, reads, tables::Tables};
use crate::{api, github::request};
use alloc::boxed::Box;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Exchange {
    pub request: request::Request,
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Step {
    Send(Exchange),
    Done(Result<api::Answer, api::Error>),
}
impl Step {
    #[must_use]
    pub fn send(request: request::Request) -> Self {
        Self::Send(Exchange { request })
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
    Refused,
}
impl Plan {
    #[must_use]
    pub fn new(repository: api::Repository, op: api::Op) -> Self {
        match op {
            api::Op::Read(read) => Self::Read(Box::new(reads::Plan::new(repository, read))),
            api::Op::Write(_) => Self::Refused,
        }
    }
    pub fn begin(&mut self, bounds: Bounds, word: &[u8], tables: &mut Tables) -> Result<Step, api::Error> {
        match self {
            Self::Read(plan) => plan.begin(bounds, word, tables),
            // Later tasks plan every write.
            Self::Refused => Err(api::Error::Refused),
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
            Self::Refused => Err(api::Error::InvalidAnswer),
        }
    }
    /// Takes the exchange the caller retained with `set_pending`, as in Forgejo.
    pub fn pending(&mut self) -> Option<Exchange> {
        match self {
            Self::Read(plan) => plan.pending(),
            Self::Refused => None,
        }
    }
    /// The caller retains a sent exchange here if it may need to resend it.
    pub fn set_pending(&mut self, exchange: Exchange) {
        match self {
            Self::Read(plan) => plan.set_pending(exchange),
            Self::Refused => {}
        }
    }
}
