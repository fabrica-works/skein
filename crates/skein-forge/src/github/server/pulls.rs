//! Pull and CI documents for the fake, measured before writing.
use super::{document, id, output_limits};
use crate::{api, forgejo::time};
use skein_json::writer::Encoder;
use skein_lib::{Overflow, Writer};

#[derive(Debug)]
enum Answer<'a> {
    Refusal(&'a [u8]),
    Merge(&'a api::Commit),
    Enqueued(u32),
    EnqueueRefused(&'a [u8], &'a [u8]),
    Pull(&'a document::Pull),
    PullGraphql(&'a document::PullGraphql),
    Pulls(&'a [document::Pull]),
    Combined(&'a document::Combined, &'a [document::Context]),
    Runs(&'a [document::Run]),
    Job(&'a document::Job),
}
/// Writes the REST merge refusal's message and recorded metadata.
pub fn merge_refusal(message: &[u8], out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Refusal(message), out)
}
/// Writes the REST merge's landed commit.
pub fn merged(commit: &api::Commit, out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Merge(commit), out)
}
/// Writes a nonnull enqueue entry and its position.
pub fn enqueued(position: u32, out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Enqueued(position), out)
}
/// GitHub answers a refused `enqueuePullRequest` this way, as recorded in live session 1917. The
/// error's `locations` is left out: it depends on the query's text.
pub fn enqueue_refused(kind: &[u8], message: &[u8], out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::EnqueueRefused(kind, message), out)
}
/// Writes one pull including its merged flag.
pub fn pull(row: &document::Pull, out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Pull(row), out)
}
/// Writes the shared GraphQL pull query's answer.
pub fn pull_graphql(row: &document::PullGraphql, out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::PullGraphql(row), out)
}
/// Writes a pull listing, which omits the merged flag.
pub fn pulls(rows: &[document::Pull], out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Pulls(rows), out)
}
/// Writes contexts followed by the combined envelope's total.
pub fn combined(row: &document::Combined, contexts: &[document::Context], out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Combined(row, contexts), out)
}
/// Writes the check run envelope with any retained metadata.
pub fn runs(rows: &[document::Run], out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Runs(rows), out)
}
/// Writes the Actions job identity and attempt, without steps or logs.
pub fn job(row: &document::Job, out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Job(row), out)
}
fn encode(answer: &Answer<'_>, out: &mut Writer) -> Result<(), Overflow> {
    let limits = output_limits(out)?;
    let mut measure = Encoder::measure(&limits);
    answer_json(answer, &mut measure)?;
    let Ok(length) = measure.measured() else {
        return Err(Overflow);
    };
    let mut json = Encoder::write(length, &limits);
    answer_json(answer, &mut json)?;
    out.put(&json.finish())
}
fn answer_json(answer: &Answer<'_>, json: &mut Encoder) -> Result<(), Overflow> {
    match answer {
        Answer::Refusal(message) => {
            json.object_start();
            json.key(b"message");
            json.string(message);
            json.key(b"documentation_url");
            json.string(b"https://docs.github.com/rest/pulls/pulls#merge-a-pull-request");
            json.key(b"status");
            json.string(b"405");
            json.object_end();
        }
        Answer::Merge(commit) => {
            json.object_start();
            json.key(b"merged");
            json.boolean(true);
            json.key(b"message");
            json.string(b"Pull Request successfully merged");
            json.key(b"sha");
            json.string(&id::write_commit(commit).ok_or(Overflow)?);
            json.object_end();
        }
        Answer::Enqueued(position) => {
            json.object_start();
            json.key(b"data");
            json.object_start();
            json.key(b"enqueuePullRequest");
            json.object_start();
            json.key(b"mergeQueueEntry");
            json.object_start();
            json.key(b"position");
            json.unsigned(u64::from(*position));
            json.object_end();
            json.object_end();
            json.object_end();
            json.object_end();
        }
        Answer::EnqueueRefused(kind, message) => enqueue_refused_json(kind, message, json),
        Answer::PullGraphql(row) => pull_graphql_json(row, json)?,
        Answer::Job(row) => {
            json.object_start();
            json.key(b"id");
            json.unsigned(row.id);
            json.key(b"run_id");
            json.unsigned(row.run);
            json.key(b"run_attempt");
            json.unsigned(u64::from(row.attempt));
            json.key(b"head_sha");
            json.string(&id::write_commit(&row.head).ok_or(Overflow)?);
            json.object_end();
        }
        Answer::Pull(row) => pull_json(row, true, json)?,
        Answer::Pulls(rows) => {
            json.array_start();
            for row in *rows {
                pull_json(row, false, json)?;
            }
            json.array_end();
        }
        Answer::Combined(row, contexts) => {
            json.object_start();
            json.key(b"state");
            json.string(state(row.check));
            json.key(b"statuses");
            json.array_start();
            for context in *contexts {
                context_json(context, json);
            }
            json.array_end();
            json.key(b"total_count");
            json.unsigned(row.total);
            json.object_end();
        }
        Answer::Runs(rows) => {
            json.object_start();
            json.key(b"check_runs");
            json.array_start();
            for row in *rows {
                json.object_start();
                json.key(b"status");
                match row.check {
                    api::Check::Pending => json.string(b"in_progress"),
                    api::Check::Passed | api::Check::Failed => {
                        json.string(b"completed");
                        json.key(b"conclusion");
                        json.string(if row.check == api::Check::Passed { b"success" } else { b"failure" });
                    }
                }
                run_metadata(row, json)?;
                json.object_end();
            }
            json.array_end();
            json.object_end();
        }
    }
    Ok(())
}
fn enqueue_refused_json(kind: &[u8], message: &[u8], json: &mut Encoder) {
    json.object_start();
    json.key(b"data");
    json.object_start();
    json.key(b"enqueuePullRequest");
    json.null();
    json.object_end();
    json.key(b"errors");
    json.array_start();
    json.object_start();
    json.key(b"type");
    json.string(kind);
    json.key(b"path");
    json.array_start();
    json.string(b"enqueuePullRequest");
    json.array_end();
    json.key(b"message");
    json.string(message);
    json.object_end();
    json.array_end();
    json.object_end();
}
fn pull_json(row: &document::Pull, merged: bool, json: &mut Encoder) -> Result<(), Overflow> {
    json.object_start();
    json.key(b"number");
    json.unsigned(row.number);
    json.key(b"state");
    json.string(if row.closed { b"closed" } else { b"open" });
    if merged {
        json.key(b"merged");
        json.boolean(row.merged);
    }
    json.key(b"mergeable");
    match row.mergeable {
        Some(mergeable) => json.boolean(mergeable),
        None => json.null(),
    }
    json.key(b"head");
    reference(&row.head, &row.head_commit, json)?;
    json.key(b"base");
    reference(&row.base, &row.base_commit, json)?;
    json.key(b"requested_reviewers");
    json.array_start();
    for reviewer in &row.reviewers {
        json.object_start();
        json.key(b"id");
        json.unsigned(*reviewer);
        json.object_end();
    }
    json.array_end();
    json.object_end();
    Ok(())
}
fn reference(name: &[u8], commit: &api::Commit, json: &mut Encoder) -> Result<(), Overflow> {
    json.object_start();
    json.key(b"ref");
    json.string(name);
    json.key(b"sha");
    json.string(&id::write_commit(commit).ok_or(Overflow)?);
    json.object_end();
    Ok(())
}
fn context_json(row: &document::Context, json: &mut Encoder) {
    json.object_start();
    if let Some(updated) = row.updated {
        json.key(b"updated_at");
        json.string(&time::write(updated));
    }
    json.key(b"context");
    json.string(&row.context);
    json.key(b"state");
    json.string(state(row.check));
    json.key(b"description");
    json.string(&row.description);
    json.key(b"target_url");
    json.string(&row.url);
    json.object_end();
}
fn state(check: api::Check) -> &'static [u8] {
    match check {
        api::Check::Pending => b"pending",
        api::Check::Passed => b"success",
        api::Check::Failed => b"failure",
    }
}

fn run_metadata(row: &document::Run, json: &mut Encoder) -> Result<(), Overflow> {
    if let Some(id) = row.id {
        json.key(b"id");
        json.unsigned(id);
    }
    if let Some(head) = &row.head {
        json.key(b"head_sha");
        json.string(&id::write_commit(head).ok_or(Overflow)?);
    }
    if let Some(at) = row.completed {
        json.key(b"completed_at");
        json.string(&time::write(at));
    }
    if let Some(at) = row.started {
        json.key(b"started_at");
        json.string(&time::write(at));
    }
    json.key(b"name");
    json.string(&row.name);
    json.key(b"html_url");
    json.string(&row.url);
    json.key(b"output");
    json.object_start();
    json.key(b"title");
    json.string(&row.title);
    json.object_end();
    if let Some(app) = &row.app {
        json.key(b"app");
        json.object_start();
        json.key(b"slug");
        json.string(app);
        json.object_end();
    }
    Ok(())
}

fn pull_graphql_json(row: &document::PullGraphql, json: &mut Encoder) -> Result<(), Overflow> {
    json.object_start();
    json.key(b"data");
    json.object_start();
    json.key(b"repository");
    json.object_start();
    json.key(b"pullRequest");
    json.object_start();
    json.key(b"id");
    json.string(&row.id);
    json.key(b"mergeCommit");
    if let Some(commit) = row.merge_commit {
        json.object_start();
        json.key(b"oid");
        json.string(&id::write_commit(&commit).ok_or(Overflow)?);
        json.object_end();
    } else {
        json.null();
    }
    json.key(b"mergeQueueEntry");
    if let Some(position) = row.queued {
        json.object_start();
        json.key(b"position");
        json.unsigned(u64::from(position));
        json.object_end();
    } else {
        json.null();
    }
    json.key(b"timelineItems");
    json.object_start();
    json.key(b"nodes");
    json.array_start();
    if let Some(reason) = &row.dequeued {
        json.object_start();
        json.key(b"reason");
        json.string(reason);
        json.object_end();
    }
    json.array_end();
    json.object_end();
    json.object_end();
    json.object_end();
    json.object_end();
    json.object_end();
    Ok(())
}
