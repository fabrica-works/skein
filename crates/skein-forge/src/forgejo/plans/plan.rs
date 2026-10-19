//! One call's exchanges, and its single terminal answer (forge.md, 3.2).
use crate::api;
use crate::forgejo::request;
/// The bytes requested from a raw answer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Range {
    Head(u32),
    Tail(u32),
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Exchange {
    pub request: request::Request,
    pub range: Option<Range>,
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Step {
    Send(Exchange),
    Done(Result<api::Answer, api::Error>),
}
impl Step {
    #[must_use]
    pub fn send(request: request::Request) -> Self {
        Self::Send(Exchange { request, range: None })
    }
}
use crate::forgejo::plans::{Heard, tables::Tables};
use alloc::boxed::Box;
#[derive(Debug)]
pub enum Plan {
    Read(Box<crate::forgejo::plans::reads::Plan>),
    Write(Box<crate::forgejo::plans::writes::Plan>),
}
impl Plan {
    #[must_use]
    pub fn error(&self, error: api::Error) -> api::Error {
        match self {
            Self::Read(_) => error,
            Self::Write(plan) => plan.error(error),
        }
    }
    #[must_use]
    pub fn new(repository: api::Repository, op: api::Op) -> Self {
        match op {
            api::Op::Read(read) => Self::Read(Box::new(crate::forgejo::plans::reads::Plan::new(repository, read))),
            api::Op::Write(write) => Self::Write(Box::new(crate::forgejo::plans::writes::Plan::new(repository, write))),
        }
    }
    pub fn begin(&mut self, bounds: Bounds, word: &[u8], tables: &mut Tables) -> Result<Step, api::Error> {
        match self {
            Self::Read(plan) => plan.begin(bounds, word, tables),
            Self::Write(plan) => plan.begin(bounds, word, tables),
        }
    }
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
    pub fn pending(&mut self) -> Option<Exchange> {
        match self {
            Self::Read(plan) => plan.pending(),
            Self::Write(plan) => plan.pending(),
        }
    }
    pub fn set_pending(&mut self, request: Exchange) {
        match self {
            Self::Read(plan) => plan.set_pending(request),
            Self::Write(plan) => plan.set_pending(request),
        }
    }
    #[must_use]
    pub fn after(&self) -> u64 {
        match self {
            Self::Read(plan) => plan.after(),
            Self::Write(_) => 0,
        }
    }
    #[must_use]
    pub fn raw_room(&self, answer_bytes: u32) -> u32 {
        match self {
            Self::Read(plan) => plan.raw_room(answer_bytes),
            Self::Write(_) => answer_bytes,
        }
    }
    pub fn take_cut(&mut self) -> bool {
        match self {
            Self::Read(plan) => plan.take_cut(),
            Self::Write(_) => false,
        }
    }
}

/// The owner's checked values for this call.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Bounds {
    pub page_size: u32,
    pub rows: u32,
    pub answer_bytes: u32,
    pub body_bytes: u32,
}
