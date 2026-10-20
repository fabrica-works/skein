//! Pull and CI documents for the fake, measured before writing.
use super::{document, id, output_limits};
use crate::{api, forgejo::time};
use skein_json::writer::Encoder;
use skein_lib::{Overflow, Writer};

#[derive(Debug)]
enum Answer<'a> {
    Pull(&'a document::Pull),
    Pulls(&'a [document::Pull]),
    Combined(&'a document::Combined, &'a [document::Context]),
    Runs(&'a [document::Run]),
}
/// Writes one pull including its merged flag.
pub fn pull(row: &document::Pull, out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Pull(row), out)
}
/// Writes a pull listing, which omits the merged flag.
pub fn pulls(rows: &[document::Pull], out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Pulls(rows), out)
}
/// Writes contexts followed by the combined envelope's total.
pub fn combined(row: &document::Combined, contexts: &[document::Context], out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Combined(row, contexts), out)
}
/// Writes the minimal check run envelope used by the CI fold.
pub fn runs(rows: &[document::Run], out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Runs(rows), out)
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
                json.object_end();
            }
            json.array_end();
            json.object_end();
        }
    }
    Ok(())
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
    json.key(b"merge_commit_sha");
    match &row.merge_commit {
        Some(commit) => json.string(&id::write_commit(commit).ok_or(Overflow)?),
        None => json.null(),
    }
    json.key(b"mergeable");
    json.boolean(row.mergeable);
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
