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
