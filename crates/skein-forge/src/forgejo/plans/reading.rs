//! Forgejo's documents in the forge client's vocabulary (forge.md, 3.2 and 3.3).
use crate::api;
use crate::forgejo::{
    document as d,
    id::{ObjectFormat, ObjectId},
};
use alloc::boxed::Box;
use skein_lib::{Time, Wall, Writer, bytes};

#[must_use]
pub fn ci(state: Option<d::StatusState>, total_count: u64) -> api::Ci {
    if total_count == 0 {
        return api::Ci::None;
    }
    match state {
        Some(d::StatusState::Failure | d::StatusState::Error) => api::Ci::Failed,
        Some(d::StatusState::Pending) => api::Ci::Pending,
        Some(d::StatusState::Success | d::StatusState::Warning | d::StatusState::Skipped) => api::Ci::Passed,
        None => api::Ci::None,
    }
}
#[must_use]
pub fn check(state: d::StatusState) -> api::Check {
    match state {
        d::StatusState::Failure | d::StatusState::Error => api::Check::Failed,
        d::StatusState::Pending => api::Check::Pending,
        d::StatusState::Success | d::StatusState::Warning | d::StatusState::Skipped => api::Check::Passed,
    }
}
#[must_use]
pub fn verdict(state: d::ReviewState, dismissed: bool) -> Option<api::Verdict> {
    match state {
        d::ReviewState::Pending | d::ReviewState::RequestReview => None,
        d::ReviewState::Approved | d::ReviewState::RequestChanges | d::ReviewState::Comment if dismissed => {
            Some(api::Verdict::Comment)
        }
        d::ReviewState::Approved => Some(api::Verdict::Approve),
        d::ReviewState::RequestChanges => Some(api::Verdict::RequestChanges),
        d::ReviewState::Comment => Some(api::Verdict::Comment),
    }
}
#[must_use]
pub fn permission(value: d::Permission) -> api::Permission {
    match value {
        d::Permission::None => api::Permission::None,
        d::Permission::Read => api::Permission::Read,
        d::Permission::Write => api::Permission::Write,
        d::Permission::Admin | d::Permission::Owner => api::Permission::Admin,
    }
}
#[must_use]
pub fn provenance(created: Option<Wall>, updated: Option<Wall>, body: &d::Body) -> api::Provenance {
    match body {
        d::Body::Long(_) => api::Provenance::Unknown,
        d::Body::Kept(_) => match created {
            Some(created) => match updated {
                Some(updated) if created == updated => api::Provenance::Original,
                Some(updated) if created < updated => api::Provenance::Revised,
                Some(_) | None => api::Provenance::Unknown,
            },
            None => api::Provenance::Unknown,
        },
    }
}
#[must_use]
pub fn revision(body: &d::Body, updated: Option<Wall>) -> u64 {
    let digest = match body {
        d::Body::Kept(bytes) => skein_lib::sha256::digest(bytes),
        d::Body::Long(length) => {
            let nanos = match updated {
                Some(wall) => wall.as_nanos(),
                None => 0,
            };
            let mut writer = Writer::new(16);
            writer.put(&length.to_be_bytes()).expect("length is eight bytes");
            writer.put(&nanos.to_be_bytes()).expect("time is eight bytes");
            skein_lib::sha256::digest(&writer.finish())
        }
    };
    u64::from_be_bytes(digest[..8].try_into().expect("eight digest bytes"))
}
#[must_use]
pub fn commit(id: ObjectId, format: ObjectFormat) -> Option<api::Commit> {
    if id.format == format { Some(id.bytes) } else { None }
}
#[must_use]
pub fn time(wall: Wall) -> Time {
    Time::from_nanos(wall.as_nanos())
}
fn optional_time(wall: Option<Wall>) -> Time {
    match wall {
        Some(wall) => time(wall),
        None => Time::ZERO,
    }
}
#[must_use]
pub fn body(word: &[u8], body: d::Body) -> (Option<Box<[u8]>>, Box<[u8]>) {
    match body {
        d::Body::Kept(bytes) => (crate::marker::decode(word, &bytes), bytes),
        d::Body::Long(_) => (None, Box::from([])),
    }
}
#[must_use]
pub fn summary(word: &[u8], issue: d::Issue) -> api::Summary {
    let (key, body) = body(word, issue.body);
    api::Summary {
        labels: issue.labels,
        number: issue.number,
        kind: if issue.pull { api::Kind::Pull } else { api::Kind::Issue },
        state: state(issue.state),
        author: issue.user.id,
        key,
        title: issue.title,
        body,
        updated: time(issue.updated_at),
    }
}
#[must_use]
pub fn state(value: d::IssueState) -> api::State {
    match value {
        d::IssueState::Open => api::State::Open,
        d::IssueState::Closed => api::State::Closed,
    }
}
#[must_use]
pub fn comment(word: &[u8], value: d::Comment) -> api::Comment {
    let provenance = provenance(value.created_at, value.updated_at, &value.body);
    let revision = revision(&value.body, value.updated_at);
    let (key, body) = body(word, value.body);
    api::Comment {
        id: value.id,
        provenance,
        author: value.user.id,
        created: optional_time(value.created_at),
        revision,
        key,
        body,
    }
}
pub fn review(word: &[u8], value: d::Review, format: ObjectFormat) -> Result<Option<api::Review>, api::Error> {
    let Some(verdict) = verdict(value.state, value.dismissed) else {
        return Ok(None);
    };
    let commit = commit(value.commit_id, format).ok_or(api::Error::InvalidAnswer)?;
    let provenance = provenance(value.submitted_at, value.updated_at, &value.body);
    let revision = revision(&value.body, value.updated_at);
    let (key, body) = body(word, value.body);
    Ok(Some(api::Review {
        id: value.id,
        provenance,
        revision,
        author: value.user.id,
        verdict,
        commit,
        key,
        body,
        at: optional_time(value.submitted_at),
        official: value.official,
    }))
}
#[must_use]
pub fn remark(word: &[u8], value: d::ReviewComment) -> api::Remark {
    let (_, body) = body(word, value.body);
    api::Remark { id: value.id, author: value.user.id, path: value.path, line: value.position, body }
}
pub fn status(value: d::CommitStatus, html_url: &[u8]) -> Result<api::Status, api::Error> {
    Ok(api::Status {
        at: time(value.updated_at),
        context: value.context,
        check: check(value.status),
        description: value.description,
        url: absolute(&value.target_url, html_url)?,
        job: None,
    })
}
/// A local Actions path carries the repository's origin; other links stay as returned.
pub fn absolute(link: &[u8], html_url: &[u8]) -> Result<Box<[u8]>, api::Error> {
    if !link.starts_with(b"/") || link.starts_with(b"//") {
        return Ok(bytes::copy_of(link));
    }
    let scheme = bytes::find(html_url, b"://").ok_or(api::Error::InvalidAnswer)?;
    let host = scheme.checked_add(3).ok_or(api::Error::InvalidAnswer)?;
    let mut end = html_url.len();
    for (at, byte) in html_url.iter().enumerate().skip(host) {
        if *byte == b'/' {
            end = at;
            break;
        }
    }
    let origin = html_url.get(..end).ok_or(api::Error::InvalidAnswer)?;
    let mut writer = Writer::new(origin.len().saturating_add(link.len()));
    writer.put(origin).expect("origin measured");
    writer.put(link).expect("link measured");
    Ok(writer.finish())
}
#[must_use]
pub fn settings(value: d::Repository) -> api::Settings {
    api::Settings {
        default_branch: value.default_branch,
        merge: value.allow_merge_commits,
        squash: value.allow_squash_merge,
        rebase: value.allow_rebase,
    }
}
#[must_use]
pub fn repository(value: d::Repository) -> crate::forgejo::plans::tables::Repository {
    crate::forgejo::plans::tables::Repository {
        owner: value.owner.login,
        name: value.name,
        object_format: value.object_format,
        merge_style: value.default_merge_style,
        html_url: value.html_url,
    }
}
#[must_use]
pub fn protection(value: d::BranchProtection) -> api::Protection {
    api::Protection {
        queue: false,
        signatures: value.require_signed_commits,
        linear: false,
        code_owners: false,
        bypass: api::Bypass::Never,
        branch: value.rule_name,
        contexts: if value.enable_status_check { value.status_check_contexts } else { Box::from([]) },
        approvals: value.required_approvals,
        dismiss_stale: value.dismiss_stale_approvals,
    }
}
pub fn pull(value: d::PullRequest, format: ObjectFormat, ci: api::Ci) -> Result<api::Pull, api::Error> {
    let head = commit(value.head.sha.ok_or(api::Error::InvalidAnswer)?, format).ok_or(api::Error::InvalidAnswer)?;
    let base_commit = optional_commit(value.base.sha, format)?;
    let merged = if value.merged { optional_commit(value.merge_commit_sha, format)? } else { None };
    let Ok(count) = u32::try_from(value.requested_reviewers.len()) else {
        return Err(api::Error::TooLarge);
    };
    let mut reviewers = skein_lib::List::with_capacity(count);
    for user in &value.requested_reviewers {
        reviewers.push(user.id).expect("reviewer count measured");
    }
    Ok(api::Pull {
        queued: None,
        dequeued: None,
        reviewers: reviewers.into_boxed(),
        number: value.number,
        state: state(value.state),
        head: value.head.name,
        base: value.base.name,
        commit: head,
        base_commit,
        merged,
        mergeable: value.mergeable,
        ci,
    })
}
fn optional_commit(value: Option<ObjectId>, format: ObjectFormat) -> Result<Option<api::Commit>, api::Error> {
    match value {
        Some(id) => Ok(Some(commit(id, format).ok_or(api::Error::InvalidAnswer)?)),
        None => Ok(None),
    }
}
