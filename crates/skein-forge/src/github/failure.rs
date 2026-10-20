//! GitHub's whole-line failure map (forge-github.md, section 7).
use super::{document::Refusal, header::Head};
use crate::api;
use skein_lib::Duration;
/// Secondary limit text, matched whole (rate-core-5000).
pub const SECONDARY: &[u8] = b"You have exceeded a secondary rate limit";
/// Integration permission text, matched whole (user-on-installation-403).
pub const INTEGRATION: &[u8] = b"Resource not accessible by integration";
/// Moved head text, matched whole (merge-stale-409).
pub const MERGE_STALE: &[u8] = b"Head branch was modified. Review and try the merge again.";
/// Unmergeable text, matched whole (merge-after-move-405-not-mergeable; merge-closed-405-not-mergeable).
pub const NOT_MERGEABLE: &[u8] = b"Pull Request is not mergeable";
/// Rules message, matched whole (ruleset-approval-405).
pub const RULES: &[u8] = b"Repository rule violations found";
/// Queue rule, matched whole (queue-direct-merge-405).
pub const QUEUE: &[u8] = b"Changes must be made through the merge queue";
/// Disabled merge method suffix (repository-allows-shown).
pub const MERGE_METHOD: &[u8] = b"merges are not allowed on this repository.";
/// Enqueue stale head suffix (enqueue-stale-unprocessable).
pub const ENQUEUE_STALE: &[u8] = b"expected head oid does not match the current head oid";
/// Update stale head, matched whole (update-stale-422).
pub const UPDATE_STALE: &[u8] = "expected head sha didn’t match current head ref.".as_bytes();
/// Update conflict, matched whole (update-conflict-422).
pub const UPDATE_CONFLICT: &[u8] = b"merge conflict between base and head";
/// Empty comparison prefix (pulls-422-no-commits).
pub const NO_COMMITS: &[u8] = b"No commits between ";
/// Existing pull prefix (pulls-422-exists).
pub const PULL_EXISTS: &[u8] = b"A pull request already exists for ";
/// Self approval, matched whole (approve-own-422).
pub const OWN_REVIEW: &[u8] = b"Can not approve your own pull request";
/// Existing reference, matched whole (refs-422-exists).
pub const REF_EXISTS: &[u8] = b"Reference already exists";
/// Absent reference, matched whole (refs-delete-missing-422).
pub const REF_MISSING: &[u8] = b"Reference does not exist";
/// The call's failure context; empty text is known by the sending plan.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Asked {
    Read,
    Job,
    Merge,
    Enqueue,
    Update,
    OpenPull { empty: bool },
    Review { empty: bool },
    CreateBranch,
    DeleteBranch,
    Write { empty: bool },
}
/// Whether the component invalidates its repository row.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Drop {
    Nothing,
    Row,
}
/// A failure, or a call-specific answer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Verdict {
    Fail(api::Error, Drop),
    Enqueue,
    BranchExists,
}
/// Matches the message's lines, then each errors[].message, without trimming.
fn has(refusal: Option<&Refusal>, text: &[u8], mode: Match) -> bool {
    let Some(refusal) = refusal else {
        return false;
    };
    let mut rest = refusal.message.as_ref();
    for _ in 0..=rest.len() {
        let (line, tail) = match skein_lib::bytes::find(rest, b"\n") {
            Some(at) => (rest.get(..at).expect("line"), Some(rest.get(at.saturating_add(1)..).expect("line"))),
            None => (rest, None),
        };
        if line_matches(line, text, mode) {
            return true;
        }
        match tail {
            Some(tail) => rest = tail,
            None => break,
        }
    }
    for line in &refusal.errors {
        if line_matches(line, text, mode) {
            return true;
        }
    }
    false
}
#[derive(Clone, Copy, Debug)]
enum Match {
    Whole,
    Prefix,
    Suffix,
}
fn line_matches(line: &[u8], text: &[u8], mode: Match) -> bool {
    match mode {
        Match::Whole => line == text,
        Match::Prefix => line.starts_with(text),
        Match::Suffix => line.ends_with(text),
    }
}
/// Classifies a status and refusal, after transport has answered.
#[must_use]
pub fn classify(asked: Asked, status: u16, head: &Head, refusal: Option<&Refusal>) -> Verdict {
    let read = asked == Asked::Read || asked == Asked::Job;
    let unknown = if read { api::Error::InvalidAnswer } else { api::Error::Refused };
    if status == 301 || status == 307 {
        return Verdict::Fail(api::Error::Unavailable, Drop::Row);
    }
    if status == 404 || status == 410 {
        return Verdict::Fail(
            if asked == Asked::Job { api::Error::MissingJob } else { api::Error::Missing },
            Drop::Row,
        );
    }
    if status == 403 || status == 429 {
        if head.remaining() == Some(0) {
            let after = match head.retry_after {
                Some(after) => after,
                None => match (head.reset(), head.date) {
                    (Some(reset), Some(date)) => Duration::from_nanos(reset.as_nanos().saturating_sub(date.as_nanos())),
                    (Some(_) | None, None) | (None, Some(_)) => seconds(60),
                },
            };
            return Verdict::Fail(api::Error::RateLimited { after }, Drop::Nothing);
        }
        if head.retry_after.is_some() || has(refusal, SECONDARY, Match::Whole) {
            return Verdict::Fail(
                api::Error::RateLimited { after: head.retry_after.unwrap_or(seconds(60)) },
                Drop::Nothing,
            );
        }
    }
    let empty = match asked {
        Asked::Write { empty } | Asked::OpenPull { empty } | Asked::Review { empty } => empty,
        Asked::Read
        | Asked::Job
        | Asked::Merge
        | Asked::Enqueue
        | Asked::Update
        | Asked::CreateBranch
        | Asked::DeleteBranch => false,
    };
    let error = match status {
        401 | 403 => api::Error::Forbidden,
        500..=599 => api::Error::Timeout,
        409 if asked == Asked::Merge && has(refusal, MERGE_STALE, Match::Whole) => api::Error::Stale,
        405 if asked == Asked::Merge => {
            if has(refusal, NOT_MERGEABLE, Match::Whole) {
                api::Error::Conflict
            } else if has(refusal, RULES, Match::Whole) {
                if has(refusal, QUEUE, Match::Whole) {
                    return Verdict::Enqueue;
                }
                api::Error::Protected
            } else if has(refusal, MERGE_METHOD, Match::Suffix) {
                return Verdict::Fail(api::Error::Refused, Drop::Row);
            } else {
                unknown
            }
        }
        422 if empty => api::Error::Empty,
        422 => match asked {
            Asked::Update if has(refusal, UPDATE_STALE, Match::Whole) => api::Error::Stale,
            Asked::Update if has(refusal, UPDATE_CONFLICT, Match::Whole) => api::Error::Conflict,
            Asked::OpenPull { .. } if has(refusal, NO_COMMITS, Match::Prefix) => api::Error::NothingToMerge,
            Asked::OpenPull { .. } if has(refusal, PULL_EXISTS, Match::Prefix) => api::Error::Exists,
            Asked::CreateBranch if has(refusal, REF_EXISTS, Match::Whole) => return Verdict::BranchExists,
            Asked::DeleteBranch if has(refusal, REF_MISSING, Match::Whole) => api::Error::Missing,
            Asked::Read
            | Asked::Job
            | Asked::Merge
            | Asked::Enqueue
            | Asked::Update
            | Asked::OpenPull { .. }
            | Asked::Review { .. }
            | Asked::CreateBranch
            | Asked::DeleteBranch
            | Asked::Write { .. } => unknown,
        },
        _ => unknown,
    };
    Verdict::Fail(error, Drop::Nothing)
}
fn seconds(n: u64) -> Duration {
    Duration::from_nanos(n.saturating_mul(1_000_000_000))
}
/// Classifies GraphQL enqueue errors by type and whole-message suffix.
#[must_use]
pub fn graphql(kind: &[u8], message: &[u8]) -> api::Error {
    if kind == b"UNPROCESSABLE" && message.ends_with(ENQUEUE_STALE) { api::Error::Stale } else { api::Error::Refused }
}
