//! The fake's delivery encoder; callers sign the returned bytes.
use super::super::{
    delivery::{self, Account, Change, Ci, Delivery},
    id,
    limits::Limits,
};
use crate::forgejo::document::Body;
use alloc::boxed::Box;
use skein_json::writer::{self, Encoder};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Sent {
    pub event: &'static [u8],
    pub body: Box<[u8]>,
}
/// Measures before allocating; `Other` is an empty ping and long bodies are refused.
pub fn encode(delivery: &Delivery, limits: &Limits) -> Result<Sent, writer::Refusal> {
    let mut measure = Encoder::measure(&limits.writer);
    let event = write(delivery, limits, &mut measure)?;
    let mut json = Encoder::write(measure.measured()?, &limits.writer);
    write(delivery, limits, &mut json)?;
    Ok(Sent { event, body: json.finish() })
}
fn text(json: &mut Encoder, key: &[u8], value: &[u8], limits: &Limits) -> Result<(), writer::Refusal> {
    if !delivery::text_fits(value, limits) {
        return Err(writer::Refusal::TooLong);
    }
    json.key(key);
    json.string(value);
    Ok(())
}
fn number(json: &mut Encoder, key: &[u8], value: u64) {
    json.key(key);
    json.unsigned(value);
}
fn repository(json: &mut Encoder, id: u64) {
    json.key(b"repository");
    json.object_start();
    number(json, b"id", id);
    json.object_end();
}
fn body(json: &mut Encoder, value: &Body, limits: &Limits) -> Result<(), writer::Refusal> {
    match value {
        Body::Long(_) => return Err(writer::Refusal::TooLong),
        Body::Kept(text) => {
            if text.len() > usize::try_from(limits.body_bytes).expect("u32 fits usize") {
                return Err(writer::Refusal::TooLong);
            }
            json.key(b"body");
            json.string(text);
        }
    }
    Ok(())
}
fn commit(json: &mut Encoder, key: &[u8], value: &crate::api::Commit, limits: &Limits) -> Result<(), writer::Refusal> {
    text(json, key, &id::write_commit(value).ok_or(writer::Refusal::Text)?, limits)
}
fn item(json: &mut Encoder, key: &[u8], value: u64) {
    json.key(key);
    json.object_start();
    number(json, b"number", value);
    json.object_end();
}
pub(super) fn write(value: &Delivery, limits: &Limits, json: &mut Encoder) -> Result<&'static [u8], writer::Refusal> {
    json.object_start();
    let event = match value {
        Delivery::Issue { repository: id, number: n, body: b } => {
            repository(json, *id);
            text(json, b"action", b"opened", limits)?;
            json.key(b"issue");
            json.object_start();
            number(json, b"number", *n);
            body(json, b, limits)?;
            json.object_end();
            b"issues".as_slice()
        }
        Delivery::Comment { repository: id, number: n, body: b }
        | Delivery::Review { repository: id, number: n, body: b } => {
            repository(json, *id);
            let (event, parent, object, action): (&[u8], &[u8], &[u8], &[u8]) = match value {
                Delivery::Comment { .. } => (b"issue_comment", b"issue", b"comment", b"created"),
                Delivery::Review { .. } => (b"pull_request_review", b"pull_request", b"review", b"submitted"),
                Delivery::Issue { .. }
                | Delivery::Pull { .. }
                | Delivery::Remark { .. }
                | Delivery::Push { .. }
                | Delivery::Ref { .. }
                | Delivery::Ci { .. }
                | Delivery::Installation { .. }
                | Delivery::Other => unreachable!("body delivery dispatch"),
            };
            text(json, b"action", action, limits)?;
            item(json, parent, *n);
            json.key(object);
            json.object_start();
            body(json, b, limits)?;
            json.object_end();
            event
        }
        Delivery::Pull { repository: id, number: n, action } => {
            if !delivery::pull_action(&action.action) {
                return Err(writer::Refusal::Text);
            }
            repository(json, *id);
            number(json, b"number", *n);
            text(json, b"action", &action.action, limits)?;
            match &action.reason {
                Some(reason) => {
                    if action.action.as_ref() != b"dequeued" {
                        return Err(writer::Refusal::Text);
                    }
                    text(json, b"reason", reason, limits)?;
                }
                None => {
                    if action.action.as_ref() == b"dequeued" {
                        return Err(writer::Refusal::Text);
                    }
                }
            }
            b"pull_request"
        }
        Delivery::Remark { repository: id, number: n } => {
            repository(json, *id);
            text(json, b"action", b"created", limits)?;
            item(json, b"pull_request", *n);
            b"pull_request_review_comment"
        }
        Delivery::Push { repository: id, branch, after, deleted, sender } => {
            repository(json, *id);
            if branch.len().saturating_add(b"refs/heads/".len())
                > usize::try_from(limits.string_bytes.min(limits.json.string)).expect("u32 fits usize")
            {
                return Err(writer::Refusal::TooLong);
            }
            json.key(b"ref");
            json.string_parts(&[b"refs/heads/", branch]);
            commit(json, b"after", after, limits)?;
            json.key(b"deleted");
            json.boolean(*deleted);
            json.key(b"sender");
            json.object_start();
            number(json, b"id", *sender);
            json.object_end();
            b"push"
        }
        Delivery::Ref { repository: id, branch, created } => {
            repository(json, *id);
            text(json, b"ref", branch, limits)?;
            text(json, b"ref_type", b"branch", limits)?;
            if *created { b"create" } else { b"delete" }
        }
        Delivery::Ci { repository: id, commit: sha, event } => ci(json, *id, sha, *event, limits)?,
        Delivery::Installation { installation: id, account, change } => {
            installation(json, *id, account, change, limits)?
        }
        Delivery::Other => b"ping",
    };
    json.object_end();
    Ok(event)
}

fn ci(
    json: &mut Encoder,
    id: u64,
    sha: &crate::api::Commit,
    event: Ci,
    limits: &Limits,
) -> Result<&'static [u8], writer::Refusal> {
    repository(json, id);
    let (name, object, action): (&[u8], &[u8], &[u8]) = match event {
        Ci::CheckRun => (b"check_run", b"check_run", b"completed"),
        Ci::CheckSuite => (b"check_suite", b"check_suite", b"completed"),
        Ci::WorkflowJob => (b"workflow_job", b"workflow_job", b"completed"),
        Ci::WorkflowRun => (b"workflow_run", b"workflow_run", b"completed"),
        Ci::ChecksRequested => (b"merge_group", b"merge_group", b"checks_requested"),
        Ci::Destroyed => (b"merge_group", b"merge_group", b"destroyed"),
        Ci::Status => (b"status", b"", b""),
    };
    if event == Ci::Status {
        commit(json, b"sha", sha, limits)?;
        text(json, b"state", b"success", limits)?;
    } else {
        text(json, b"action", action, limits)?;
        json.key(object);
        json.object_start();
        commit(json, b"head_sha", sha, limits)?;
        json.object_end();
    }
    Ok(name)
}

fn installation(
    json: &mut Encoder,
    installation: u64,
    account: &Account,
    change: &Change,
    limits: &Limits,
) -> Result<&'static [u8], writer::Refusal> {
    let (event, action): (&[u8], &[u8]) = match change {
        Change::Created => (b"installation", b"created"),
        Change::Deleted => (b"installation", b"deleted"),
        Change::Suspended => (b"installation", b"suspend"),
        Change::Unsuspended => (b"installation", b"unsuspend"),
        Change::Repositories => (b"installation_repositories", b"added"),
        Change::Renamed { login } => {
            if login != &account.login {
                return Err(writer::Refusal::Text);
            }
            (b"installation_target", b"renamed")
        }
    };
    text(json, b"action", action, limits)?;
    json.key(b"installation");
    json.object_start();
    number(json, b"id", installation);
    if event == b"installation_target" {
        json.object_end();
    }
    json.key(b"account");
    json.object_start();
    number(json, b"id", account.id);
    text(json, b"login", &account.login, limits)?;
    json.object_end();
    if event != b"installation_target" {
        json.object_end();
    }
    Ok(event)
}
