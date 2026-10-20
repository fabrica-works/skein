//! Single-attempt writes and read prerequisites (forge-github.md, 4.2 and 7).
use super::{
    Body, Failure, Heard, failure as read_failure,
    plan::{Bounds, Exchange, Step},
    tables::{self, Repository, Tables},
};
use crate::{
    api,
    github::{
        document::Document,
        failure::{self, Asked, Drop, Verdict},
        header::Head,
        request::Request,
    },
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    New,
    Resolve,
    Mutate,
    Done,
}
#[derive(Debug)]
enum Response {
    None,
    Head,
    Body,
    Refusal { status: u16, head: Head },
}
#[derive(Debug)]
pub struct Plan {
    repository: api::Repository,
    write: api::Write,
    stage: Stage,
    response: Response,
    row: Option<Repository>,
    pending: Option<Exchange>,
}
impl Plan {
    #[must_use]
    pub fn new(repository: api::Repository, write: api::Write) -> Self {
        Self { repository, write, stage: Stage::New, response: Response::None, row: None, pending: None }
    }
    pub fn begin(&mut self, _bounds: Bounds, _word: &[u8], tables: &mut Tables) -> Result<Step, api::Error> {
        if self.stage != Stage::New {
            return Err(api::Error::InvalidAnswer);
        }
        match self.write {
            api::Write::DeleteBranch { .. }
            | api::Write::Close { .. }
            | api::Write::Reopen { .. }
            | api::Write::Edit { .. }
            | api::Write::Status { .. } => {}
            api::Write::CreateIssue { .. }
            | api::Write::OpenPull { .. }
            | api::Write::Post { .. }
            | api::Write::Review { .. }
            | api::Write::SetReviewers { .. }
            | api::Write::Merge { .. }
            | api::Write::Update { .. }
            | api::Write::CreateBranch { .. } => return Err(api::Error::Refused),
        }
        self.row = tables.repository(self.repository).cloned();
        if self.row.is_none() {
            self.stage = Stage::Resolve;
            return Ok(self.send(Request::Repository { id: u64::from(self.repository.repository) }));
        }
        self.mutation()
    }
    pub fn pending(&mut self) -> Option<Exchange> {
        self.pending.take()
    }
    pub fn set_pending(&mut self, exchange: Exchange) {
        self.pending = Some(exchange);
    }
    fn send(&mut self, request: Request) -> Step {
        self.response = Response::Head;
        Step::send(request)
    }
    fn mutation(&mut self) -> Result<Step, api::Error> {
        let row = self.row.as_ref().ok_or(api::Error::InvalidAnswer)?;
        let owner = row.owner.clone();
        let name = row.name.clone();
        let request = match &self.write {
            api::Write::Status { commit, context, check } => {
                Request::Status { owner, name, commit: *commit, context: context.clone(), check: *check }
            }
            api::Write::Edit { number, title, body } => {
                Request::Edit { owner, name, number: *number, title: title.clone(), body: body.clone() }
            }
            api::Write::Reopen { number } => Request::Reopen { owner, name, number: *number },
            api::Write::Close { number } => Request::Close { owner, name, number: *number },
            api::Write::DeleteBranch { branch } => Request::DeleteBranch { owner, name, branch: branch.clone() },
            api::Write::CreateIssue { .. }
            | api::Write::OpenPull { .. }
            | api::Write::Post { .. }
            | api::Write::Review { .. }
            | api::Write::SetReviewers { .. }
            | api::Write::Merge { .. }
            | api::Write::Update { .. }
            | api::Write::CreateBranch { .. } => return Err(api::Error::Refused),
        };
        self.stage = Stage::Mutate;
        Ok(self.send(request))
    }
    fn status(&self) -> u16 {
        match self.write {
            api::Write::DeleteBranch { .. } => 204,
            api::Write::Status { .. } => 201,
            api::Write::CreateIssue { .. }
            | api::Write::OpenPull { .. }
            | api::Write::Post { .. }
            | api::Write::Review { .. }
            | api::Write::Edit { .. }
            | api::Write::SetReviewers { .. }
            | api::Write::Close { .. }
            | api::Write::Reopen { .. }
            | api::Write::Merge { .. }
            | api::Write::Update { .. }
            | api::Write::CreateBranch { .. } => 200,
        }
    }
    fn asked(&self) -> Asked {
        match &self.write {
            api::Write::DeleteBranch { .. } => Asked::DeleteBranch,
            api::Write::Edit { title, body, .. } => {
                Asked::Write { empty: empty(title.as_deref()) || empty(body.as_deref()) }
            }
            api::Write::CreateIssue { .. }
            | api::Write::OpenPull { .. }
            | api::Write::Post { .. }
            | api::Write::Review { .. }
            | api::Write::SetReviewers { .. }
            | api::Write::Close { .. }
            | api::Write::Reopen { .. }
            | api::Write::Merge { .. }
            | api::Write::Update { .. }
            | api::Write::Status { .. }
            | api::Write::CreateBranch { .. } => Asked::Write { empty: false },
        }
    }
    pub fn heard(
        &mut self,
        heard: Heard,
        bounds: Bounds,
        _word: &[u8],
        tables: &mut Tables,
    ) -> Result<Option<Step>, api::Error> {
        if self.stage == Stage::New || self.stage == Stage::Done {
            return Err(api::Error::InvalidAnswer);
        }
        match (&heard, &self.response) {
            (Heard::Head { .. }, Response::Head)
            | (Heard::Done(_) | Heard::Row(_), Response::Body | Response::Refusal { .. })
            | (Heard::Failed(_), Response::Head | Response::Body | Response::Refusal { .. }) => {}
            (Heard::Head { .. }, Response::None | Response::Body | Response::Refusal { .. })
            | (Heard::Done(_) | Heard::Row(_), Response::None | Response::Head)
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
    fn malformed(&self, error: crate::github::decode::Malformed) -> api::Error {
        if self.stage == Stage::Mutate {
            return api::Error::Timeout;
        }
        if read_failure::malformed(error) == api::Error::TooLarge {
            api::Error::TooLarge
        } else {
            api::Error::Unavailable
        }
    }

    fn invalid(&self) -> api::Error {
        if self.stage == Stage::Mutate { api::Error::Timeout } else { api::Error::Unavailable }
    }
    fn terminal(&mut self, answer: api::Answer, bounds: Bounds) -> Result<Option<Step>, api::Error> {
        let size = crate::measure::answer(&answer).ok_or(self.invalid())?;
        if size > u64::from(bounds.answer_bytes) {
            return Err(self.invalid());
        }
        self.stage = Stage::Done;
        self.response = Response::None;
        Ok(Some(Step::Done(Ok(answer))))
    }
    fn refusal(
        &self,
        status: u16,
        head: &Head,
        body: Option<&crate::github::document::Refusal>,
        tables: &mut Tables,
    ) -> Result<api::Answer, api::Error> {
        let asked = if self.stage == Stage::Mutate { self.asked() } else { Asked::Read };
        match failure::classify(asked, status, head, body) {
            Verdict::Fail(error, drop) => {
                match drop {
                    Drop::Nothing => {}
                    Drop::Row => tables.drop_repository(self.repository),
                }
                Err(if self.stage != Stage::Mutate && error == api::Error::Timeout {
                    api::Error::Unavailable
                } else {
                    error
                })
            }
            Verdict::BranchExists | Verdict::Enqueue => Err(self.invalid()),
        }
    }
    fn refusal_head(&self) -> Option<(u16, &Head)> {
        match &self.response {
            Response::Refusal { status, head } => Some((*status, head)),
            Response::None | Response::Head | Response::Body => None,
        }
    }
    fn advance(&mut self, heard: Heard, bounds: Bounds, tables: &mut Tables) -> Result<Option<Step>, api::Error> {
        match heard {
            Heard::Head { status, head } => {
                let expected = if self.stage == Stage::Mutate { self.status() } else { 200 };
                if status == expected {
                    self.response = Response::Body;
                } else if (200..300).contains(&status) {
                    return Err(self.invalid());
                } else {
                    self.response = Response::Refusal { status, head };
                }
                Ok(None)
            }
            Heard::Row(_) => match self.response {
                Response::Refusal { .. } => Ok(None),
                Response::None | Response::Head | Response::Body => Err(api::Error::InvalidAnswer),
            },
            Heard::Failed(failed) => {
                if let Some((status, head)) = self.refusal_head() {
                    let answer = self.refusal(status, head, None, tables)?;
                    return self.terminal(answer, bounds);
                }
                Err(if self.stage == Stage::Mutate {
                    match failed {
                        Failure::Unseen => api::Error::Unavailable,
                        Failure::Seen => api::Error::Timeout,
                    }
                } else {
                    api::Error::Unavailable
                })
            }
            Heard::Done(body) => {
                if let Some((status, head)) = self.refusal_head() {
                    let refusal = match &body {
                        Body::Document(Document::Error(refusal)) => Some(refusal),
                        Body::Document(
                            Document::Installation(_)
                            | Document::Minted(_)
                            | Document::Viewer(_)
                            | Document::Graphql(_)
                            | Document::App(_)
                            | Document::HookConfig(_)
                            | Document::Conversion(_)
                            | Document::User(_)
                            | Document::Compare(_)
                            | Document::Permission(_)
                            | Document::Done
                            | Document::BotUser(_)
                            | Document::Repository(_)
                            | Document::Branch(_)
                            | Document::Issue(_)
                            | Document::Pull(_)
                            | Document::Job(_)
                            | Document::Combined(_)
                            | Document::Rows,
                        )
                        | Body::Raw { .. }
                        | Body::Empty
                        | Body::Malformed(_) => None,
                    };
                    let answer = self.refusal(status, head, refusal, tables)?;
                    return self.terminal(answer, bounds);
                }
                match body {
                    Body::Malformed(error) => return Err(self.malformed(error)),
                    Body::Document(Document::Repository(repository)) if self.stage == Stage::Resolve => {
                        let id = crate::github::id::repository(repository.id)?;
                        if id == 0 || id != self.repository.repository {
                            return Err(self.invalid());
                        }
                        let row = tables::repository(&repository);
                        tables.keep_repository(self.repository, row.clone());
                        tables.keep_user(self.repository.forge, repository.owner_id, repository.owner);
                        self.row = Some(row);
                        return Ok(Some(self.mutation()?));
                    }
                    Body::Empty if self.stage == Stage::Mutate && self.asked() == Asked::DeleteBranch => {}
                    Body::Document(Document::Done)
                        if self.stage == Stage::Mutate && self.asked() != Asked::DeleteBranch => {}
                    Body::Document(
                        Document::Installation(_)
                        | Document::Minted(_)
                        | Document::Viewer(_)
                        | Document::Graphql(_)
                        | Document::App(_)
                        | Document::HookConfig(_)
                        | Document::Conversion(_)
                            | Document::User(_)
                            | Document::Compare(_)
                            | Document::Permission(_)
                        | Document::Done
                        | Document::BotUser(_)
                        | Document::Repository(_)
                        | Document::Branch(_)
                        | Document::Issue(_)
                        | Document::Pull(_)
                        | Document::Job(_)
                        | Document::Combined(_)
                        | Document::Rows
                        | Document::Error(_),
                    )
                    | Body::Empty
                    | Body::Raw { .. } => return Err(self.invalid()),
                }
                self.terminal(api::Answer::Done, bounds)
            }
        }
    }
}

fn empty(text: Option<&[u8]>) -> bool {
    match text {
        Some(text) => text.is_empty(),
        None => false,
    }
}
