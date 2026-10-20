//! The reads whose GitHub requests and documents are complete (forge-github.md, 4.1).
use super::{
    Body, Heard, failure,
    plan::{Bounds, Exchange, Step},
    tables::{self, Repository, Tables},
};
use crate::{
    api,
    github::{document, header::Head, request::Request},
};
use alloc::boxed::Box;
use skein_lib::List;

mod done;
mod size;

/// Only these reads have stages; all others are refused at admission.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Read {
    Locate { owner: Box<[u8]>, name: Box<[u8]> },
    Settings,
    Branch { branch: Box<[u8]> },
    Branches,
    Refused,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Stage {
    New,
    Resolve,
    Read,
    Done,
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Response {
    None,
    Head,
    Body { next: bool },
    Refusal { status: u16, head: Head },
}
#[derive(Debug)]
pub struct Plan {
    repository: api::Repository,
    read: Read,
    stage: Stage,
    response: Response,
    row: Option<Repository>,
    request: Option<Exchange>,
    rows: List<Box<[u8]>>,
    size: u64,
    count: u32,
    page: u32,
    limit: u32,
}
impl Plan {
    #[must_use]
    pub fn new(repository: api::Repository, read: api::Read) -> Self {
        let read = match read {
            api::Read::Locate { owner, name } => Read::Locate { owner, name },
            api::Read::Settings => Read::Settings,
            api::Read::Branch { branch } => Read::Branch { branch },
            api::Read::Branches => Read::Branches,
            api::Read::Items { .. }
            | api::Read::Item { .. }
            | api::Read::Pull { .. }
            | api::Read::PullFor { .. }
            | api::Read::Reviews { .. }
            | api::Read::Statuses { .. }
            | api::Read::Remarks { .. }
            | api::Read::PullFiles { .. }
            | api::Read::Compare { .. }
            | api::Read::Checks { .. }
            | api::Read::Job { .. }
            | api::Read::File { .. }
            | api::Read::Protection { .. }
            | api::Read::Collaborators { .. }
            | api::Read::Permission { .. } => Read::Refused,
        };
        Self {
            repository,
            read,
            stage: Stage::New,
            response: Response::None,
            row: None,
            request: None,
            rows: List::with_capacity(0),
            size: 0,
            count: 0,
            page: 1,
            limit: 0,
        }
    }
    pub fn begin(&mut self, bounds: Bounds, tables: &mut Tables) -> Result<Step, api::Error> {
        if self.stage != Stage::New {
            return Err(api::Error::InvalidAnswer);
        }
        match &self.read {
            Read::Locate { owner, name } => {
                self.stage = Stage::Read;
                return Ok(self.send(Request::Locate { owner: owner.clone(), name: name.clone() }));
            }
            Read::Settings => {
                self.stage = Stage::Read;
                return Ok(self.send(Request::Repository { id: u64::from(self.repository.repository) }));
            }
            Read::Branches => {
                let capacity = size::capacity(bounds)?;
                self.limit = size::page_size(bounds, capacity)?;
                self.rows = List::with_capacity(capacity);
            }
            Read::Branch { .. } => {}
            // Later tasks plan every other read.
            Read::Refused => return Err(api::Error::Refused),
        }
        self.row = tables.repository(self.repository).cloned();
        if self.row.is_none() {
            self.stage = Stage::Resolve;
            return Ok(self.send(Request::Repository { id: u64::from(self.repository.repository) }));
        }
        self.initial()
    }
    pub fn pending(&mut self) -> Option<Exchange> {
        self.request.take()
    }
    pub fn set_pending(&mut self, exchange: Exchange) {
        self.request = Some(exchange);
    }
    fn send(&mut self, request: Request) -> Step {
        self.response = Response::Head;
        self.count = 0;
        Step::send(request)
    }
    fn initial(&mut self) -> Result<Step, api::Error> {
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let owner = row.owner.clone();
        let name = row.name.clone();
        let request = match &self.read {
            Read::Branch { branch } => Request::Branch { owner, name, branch: branch.clone() },
            Read::Branches => Request::Branches { owner, name, page: self.page, per_page: self.limit },
            Read::Locate { .. } | Read::Settings | Read::Refused => return Err(api::Error::InvalidAnswer),
        };
        self.stage = Stage::Read;
        Ok(self.send(request))
    }
    /// Admission and call-order errors use `Err`; exchange verdicts finish the plan.
    pub fn heard(&mut self, heard: Heard, bounds: Bounds, tables: &mut Tables) -> Result<Option<Step>, api::Error> {
        if self.stage == Stage::New || self.stage == Stage::Done {
            return Err(api::Error::InvalidAnswer);
        }
        // A second head or a body before the head is a caller ordering error.
        match (&heard, &self.response) {
            (Heard::Head { .. }, Response::Head)
            | (Heard::Row(_) | Heard::Done(_), Response::Body { .. } | Response::Refusal { .. })
            | (Heard::Failed(_), Response::Head | Response::Body { .. } | Response::Refusal { .. }) => {}
            (Heard::Head { .. }, Response::None | Response::Body { .. } | Response::Refusal { .. })
            | (Heard::Row(_) | Heard::Done(_), Response::None | Response::Head)
            | (Heard::Failed(_), Response::None) => return Err(api::Error::InvalidAnswer),
        }
        match self.advance(heard, bounds, tables) {
            Ok(step) => Ok(step),
            Err(error) => {
                self.stage = Stage::Done;
                self.response = Response::None;
                Ok(Some(Step::Done(Err(error))))
            }
        }
    }
    fn advance(&mut self, heard: Heard, bounds: Bounds, tables: &mut Tables) -> Result<Option<Step>, api::Error> {
        match heard {
            Heard::Head { status, head } => {
                if status == 200 {
                    self.response = Response::Body { next: head.next };
                    return Ok(None);
                }
                // Both limit statuses wait for Done so secondary-limit text can be
                // classified. If transport fails meanwhile, the heard status wins.
                if status == 403 || status == 429 {
                    self.response = Response::Refusal { status, head };
                    return Ok(None);
                }
                Err(failure::read(status, &head, None, self.repository, tables))
            }
            Heard::Row(row) => {
                match self.response {
                    Response::Refusal { .. } => return Ok(None),
                    Response::Body { .. } => {}
                    Response::None | Response::Head => return Err(api::Error::InvalidAnswer),
                }
                self.keep_row(row, bounds)?;
                Ok(None)
            }
            Heard::Done(body) => {
                match &self.response {
                    Response::Refusal { status, head } => {
                        let refusal = match &body {
                            Body::Document(document::Document::Error(refusal)) => Some(refusal),
                            Body::Document(
                                document::Document::Repository(_)
                                | document::Document::Branch(_)
                                | document::Document::Rows
                                | document::Document::Installation(_)
                                | document::Document::Minted(_)
                                | document::Document::Viewer(_)
                                | document::Document::Graphql(_)
                                | document::Document::App(_)
                                | document::Document::HookConfig(_)
                                | document::Document::Conversion(_)
                                | document::Document::BotUser(_),
                            )
                            | Body::Raw { .. }
                            | Body::Empty
                            | Body::Malformed(_) => None,
                        };
                        return Err(failure::read(*status, head, refusal, self.repository, tables));
                    }
                    Response::Body { .. } => {}
                    Response::None | Response::Head => return Err(api::Error::InvalidAnswer),
                }
                match body {
                    Body::Document(document) => self.done(document, bounds, tables),
                    Body::Malformed(malformed) => Err(failure::malformed(malformed)),
                    Body::Empty | Body::Raw { .. } => Err(api::Error::InvalidAnswer),
                }
            }
            Heard::Failed(failed) => match &self.response {
                Response::Refusal { status, head } => Err(failure::read(*status, head, None, self.repository, tables)),
                Response::None | Response::Head | Response::Body { .. } => Err(failure::transport(failed)),
            },
        }
    }
    fn keep_row(&mut self, row: document::Row, bounds: Bounds) -> Result<(), api::Error> {
        if self.stage != Stage::Read || self.read != Read::Branches {
            return Err(api::Error::InvalidAnswer);
        }
        self.count = self.count.checked_add(1).ok_or(api::Error::TooLarge)?;
        if self.count > self.limit || self.rows.len() >= bounds.rows {
            return Err(api::Error::TooLarge);
        }
        match row {
            document::Row::Branch(name) => {
                self.size = size::add(self.size, &name, bounds)?;
                match self.rows.push(name) {
                    Ok(()) => Ok(()),
                    Err(_) => Err(api::Error::TooLarge),
                }
            }
            document::Row::Installation(_) | document::Row::Reachable(_) => Err(api::Error::InvalidAnswer),
        }
    }
}
