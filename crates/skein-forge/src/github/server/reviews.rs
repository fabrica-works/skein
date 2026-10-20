//! Sized review comment listing answers.
use super::{document, output_limits};
use skein_json::writer::Encoder;
use skein_lib::{Overflow, Writer};
/// Writes review comments with their selected line.
pub fn remarks(rows: &[document::Remark], out: &mut Writer) -> Result<(), Overflow> {
    let limits = output_limits(out)?;
    let mut measure = Encoder::measure(&limits);
    remarks_json(rows, &mut measure)?;
    let Ok(length) = measure.measured() else {
        return Err(Overflow);
    };
    let mut json = Encoder::write(length, &limits);
    remarks_json(rows, &mut json)?;
    out.put(&json.finish())
}
fn remarks_json(rows: &[document::Remark], json: &mut Encoder) -> Result<(), Overflow> {
    json.array_start();
    for row in rows {
        json.object_start();
        json.key(b"id");
        json.unsigned(row.id);
        json.key(b"user");
        json.object_start();
        json.key(b"id");
        json.unsigned(row.author);
        json.object_end();
        json.key(b"path");
        json.string(&row.path);
        json.key(b"line");
        json.unsigned(u64::from(row.line));
        json.key(b"body");
        match &row.body {
            crate::forgejo::document::Body::Kept(body) => json.string(body),
            crate::forgejo::document::Body::Long(_) => return Err(Overflow),
        }
        json.object_end();
    }
    json.array_end();
    Ok(())
}

/// Writes review states, submission times and associations.
pub fn reviews(rows: &[document::Review], out: &mut Writer) -> Result<(), Overflow> {
    let limits = output_limits(out)?;
    let mut measure = Encoder::measure(&limits);
    reviews_json(rows, &mut measure)?;
    let Ok(length) = measure.measured() else {
        return Err(Overflow);
    };
    let mut json = Encoder::write(length, &limits);
    reviews_json(rows, &mut json)?;
    out.put(&json.finish())
}
fn reviews_json(rows: &[document::Review], json: &mut Encoder) -> Result<(), Overflow> {
    json.array_start();
    for row in rows {
        json.object_start();
        json.key(b"id");
        json.unsigned(row.id);
        json.key(b"user");
        json.object_start();
        json.key(b"id");
        json.unsigned(row.author);
        json.object_end();
        json.key(b"state");
        json.string(match row.verdict {
            Some(crate::api::Verdict::Approve) => b"APPROVED",
            Some(crate::api::Verdict::RequestChanges) => b"CHANGES_REQUESTED",
            Some(crate::api::Verdict::Comment) => b"COMMENTED",
            None => b"PENDING",
        });
        json.key(b"commit_id");
        json.string(&crate::github::id::write_commit(&row.commit).ok_or(Overflow)?);
        json.key(b"body");
        match &row.body {
            crate::forgejo::document::Body::Kept(body) => json.string(body),
            crate::forgejo::document::Body::Long(_) => return Err(Overflow),
        }
        json.key(b"submitted_at");
        match row.submitted {
            Some(at) => json.string(&crate::forgejo::time::write(at)),
            None => json.null(),
        }
        json.key(b"author_association");
        json.string(if row.official { b"COLLABORATOR" } else { b"NONE" });
        json.object_end();
    }
    json.array_end();
    Ok(())
}
