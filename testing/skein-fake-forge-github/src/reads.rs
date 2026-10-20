//! Repository reads each translate to one shared-domain call.
use crate::{
    answers::{self, Body, Page},
    installations::Grant,
    service::{self, Limits, Outcome, Response, Row, Service},
};
use alloc::boxed::Box;
use skein_fake_forge_domain::{
    self as fake,
    api::{self as a, Answer, Error, Op},
};
use skein_forge::{
    api,
    forgejo::id::{ObjectFormat, ObjectId},
    github::document as d,
};
use skein_lib::{List, ReplyTo, Wall, bytes};

mod issues;
mod people;
mod pulls;
mod reviews;

#[derive(Debug)]
pub(crate) enum Read {
    Repository(u64),
    Reviews { owner: Box<[u8]>, name: Box<[u8]>, number: u64, page: u32, per_page: u32 },
    ReviewComments { owner: Box<[u8]>, name: Box<[u8]>, number: u64, review: u64 },
    Collaborators { owner: Box<[u8]>, name: Box<[u8]>, page: u32, per_page: u32 },
    Permission { owner: Box<[u8]>, name: Box<[u8]>, login: Box<[u8]> },
    Pull { owner: Box<[u8]>, name: Box<[u8]>, number: u64 },
    PullsFor { owner: Box<[u8]>, name: Box<[u8]>, head: Box<[u8]>, base: Box<[u8]>, open: bool },
    PullFiles { owner: Box<[u8]>, name: Box<[u8]>, number: u64, page: u32, per_page: u32 },
    Compare { owner: Box<[u8]>, name: Box<[u8]>, before: api::Commit, after: api::Commit, per_page: u32 },
    Raw { owner: Box<[u8]>, name: Box<[u8]>, commit: api::Commit, path: Box<[u8]> },
    Issues { owner: Box<[u8]>, name: Box<[u8]>, since: Wall, page: u32, per_page: u32 },
    Issue { owner: Box<[u8]>, name: Box<[u8]>, number: u64 },
    Comments { owner: Box<[u8]>, name: Box<[u8]>, number: u64, page: u32, per_page: u32 },
    Locate { owner: Box<[u8]>, name: Box<[u8]> },
    Branch { owner: Box<[u8]>, name: Box<[u8]>, branch: Box<[u8]> },
    Branches { owner: Box<[u8]>, name: Box<[u8]>, page: u32, per_page: u32 },
}
#[derive(Debug)]
pub(crate) enum Pending {
    Repository(Row),
    Reviews { page: u32, size: u32, target: Box<[u8]> },
    ReviewComments(u64),
    Collaborators { page: u32, size: u32, target: Box<[u8]> },
    Permission,
    Pull,
    PullsFor { open: bool },
    PullFiles { target: Box<[u8]>, page: u32, size: u32 },
    Compare { size: u32 },
    Raw,
    Issues { target: Box<[u8]>, page: u32, size: u32 },
    Issue,
    Comments { target: Box<[u8]>, page: u32, size: u32 },
    Branch(Box<[u8]>),
    Branches { page: u32, size: u32, target: Box<[u8]> },
}
fn row<'a>(service: &'a Service, read: &Read) -> Option<&'a Row> {
    match read {
        Read::Repository(id) => service.repositories.get(&u32::try_from(*id).ok()?),
        Read::Reviews { owner, name, .. }
        | Read::ReviewComments { owner, name, .. }
        | Read::Collaborators { owner, name, .. }
        | Read::Permission { owner, name, .. }
        | Read::Pull { owner, name, .. }
        | Read::PullsFor { owner, name, .. }
        | Read::PullFiles { owner, name, .. }
        | Read::Compare { owner, name, .. }
        | Read::Raw { owner, name, .. }
        | Read::Issues { owner, name, .. }
        | Read::Issue { owner, name, .. }
        | Read::Comments { owner, name, .. }
        | Read::Locate { owner, name }
        | Read::Branch { owner, name, .. }
        | Read::Branches { owner, name, .. } => {
            for (_, row) in &service.repositories {
                if row.owner == *owner && row.name == *name {
                    return Some(row);
                }
            }
            None
        }
    }
}
pub(crate) fn call(
    service: &Service,
    read: Read,
    grant: &Grant,
    target: &[u8],
    body_fits: bool,
    reply_to: ReplyTo,
) -> Outcome {
    let Some(row) = row(service, &read) else {
        return Outcome::Reply(answers::missing(&service.limits));
    };
    if !service.installations.reaches(grant, row.id) {
        return Outcome::Reply(answers::missing(&service.limits));
    }
    if !body_fits {
        return Outcome::Reply(answers::refusal(413, b"Payload Too Large", &service.limits));
    }
    let Some(repository) = full(row) else {
        return Outcome::Reply(answers::failed());
    };
    let (kind, op) = match read {
        Read::Reviews { number, page, per_page, .. } => (
            Pending::Reviews { page, size: per_page.min(service.limits.page), target: bytes::copy_of(target) },
            Op::Read(a::Read::Pull { number }),
        ),
        Read::ReviewComments { number, review, .. } => {
            (Pending::ReviewComments(review), Op::Read(a::Read::Pull { number }))
        }
        Read::Collaborators { page, per_page, .. } => (
            Pending::Collaborators { page, size: per_page.min(service.limits.page), target: bytes::copy_of(target) },
            Op::Read(a::Read::Collaborators),
        ),
        Read::Permission { login, .. } => {
            let Some(user) = people::id(service, &login) else {
                return Outcome::Reply(answers::missing(&service.limits));
            };
            (Pending::Permission, Op::Read(a::Read::Permission { user }))
        }
        Read::Pull { number, .. } => (Pending::Pull, Op::Read(a::Read::Pull { number })),
        Read::PullsFor { head, base, open, .. } => {
            (Pending::PullsFor { open }, Op::Read(a::Read::PullFor { head, base }))
        }
        Read::PullFiles { number, page, per_page, .. } => (
            Pending::PullFiles { target: bytes::copy_of(target), page, size: per_page.min(service.limits.page) },
            Op::Read(a::Read::PullFiles { number, page, limit: per_page.min(service.limits.page) }),
        ),
        Read::Compare { before, after, per_page, .. } => {
            let (Some(base), Some(head)) = (count(&before), count(&after)) else {
                return Outcome::Reply(answers::missing(&service.limits));
            };
            (
                Pending::Compare { size: per_page.min(service.limits.page) },
                Op::Read(a::Read::Compare { base, head, page: 1, limit: per_page.min(service.limits.page) }),
            )
        }
        Read::Raw { commit, path, .. } => {
            let Some(commit) = count(&commit) else {
                return Outcome::Reply(answers::missing(&service.limits));
            };
            (Pending::Raw, Op::Read(a::Read::File { commit, path }))
        }
        Read::Issues { since, page, per_page, .. } => (
            Pending::Issues { target: bytes::copy_of(target), page, size: per_page.min(service.limits.page) },
            Op::Read(a::Read::Items {
                state: None,
                kind: None,
                labels: Box::from([]),
                author: None,
                order: a::Order::LeastUpdated,
                since: skein_lib::Time::from_nanos(since.as_nanos()),
                page,
                limit: per_page.min(service.limits.page),
            }),
        ),
        Read::Issue { number, .. } => (Pending::Issue, Op::Read(a::Read::Item { number, after: 0 })),
        Read::Comments { number, page, per_page, .. } => (
            Pending::Comments { target: bytes::copy_of(target), page, size: per_page.min(service.limits.page) },
            Op::Read(a::Read::Comments { number }),
        ),
        Read::Repository(_) | Read::Locate { .. } => (Pending::Repository(row.clone()), Op::Read(a::Read::Settings)),
        Read::Branch { branch, .. } => (Pending::Branch(branch.clone()), Op::Read(a::Read::Branch { branch })),
        Read::Branches { page, per_page, .. } => (
            Pending::Branches { page, size: per_page.min(service.limits.page), target: bytes::copy_of(target) },
            Op::Git(a::Git::Clone),
        ),
    };
    Outcome::Call(service::Pending { kind }, fake::Event::Call { reply_to, user: service.bot, repository, op })
}
fn full(row: &Row) -> Option<Box<[u8]>> {
    let owner = service::joined(&row.owner, b"/")?;
    service::joined(&owner, &row.name)
}
// fake-forge.md, 4: the first eight bytes hold the domain count, big-endian;
// the rest of the SHA-1 is zero. This spelling is shared with the Forgejo face.
fn object(commit: u64) -> ObjectId {
    let mut bytes = [0; 32];
    for (out, byte) in bytes.iter_mut().zip(commit.to_be_bytes()) {
        *out = byte;
    }
    ObjectId { format: ObjectFormat::Sha1, bytes }
}
fn commit(count: u64) -> api::Commit {
    object(count).bytes
}

// Reject a non-domain SHA rather than silently folding it onto a stored count.
#[expect(clippy::manual_map, reason = "step code spells Option transformations with exhaustive matches")]
fn optional_commit(value: Option<u64>) -> Option<api::Commit> {
    match value {
        Some(value) => Some(commit(value)),
        None => None,
    }
}
fn count(value: &api::Commit) -> Option<u64> {
    let first = value.get(..8)?.try_into().ok()?;
    let count = u64::from_be_bytes(first);
    if commit(count) == *value { Some(count) } else { None }
}

pub(crate) fn finish(pending: service::Pending, result: Result<Answer, Error>, limits: &Limits) -> Response {
    let result = match &pending.kind {
        Pending::PullsFor { .. } => match result {
            Err(Error::Missing(a::What::Item | a::What::Pull)) => {
                return answers::response(200, &Body::Pulls(&[]), limits);
            }
            result => result,
        },
        Pending::Reviews { .. }
        | Pending::ReviewComments(_)
        | Pending::Collaborators { .. }
        | Pending::Permission
        | Pending::Repository(_)
        | Pending::Issues { .. }
        | Pending::Issue
        | Pending::Comments { .. }
        | Pending::Branch(_)
        | Pending::Branches { .. }
        | Pending::Pull
        | Pending::PullFiles { .. }
        | Pending::Compare { .. }
        | Pending::Raw => result,
    };
    let answer = match result {
        Ok(answer) => answer,
        Err(error) => return refused(error, limits),
    };
    match pending.kind {
        Pending::Reviews { page, size, target } => reviews::reviews(answer, page, size, &target, limits),
        Pending::ReviewComments(review) => reviews::comments(answer, review, limits),
        Pending::Collaborators { page, size, target } => people::collaborators(answer, page, size, &target, limits),
        Pending::Permission => {
            let Answer::Permission(value) = answer else {
                return answers::failed();
            };
            answers::response(200, &Body::Permission(people::permission(value)), limits)
        }
        Pending::Pull => pulls::pull(answer, limits),
        Pending::PullsFor { open } => pulls::pulls_for(answer, open, limits),
        Pending::PullFiles { target, page, size } => pulls::files(answer, &target, page, size, limits),
        Pending::Compare { size } => pulls::compare(answer, size, limits),
        Pending::Raw => {
            let Answer::File(body) = answer else {
                return answers::failed();
            };
            if body.len() > usize::try_from(limits.response).expect("response bound") {
                return answers::failed();
            }
            Response { status: 200, link: None, body }
        }
        Pending::Issues { target, page, size } => issues::items(answer, &target, page, size, limits),
        Pending::Issue => issues::item(answer, limits),
        Pending::Comments { target, page, size } => issues::comments(answer, &target, page, size, limits),
        Pending::Repository(row) => repository(row, answer, limits),
        Pending::Branch(name) => {
            let Answer::Commit(count) = answer else {
                return answers::failed();
            };
            answers::response(200, &Body::Branch(&d::Branch { name, commit: commit(count) }), limits)
        }
        Pending::Branches { page, size, target } => {
            let Answer::Cloned { branches, .. } = answer else {
                return answers::failed();
            };
            let Ok(total) = u64::try_from(branches.len()) else {
                return answers::failed();
            };
            let Some(page) = Page::new(page, size, total) else {
                return answers::failed();
            };
            let mut names = List::with_capacity(size);
            for (index, branch) in branches.iter().enumerate() {
                let Ok(index) = u64::try_from(index) else {
                    return answers::failed();
                };
                if page.contains(index) {
                    let Ok(length) = u32::try_from(branch.branch.len()) else {
                        return answers::failed();
                    };
                    if length > limits.github.string_bytes || names.push(branch.branch.as_ref()).is_err() {
                        return answers::failed();
                    }
                }
            }
            page.link(answers::response(200, &Body::Branches(names.as_slice()), limits), &target, limits)
        }
    }
}
fn refused(error: Error, limits: &Limits) -> Response {
    match error {
        Error::Missing(_) | Error::Forbidden => answers::missing(limits),
        Error::Unavailable => answers::refusal(503, b"Server Error", limits),
        Error::Timeout => answers::refusal(504, b"Server Error", limits),
        // Rate heads/refusals belong to the rates task; no rate simulation here.
        Error::RateLimited { .. }
        | Error::TooLarge
        | Error::Full
        | Error::Exists
        | Error::Circular
        | Error::NothingToMerge
        | Error::Empty
        | Error::Closed
        | Error::Stale
        | Error::Conflict
        | Error::Protected
        | Error::Unreachable
        | Error::Refused => answers::failed(),
    }
}

fn repository(row: Row, answer: Answer, limits: &Limits) -> Response {
    let Answer::Settings(settings) = answer else {
        return answers::failed();
    };
    let Ok(length) = u32::try_from(settings.default.len()) else {
        return answers::failed();
    };
    if length > limits.github.string_bytes {
        return answers::failed();
    }
    let Some(full) = full(&row) else {
        return answers::failed();
    };
    // Routing/owner data is GitHub's table; settings and default come
    // only from the domain. html_url uses GitHub's public web origin.
    let Some(html_url) = service::joined(b"https://github.com/", &full) else {
        return answers::failed();
    };
    let document = d::Repository {
        id: u64::from(row.id),
        owner: row.owner,
        owner_id: row.owner_id,
        organization: row.organization,
        name: row.name,
        default_branch: settings.default,
        html_url,
        merge: settings.merge,
        squash: settings.squash,
        rebase: settings.rebase,
    };
    answers::response(200, &Body::Repository(&document), limits)
}

pub(crate) fn user(service: &Service, id: u64, body_fits: bool) -> Response {
    people::user(service, id, body_fits)
}
