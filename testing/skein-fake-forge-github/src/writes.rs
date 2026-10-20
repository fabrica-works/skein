//! GitHub mutations decoded by the dialect and applied to the shared domain.
use crate::{
    answers::{self, Body},
    installations::Grant,
    reads,
    service::{Limits, Outcome, Response, Service},
};
use alloc::boxed::Box;
use skein_fake_forge_domain::{
    self as fake,
    api::{self as a, Op, Write},
};
use skein_forge::github::{request::Request, server};
use skein_lib::ReplyTo;

#[derive(Debug)]
pub(crate) enum Route {
    Reopen { owner: Box<[u8]>, name: Box<[u8]>, number: u64 },
    Close { owner: Box<[u8]>, name: Box<[u8]>, number: u64 },
    Edit { owner: Box<[u8]>, name: Box<[u8]>, number: u64, title: Option<Box<[u8]>>, body: Option<Box<[u8]>> },
    Post { owner: Box<[u8]>, name: Box<[u8]>, number: u64, body: Box<[u8]> },
    CreateIssue { owner: Box<[u8]>, name: Box<[u8]>, title: Box<[u8]>, body: Box<[u8]> },
}
impl Route {
    fn request(self) -> Request {
        match self {
            Self::Reopen { owner, name, number } => Request::Reopen { owner, name, number },
            Self::Close { owner, name, number } => Request::Close { owner, name, number },
            Self::Edit { owner, name, number, title, body } => Request::Edit { owner, name, number, title, body },
            Self::Post { owner, name, number, body } => Request::Post { owner, name, number, body },
            Self::CreateIssue { owner, name, title, body } => Request::CreateIssue { owner, name, title, body },
        }
    }
    fn path(&self) -> (&[u8], &[u8]) {
        match self {
            Self::Reopen { owner, name, .. }
            | Self::Close { owner, name, .. }
            | Self::Edit { owner, name, .. }
            | Self::Post { owner, name, .. }
            | Self::CreateIssue { owner, name, .. } => (owner, name),
        }
    }
}
#[derive(Debug)]
pub(crate) enum Pending {
    Done,
    Created,
    Commented,
}
pub(crate) fn call(service: &Service, route: Route, grant: &Grant, body: &[u8], reply_to: ReplyTo) -> Outcome {
    let (owner, name) = route.path();
    let mut found = None;
    for (_, row) in &service.repositories {
        if row.owner.as_ref() == owner && row.name.as_ref() == name {
            found = Some(row);
        }
    }
    let Some(row) = found else {
        return Outcome::Reply(answers::missing(&service.limits));
    };
    if !service.installations.reaches(grant, row.id) {
        return Outcome::Reply(answers::missing(&service.limits));
    }
    if !service.body_fits(body) {
        return Outcome::Reply(answers::refusal(413, b"Payload Too Large", &service.limits));
    }
    let Some(request) = server::write_body(route.request(), body, &service.limits.github) else {
        return Outcome::Reply(answers::refusal(422, b"Validation Failed", &service.limits));
    };
    let Some((kind, write)) = issue(request) else {
        return Outcome::Reply(answers::failed());
    };
    let Some(repository) = reads::full(row) else {
        return Outcome::Reply(answers::failed());
    };
    Outcome::Call(
        crate::service::Pending { kind: crate::service::PendingKind::Write(kind) },
        fake::Event::Call { reply_to, user: service.bot, repository, op: Op::Write(write) },
    )
}
fn issue(request: Request) -> Option<(Pending, Write)> {
    Some(match request {
        Request::Reopen { number, .. } => (Pending::Done, Write::Reopen { number }),
        Request::Close { number, .. } => (Pending::Done, Write::Close { number }),
        Request::Edit { number, title, body, .. } => (Pending::Done, Write::EditItem { number, title, body }),
        Request::Post { number, body, .. } => (Pending::Commented, Write::Comment { number, body }),
        Request::CreateIssue { title, body, .. } => {
            (Pending::Created, Write::CreateIssue { title, body, labels: Box::from([]) })
        }
        Request::Installations { .. }
        | Request::Installation { .. }
        | Request::Mint { .. }
        | Request::Reachable { .. }
        | Request::Viewer
        | Request::Merge { .. }
        | Request::Enqueue { .. }
        | Request::PullGraphql { .. }
        | Request::App
        | Request::HookConfig
        | Request::SetHook { .. }
        | Request::Uninstall { .. }
        | Request::Conversion { .. }
        | Request::BotUser { .. }
        | Request::Repository { .. }
        | Request::Locate { .. }
        | Request::Issues { .. }
        | Request::Issue { .. }
        | Request::Comments { .. }
        | Request::Pull { .. }
        | Request::PullsFor { .. }
        | Request::Reviews { .. }
        | Request::ReviewComments { .. }
        | Request::CombinedStatus { .. }
        | Request::CheckRuns { .. }
        | Request::Job { .. }
        | Request::JobLog { .. }
        | Request::Branch { .. }
        | Request::Branches { .. }
        | Request::PullFiles { .. }
        | Request::Compare { .. }
        | Request::Raw { .. }
        | Request::RulesForBranch { .. }
        | Request::Ruleset { .. }
        | Request::ClassicProtection { .. }
        | Request::Collaborators { .. }
        | Request::Permission { .. }
        | Request::User { .. }
        | Request::Status { .. }
        | Request::CreateBranch { .. }
        | Request::Reviewers { .. }
        | Request::Update { .. }
        | Request::Review { .. }
        | Request::OpenPull { .. }
        | Request::DeleteBranch { .. } => return None,
    })
}
pub(crate) fn finish(pending: Pending, result: Result<a::Answer, a::Error>, limits: &Limits) -> Response {
    let answer = match result {
        Ok(answer) => answer,
        Err(error) => return refused(error, limits),
    };
    match pending {
        Pending::Done => {
            let a::Answer::Done = answer else {
                return answers::failed();
            };
            answers::response(200, &Body::Done, limits)
        }
        Pending::Created => {
            let a::Answer::Created(number) = answer else {
                return answers::failed();
            };
            answers::response(201, &Body::Number(number), limits)
        }
        Pending::Commented => {
            let a::Answer::Commented(id) = answer else {
                return answers::failed();
            };
            answers::response(201, &Body::Id(id), limits)
        }
    }
}
fn refused(error: a::Error, limits: &Limits) -> Response {
    match error {
        a::Error::Missing(_) | a::Error::Forbidden => answers::missing(limits),
        a::Error::Unavailable => answers::refusal(503, b"Server Error", limits),
        a::Error::Timeout => answers::refusal(504, b"Server Error", limits),
        a::Error::TooLarge | a::Error::Full | a::Error::RateLimited { .. } => answers::failed(),
        a::Error::Empty
        | a::Error::Exists
        | a::Error::Circular
        | a::Error::NothingToMerge
        | a::Error::Closed
        | a::Error::Stale
        | a::Error::Conflict
        | a::Error::Protected
        | a::Error::Unreachable
        | a::Error::Refused => answers::refusal(422, b"Validation Failed", limits),
    }
}
