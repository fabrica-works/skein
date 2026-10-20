//! REST merges enforce repository methods and describe the domain's protection.
use super::{Pending, Stage as WriteStage, dispatch};
use crate::{
    answers::{self, Body},
    reads,
    service::{Limits, Outcome, Response, Service},
};
use alloc::boxed::Box;
use skein_fake_forge_domain::{self as fake, api as a};
use skein_forge::{
    api,
    github::{failure, plans::tables::Method},
};
use skein_lib::{Decimal, ReplyTo, Writer};

// recorded; other counts inferred (owner's rule-texts.md, 2026-10-20).
const APPROVAL_START: &[u8] = b"At least ";
// recorded; other counts inferred (owner's rule-texts.md, 2026-10-20).
const APPROVAL_END: &[u8] = b" approving review is required by reviewers with write access.";
// recorded 2026-10-20 (owner's rule-texts.md).
const CHECK_START: &[u8] = b"Required status check \"";
// recorded 2026-10-20 (owner's rule-texts.md).
const CHECK_EXPECTED: &[u8] = b"\" is expected.";
// recorded 2026-10-20 (owner's rule-texts.md).
const CHECK_PROGRESS: &[u8] = b"\" is in progress.";
// recorded for Actions; inferred: commit status failure unrecorded (rule-texts.md).
const CHECK_FAILING: &[u8] = b"\" is failing.";

#[derive(Debug)]
pub(super) enum Stage {
    Settings { number: u64, head: api::Commit, method: Method },
    Head { number: u64, head: api::Commit },
    Merge { number: u64 },
    Pull { number: u64 },
    Author(Box<a::Pull>),
}
fn call(service: &Service, repository: Box<[u8]>, stage: Stage, op: a::Op, reply_to: ReplyTo) -> Outcome {
    dispatch(service, Pending { repository, stage: WriteStage::Merge(stage) }, op, reply_to)
}
pub(super) fn finish(
    service: &Service,
    repository: Box<[u8]>,
    stage: Stage,
    result: Result<a::Answer, a::Error>,
    reply_to: ReplyTo,
    domain: &fake::Domain,
) -> Outcome {
    let limits = &service.limits;
    let answer = match result {
        Ok(answer) => answer,
        Err(error) => return refused(service, repository, stage, error, reply_to),
    };
    match stage {
        Stage::Settings { number, head, method } => {
            let a::Answer::Settings(settings) = answer else {
                return Outcome::Reply(answers::failed());
            };
            let allowed = match method {
                Method::Merge => settings.merge,
                Method::Squash => settings.squash,
                Method::Rebase => settings.rebase,
            };
            if !allowed {
                let prefix = match method {
                    Method::Merge => b"Merge ".as_slice(),
                    Method::Squash => b"Squash ",
                    Method::Rebase => b"Rebase ",
                };
                let Some(message) = crate::service::joined(prefix, failure::MERGE_METHOD) else {
                    return Outcome::Reply(answers::failed());
                };
                return Outcome::Reply(answers::refusal(405, &message, limits));
            }
            call(service, repository, Stage::Head { number, head }, a::Op::Read(a::Read::Pull { number }), reply_to)
        }
        Stage::Head { number, head } => {
            let a::Answer::Pull(pull) = answer else {
                return Outcome::Reply(answers::failed());
            };
            if reads::commit(pull.commit) != head {
                return Outcome::Reply(answers::refusal(409, failure::MERGE_STALE, limits));
            }
            if pull.queue_required {
                return Outcome::Reply(queue(limits));
            }
            // fake-forge.md, 10.4: the domain squashes every allowed method.
            call(
                service,
                repository,
                Stage::Merge { number },
                a::Op::Write(a::Write::Merge { number, head: pull.commit }),
                reply_to,
            )
        }
        Stage::Merge { .. } => {
            let a::Answer::Merged(commit) = answer else {
                return Outcome::Reply(answers::failed());
            };
            Outcome::Reply(answers::response(200, &Body::Merged(&reads::commit(commit)), limits))
        }
        Stage::Pull { number } => {
            let a::Answer::Pull(pull) = answer else {
                return Outcome::Reply(fallback(limits));
            };
            call(
                service,
                repository,
                Stage::Author(Box::new(pull)),
                a::Op::Read(a::Read::Item { number, after: 0 }),
                reply_to,
            )
        }
        Stage::Author(pull) => {
            let a::Answer::Item { item, .. } = answer else {
                return Outcome::Reply(fallback(limits));
            };
            Outcome::Reply(rules(domain.inspect_protection(&repository, &pull.base), &pull, item.author, limits))
        }
    }
}
fn refused(service: &Service, repository: Box<[u8]>, stage: Stage, error: a::Error, reply_to: ReplyTo) -> Outcome {
    let limits = &service.limits;
    match stage {
        Stage::Merge { number } if error == a::Error::Protected => {
            return call(service, repository, Stage::Pull { number }, a::Op::Read(a::Read::Pull { number }), reply_to);
        }
        Stage::Pull { .. } | Stage::Author(_) => return Outcome::Reply(fallback(limits)),
        Stage::Merge { .. } | Stage::Settings { .. } | Stage::Head { .. } => {}
    }
    let response = match error {
        a::Error::Stale => answers::refusal(409, failure::MERGE_STALE, limits),
        a::Error::Conflict | a::Error::Closed | a::Error::NothingToMerge => {
            answers::refusal(405, failure::NOT_MERGEABLE, limits)
        }
        a::Error::Missing(_) | a::Error::Forbidden => answers::missing(limits),
        a::Error::Unavailable => answers::refusal(503, b"Server Error", limits),
        a::Error::Timeout => answers::refusal(504, b"Server Error", limits),
        a::Error::RateLimited { .. } | a::Error::TooLarge | a::Error::Full | a::Error::AlreadyQueued(_) => {
            answers::failed()
        }
        a::Error::Exists
        | a::Error::Circular
        | a::Error::Empty
        | a::Error::Protected
        | a::Error::Unreachable
        | a::Error::Refused => answers::refusal(422, b"Validation Failed", limits),
    };
    Outcome::Reply(response)
}
fn approvals(pull: &a::Pull, author: u64, protection: &a::Protection) -> u32 {
    let mut count = 0_u32;
    for (index, review) in pull.reviews.iter().enumerate() {
        if review.author == author
            || !review.official
            || review.verdict != a::Verdict::Approve
            || (protection.dismiss_stale && review.commit != pull.commit)
        {
            continue;
        }
        let mut last = true;
        for later in pull.reviews.get(index.saturating_add(1)..).unwrap_or(&[]) {
            if later.author == review.author && later.verdict != a::Verdict::Comment {
                last = false;
            }
        }
        if last {
            count = count.saturating_add(1);
        }
    }
    count
}
fn check(pull: &a::Pull, context: &[u8]) -> Option<a::Check> {
    for status in &pull.statuses {
        if status.context.as_ref() == context {
            return Some(status.state);
        }
    }
    None
}
fn approval(out: &mut Writer, count: u32) -> Option<()> {
    out.put(APPROVAL_START).ok()?;
    out.put(Decimal::of(u64::from(count)).as_bytes()).ok()?;
    out.put(APPROVAL_END).ok()?;
    out.put(b"\n\n").ok()
}
fn context(out: &mut Writer, name: &[u8], state: Option<a::Check>) -> Option<()> {
    let suffix = match state {
        None => CHECK_EXPECTED,
        Some(a::Check::Pending) => CHECK_PROGRESS,
        Some(a::Check::Failed) => CHECK_FAILING,
        Some(a::Check::Passed) => return Some(()),
    };
    out.put(CHECK_START).ok()?;
    out.put(name).ok()?;
    out.put(suffix).ok()?;
    out.put(b"\n\n").ok()
}
fn message(protection: &a::Protection, pull: &a::Pull, author: u64, capacity: usize) -> Option<Box<[u8]>> {
    let mut out = Writer::new(capacity);
    out.put(failure::RULES).ok()?;
    out.put(b"\n\n").ok()?;
    let missing_approvals = approvals(pull, author, protection) < protection.approvals;
    if missing_approvals {
        approval(&mut out, protection.approvals)?;
    }
    let mut missing_context = false;
    for name in &protection.contexts {
        let state = check(pull, name);
        if state != Some(a::Check::Passed) {
            missing_context = true;
            context(&mut out, name, state)?;
        }
    }
    if !missing_approvals && !missing_context {
        return None;
    }
    answers::finish(out)
}
fn rules(protection: Option<&a::Protection>, pull: &a::Pull, author: u64, limits: &Limits) -> Response {
    let Some(protection) = protection else {
        return fallback(limits);
    };
    let Ok(capacity) = usize::try_from(limits.response) else {
        return answers::failed();
    };
    let Some(message) = message(protection, pull, author, capacity) else {
        return fallback(limits);
    };
    answers::response(405, &Body::Rules(&message), limits)
}
fn fallback(limits: &Limits) -> Response {
    // no unmet rule visible; unrecorded shape.
    answers::response(405, &Body::Rules(b"Repository rule violations found\n\n"), limits)
}

fn queue(limits: &Limits) -> Response {
    let Ok(capacity) = usize::try_from(limits.response) else {
        return answers::failed();
    };
    let mut out = Writer::new(capacity);
    for line in [failure::RULES, failure::QUEUE] {
        if out.put(line).is_err() || out.put(b"\n\n").is_err() {
            return answers::failed();
        }
    }
    let Some(message) = answers::finish(out) else {
        return answers::failed();
    };
    answers::response(405, &Body::Rules(&message), limits)
}
