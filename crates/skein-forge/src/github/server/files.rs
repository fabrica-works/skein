//! Sized comparison envelopes and streamed-list documents.
use super::{document, output_limits};
use crate::api;
use skein_json::writer::Encoder;
use skein_lib::{Overflow, Writer};
/// Writes comparison metadata, commits and filenames.
pub fn compare(
    row: &document::Compare,
    commits: &[api::Commit],
    files: &[document::File],
    out: &mut Writer,
) -> Result<(), Overflow> {
    let limits = output_limits(out)?;
    let mut measure = Encoder::measure(&limits);
    compare_json(row, commits, files, &mut measure)?;
    let Ok(length) = measure.measured() else {
        return Err(Overflow);
    };
    let mut json = Encoder::write(length, &limits);
    compare_json(row, commits, files, &mut json)?;
    out.put(&json.finish())
}
fn compare_json(
    row: &document::Compare,
    commits: &[api::Commit],
    files: &[document::File],
    json: &mut Encoder,
) -> Result<(), Overflow> {
    json.object_start();
    json.key(b"total_commits");
    json.unsigned(row.total_commits);
    json.key(b"behind_by");
    json.unsigned(row.behind_by);
    if let Some(base) = row.merge_base {
        json.key(b"merge_base_commit");
        commit_json(&base, json)?;
    }
    json.key(b"commits");
    json.array_start();
    for commit in commits {
        commit_json(commit, json)?;
    }
    json.array_end();
    json.key(b"files");
    json.array_start();
    for file in files {
        json.object_start();
        json.key(b"filename");
        json.string(&file.path);
        json.object_end();
    }
    json.array_end();
    json.object_end();
    Ok(())
}
fn commit_json(commit: &api::Commit, json: &mut Encoder) -> Result<(), Overflow> {
    json.object_start();
    json.key(b"sha");
    json.string(&crate::github::id::write_commit(commit).ok_or(Overflow)?);
    json.object_end();
    Ok(())
}

/// Writes one page of pull file descriptors.
pub fn file_rows(rows: &[document::File], out: &mut Writer) -> Result<(), Overflow> {
    let limits = output_limits(out)?;
    let mut measure = Encoder::measure(&limits);
    file_rows_json(rows, &mut measure);
    let Ok(length) = measure.measured() else {
        return Err(Overflow);
    };
    let mut json = Encoder::write(length, &limits);
    file_rows_json(rows, &mut json);
    out.put(&json.finish())
}
fn file_rows_json(rows: &[document::File], json: &mut Encoder) {
    json.array_start();
    for row in rows {
        json.object_start();
        json.key(b"filename");
        json.string(&row.path);
        json.key(b"status");
        json.string(status(row.status));
        if let Some(previous) = &row.previous {
            json.key(b"previous_filename");
            json.string(previous);
        }
        json.object_end();
    }
    json.array_end();
}
fn status(status: document::FileStatus) -> &'static [u8] {
    match status {
        document::FileStatus::Added => b"added",
        document::FileStatus::Removed => b"removed",
        document::FileStatus::Renamed => b"renamed",
        document::FileStatus::Modified => b"modified",
        document::FileStatus::Changed => b"changed",
        document::FileStatus::Copied => b"copied",
        document::FileStatus::Unchanged => b"unchanged",
    }
}
