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
    OpenPull { owner: Box<[u8]>, name: Box<[u8]>, title: Box<[u8]>, body: Box<[u8]>, head: Box<[u8]>, base: Box<[u8]> },
    Update { owner: Box<[u8]>, name: Box<[u8]>, number: u64, head: skein_forge::api::Commit },
    Review { owner: Box<[u8]>, name: Box<[u8]>, number: u64, verdict: skein_forge::api::Verdict, body: Box<[u8]> },
    Reviewers { owner: Box<[u8]>, name: Box<[u8]>, number: u64, add: bool, logins: Box<[Box<[u8]>]> },
    Reopen { owner: Box<[u8]>, name: Box<[u8]>, number: u64 },
    Close { owner: Box<[u8]>, name: Box<[u8]>, number: u64 },
    Edit { owner: Box<[u8]>, name: Box<[u8]>, number: u64, title: Option<Box<[u8]>>, body: Option<Box<[u8]>> },
    Post { owner: Box<[u8]>, name: Box<[u8]>, number: u64, body: Box<[u8]> },
    CreateIssue { owner: Box<[u8]>, name: Box<[u8]>, title: Box<[u8]>, body: Box<[u8]> },
}
impl Route {
    fn request(self) -> Request {
        match self {
            Self::OpenPull { owner, name, title, body, head, base } => {
                Request::OpenPull { owner, name, title, body, head, base }
            }
            Self::Update { owner, name, number, head } => Request::Update { owner, name, number, head },
            Self::Review { owner, name, number, verdict, body } => {
                Request::Review { owner, name, number, verdict, body }
            }
            Self::Reviewers { owner, name, number, add, logins } => {
                Request::Reviewers { owner, name, number, add, logins }
            }
            Self::Reopen { owner, name, number } => Request::Reopen { owner, name, number },
            Self::Close { owner, name, number } => Request::Close { owner, name, number },
            Self::Edit { owner, name, number, title, body } => Request::Edit { owner, name, number, title, body },
            Self::Post { owner, name, number, body } => Request::Post { owner, name, number, body },
            Self::CreateIssue { owner, name, title, body } => Request::CreateIssue { owner, name, title, body },
        }
    }
    fn path(&self) -> (&[u8], &[u8]) {
        match self {
            Self::OpenPull { owner, name, .. }
            | Self::Update { owner, name, .. }
            | Self::Review { owner, name, .. }
            | Self::Reviewers { owner, name, .. }
            | Self::Reopen { owner, name, .. }
            | Self::Close { owner, name, .. }
            | Self::Edit { owner, name, .. }
            | Self::Post { owner, name, .. }
            | Self::CreateIssue { owner, name, .. } => (owner, name),
        }
    }
}
#[derive(Debug)]
pub(crate) struct Pending {
    repository: Box<[u8]>,
    stage: Stage,
}
#[derive(Debug)]
enum Stage {
    Done,
    Created,
    Commented,
    OpenPull { head: Box<[u8]>, base: Box<[u8]> },
    UpdateHead { number: u64, head: skein_forge::api::Commit },
    Updated,
    Reviewed,
    ReviewAuthor { number: u64, verdict: skein_forge::api::Verdict, body: Box<[u8]> },
    Reviewers(bool),
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
    if grant.scope == crate::installations::Scope::Contents {
        return Outcome::Reply(answers::refusal(403, skein_forge::github::failure::INTEGRATION, &service.limits));
    }
    if !service.body_fits(body) {
        return Outcome::Reply(answers::refusal(413, b"Payload Too Large", &service.limits));
    }
    let Some(request) = server::write_body(route.request(), body, &service.limits.github) else {
        return Outcome::Reply(answers::refusal(422, b"Validation Failed", &service.limits));
    };
    let (stage, op) = match operation(service, request) {
        Ok(value) => value,
        Err(response) => return Outcome::Reply(response),
    };
    let Some(repository) = reads::full(row) else {
        return Outcome::Reply(answers::failed());
    };
    dispatch(service, Pending { repository, stage }, op, reply_to)
}
fn dispatch(service: &Service, pending: Pending, op: Op, reply_to: ReplyTo) -> Outcome {
    let repository = pending.repository.clone();
    Outcome::Call(
        crate::service::Pending { kind: crate::service::PendingKind::Write(pending) },
        fake::Event::Call { reply_to, user: service.bot, repository, op },
    )
}
fn verdict_of(verdict: skein_forge::api::Verdict) -> a::Verdict {
    match verdict {
        skein_forge::api::Verdict::Approve => a::Verdict::Approve,
        skein_forge::api::Verdict::RequestChanges => a::Verdict::RequestChanges,
        skein_forge::api::Verdict::Comment => a::Verdict::Comment,
    }
}
fn operation(service: &Service, request: Request) -> Result<(Stage, Op), Response> {
    let (stage, write) = match request {
        Request::OpenPull { title, body, head, base, .. } => (
            Stage::OpenPull { head: head.clone(), base: base.clone() },
            Write::OpenPull { title, body, head, base, opens_empty: false },
        ),
        Request::Update { number, head, .. } => {
            return Ok((Stage::UpdateHead { number, head }, Op::Read(a::Read::Pull { number })));
        }
        Request::Review { number, verdict, body, .. } => {
            return Ok((Stage::ReviewAuthor { number, verdict, body }, Op::Read(a::Read::Item { number, after: 0 })));
        }
        Request::Reviewers { number, add, logins, .. } => {
            let mut reviewers = skein_lib::List::with_capacity(service.limits.github.inner);
            for login in logins {
                let Some(user) = reads::people::id(service, &login) else {
                    return Err(answers::refusal(422, b"Validation Failed", &service.limits));
                };
                reviewers.push(user).expect("body reader bounds logins");
            }
            let reviewers = reviewers.into_boxed();
            (
                Stage::Reviewers(add),
                if add {
                    Write::RequestReviewers { number, reviewers }
                } else {
                    Write::RemoveReviewers { number, reviewers }
                },
            )
        }
        Request::Reopen { number, .. } => (Stage::Done, Write::Reopen { number }),
        Request::Close { number, .. } => (Stage::Done, Write::Close { number }),
        Request::Edit { number, title, body, .. } => (Stage::Done, Write::EditItem { number, title, body }),
        Request::Post { number, body, .. } => (Stage::Commented, Write::Comment { number, body }),
        Request::CreateIssue { title, body, .. } => {
            (Stage::Created, Write::CreateIssue { title, body, labels: Box::from([]) })
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
        | Request::DeleteBranch { .. } => return Err(answers::failed()),
    };
    Ok((stage, Op::Write(write)))
}
pub(crate) fn finish(
    service: &Service,
    pending: Pending,
    result: Result<a::Answer, a::Error>,
    reply_to: ReplyTo,
) -> Outcome {
    let limits = &service.limits;
    let answer = match result {
        Ok(answer) => answer,
        Err(error) => return Outcome::Reply(refused(&pending.stage, error, limits)),
    };
    let response = match pending.stage {
        Stage::ReviewAuthor { number, verdict, body } => {
            let a::Answer::Item { item, .. } = answer else {
                return Outcome::Reply(answers::failed());
            };
            if item.author == service.bot && verdict != skein_forge::api::Verdict::Comment {
                return Outcome::Reply(answers::refusal(422, skein_forge::github::failure::OWN_REVIEW, limits));
            }
            return dispatch(
                service,
                Pending { repository: pending.repository, stage: Stage::Reviewed },
                Op::Write(Write::Review { number, verdict: Some(verdict_of(verdict)), body }),
                reply_to,
            );
        }
        Stage::UpdateHead { number, head } => {
            let a::Answer::Pull(pull) = answer else {
                return Outcome::Reply(answers::failed());
            };
            if reads::commit(pull.commit) != head {
                return Outcome::Reply(answers::refusal(422, skein_forge::github::failure::UPDATE_STALE, limits));
            }
            return dispatch(
                service,
                Pending { repository: pending.repository, stage: Stage::Updated },
                Op::Write(Write::Update { number }),
                reply_to,
            );
        }
        Stage::Done | Stage::Updated | Stage::Reviewers(_) => {
            let a::Answer::Done = answer else {
                return Outcome::Reply(answers::failed());
            };
            let status = match pending.stage {
                Stage::Updated => 202,
                Stage::Reviewers(true) => 201,
                Stage::Done | Stage::Reviewers(false) => 200,
                Stage::ReviewAuthor { .. }
                | Stage::Created
                | Stage::Commented
                | Stage::OpenPull { .. }
                | Stage::UpdateHead { .. }
                | Stage::Reviewed => return Outcome::Reply(answers::failed()),
            };
            answers::response(status, &Body::Done, limits)
        }
        Stage::Created | Stage::OpenPull { .. } => {
            let a::Answer::Created(number) = answer else {
                return Outcome::Reply(answers::failed());
            };
            answers::response(201, &Body::Number(number), limits)
        }
        Stage::Commented | Stage::Reviewed => {
            let (a::Answer::Commented(id) | a::Answer::Reviewed(id)) = answer else {
                return Outcome::Reply(answers::failed());
            };
            answers::response(if matches_review(&pending.stage) { 200 } else { 201 }, &Body::Id(id), limits)
        }
    };
    Outcome::Reply(response)
}
fn matches_review(stage: &Stage) -> bool {
    match stage {
        Stage::Reviewed => true,
        Stage::ReviewAuthor { .. }
        | Stage::Done
        | Stage::Created
        | Stage::Commented
        | Stage::OpenPull { .. }
        | Stage::UpdateHead { .. }
        | Stage::Updated
        | Stage::Reviewers(_) => false,
    }
}

fn refused(stage: &Stage, error: a::Error, limits: &Limits) -> Response {
    use skein_forge::github::failure;
    match stage {
        Stage::OpenPull { head, base } => {
            let text = match error {
                a::Error::NothingToMerge => {
                    let Some(prefix) = crate::service::joined(failure::NO_COMMITS, base) else {
                        return answers::failed();
                    };
                    let Some(prefix) = crate::service::joined(&prefix, b" and ") else {
                        return answers::failed();
                    };
                    crate::service::joined(&prefix, head)
                }
                a::Error::Exists => crate::service::joined(failure::PULL_EXISTS, head),
                a::Error::Unavailable
                | a::Error::Timeout
                | a::Error::RateLimited { .. }
                | a::Error::Forbidden
                | a::Error::Missing(_)
                | a::Error::TooLarge
                | a::Error::Full
                | a::Error::Circular
                | a::Error::Empty
                | a::Error::Closed
                | a::Error::Stale
                | a::Error::Conflict
                | a::Error::Protected
                | a::Error::Unreachable
                | a::Error::Refused => None,
            };
            if let Some(text) = text {
                return answers::refusal(422, &text, limits);
            }
        }
        Stage::Updated if error == a::Error::Conflict => {
            return answers::refusal(422, failure::UPDATE_CONFLICT, limits);
        }
        Stage::ReviewAuthor { .. }
        | Stage::Done
        | Stage::Created
        | Stage::Commented
        | Stage::UpdateHead { .. }
        | Stage::Updated
        | Stage::Reviewed
        | Stage::Reviewers(_) => {}
    }
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
