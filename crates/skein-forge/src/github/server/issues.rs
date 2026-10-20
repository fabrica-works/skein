//! Issue documents and comments share GitHub's issue numbering.
use super::{document, output_limits};
use crate::{
    api,
    forgejo::{document::Body, time},
};
use skein_json::writer::Encoder;
use skein_lib::{Overflow, Writer};

#[derive(Debug)]
enum Answer<'a> {
    Issue(&'a document::Issue),
    Issues(&'a [document::Issue]),
    Comments(&'a [document::Comment]),
}
/// Writes one issue, including a pull's marker when present.
pub fn issue(row: &document::Issue, out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Issue(row), out)
}
/// Writes a page before the client filters issues from pulls.
pub fn issues(rows: &[document::Issue], out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Issues(rows), out)
}
/// Writes issue comments with creation and revision times.
pub fn comments(rows: &[document::Comment], out: &mut Writer) -> Result<(), Overflow> {
    encode(&Answer::Comments(rows), out)
}
fn encode(answer: &Answer<'_>, out: &mut Writer) -> Result<(), Overflow> {
    let limits = output_limits(out)?;
    let mut measure = Encoder::measure(&limits);
    json(answer, &mut measure)?;
    let length = measure.measured().or(Err(Overflow))?;
    let mut encoder = Encoder::write(length, &limits);
    json(answer, &mut encoder)?;
    out.put(&encoder.finish())
}
fn text(body: &Body, json: &mut Encoder) -> Result<(), Overflow> {
    match body {
        Body::Kept(bytes) => json.string(bytes),
        Body::Long(_) => return Err(Overflow),
    }
    Ok(())
}
fn json(answer: &Answer<'_>, json: &mut Encoder) -> Result<(), Overflow> {
    match answer {
        Answer::Issue(row) => issue_json(row, json)?,
        Answer::Issues(rows) => {
            json.array_start();
            for row in *rows {
                issue_json(row, json)?;
            }
            json.array_end();
        }
        Answer::Comments(rows) => {
            json.array_start();
            for row in *rows {
                json.object_start();
                json.key(b"id");
                json.unsigned(row.id);
                user(row.author, json);
                if let Some(at) = row.created_at {
                    json.key(b"created_at");
                    json.string(&time::write(at));
                }
                if let Some(at) = row.updated_at {
                    json.key(b"updated_at");
                    json.string(&time::write(at));
                }
                json.key(b"body");
                text(&row.body, json)?;
                json.object_end();
            }
            json.array_end();
        }
    }
    Ok(())
}
fn user(id: u64, json: &mut Encoder) {
    json.key(b"user");
    json.object_start();
    json.key(b"id");
    json.unsigned(id);
    json.object_end();
}
fn issue_json(row: &document::Issue, json: &mut Encoder) -> Result<(), Overflow> {
    json.object_start();
    json.key(b"number");
    json.unsigned(row.number);
    json.key(b"state");
    json.string(match row.state {
        api::State::Open => b"open",
        api::State::Closed => b"closed",
    });
    user(row.author, json);
    json.key(b"labels");
    json.array_start();
    for label in &row.labels {
        json.object_start();
        json.key(b"name");
        json.string(label);
        json.object_end();
    }
    json.array_end();
    json.key(b"updated_at");
    json.string(&time::write(row.updated_at));
    json.key(b"title");
    json.string(&row.title);
    json.key(b"body");
    text(&row.body, json)?;
    if row.pull {
        json.key(b"pull_request");
        json.object_start();
        json.key(b"merged_at");
        if row.merged {
            json.string(&time::write(row.updated_at));
        } else {
            json.null();
        }
        json.object_end();
    }
    json.object_end();
    Ok(())
}
