//! The fake forge's webhook bodies; callers sign the returned bytes.
use crate::forgejo::{
    delivery::{self, Delivery, RefType},
    document::Body,
    limits::Limits,
};
use alloc::boxed::Box;
use skein_json::writer;
/// An event header and the JSON body to sign.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Sent {
    pub event: &'static [u8],
    pub body: Box<[u8]>,
}
/// Writes a delivery; a long body is refused.
pub fn encode(delivery: &Delivery, limits: &Limits) -> Result<Sent, writer::Refusal> {
    let mut measure = writer::Encoder::measure(&limits.writer);
    write(&mut measure, delivery)?;
    let mut json = writer::Encoder::write(measure.measured()?, &limits.writer);
    write(&mut json, delivery)?;
    Ok(Sent { event: delivery::header(delivery), body: json.finish() })
}
fn repository(json: &mut writer::Encoder, id: u64) {
    json.key(b"repository");
    json.object_start();
    json.key(b"id");
    json.unsigned(id);
    json.object_end();
}
fn number(json: &mut writer::Encoder, number: u64) {
    json.key(b"number");
    json.unsigned(number);
}
fn body(json: &mut writer::Encoder, object: &[u8], key: &[u8], body: &Body) -> Result<(), writer::Refusal> {
    let Body::Kept(text) = body else {
        return Err(writer::Refusal::TooLong);
    };
    json.key(object);
    json.object_start();
    json.key(key);
    json.string(text);
    json.object_end();
    Ok(())
}
fn write(json: &mut writer::Encoder, value: &Delivery) -> Result<(), writer::Refusal> {
    json.object_start();
    match value {
        Delivery::Issue { repository: id, number: n, body: text } => {
            repository(json, *id);
            number(json, *n);
            body(json, b"issue", b"body", text)?;
        }
        Delivery::Pull { repository: id, number: n } => {
            repository(json, *id);
            number(json, *n);
        }
        Delivery::Comment { repository: id, number: n, body: text } => {
            repository(json, *id);
            json.key(b"issue");
            json.object_start();
            number(json, *n);
            json.object_end();
            body(json, b"comment", b"body", text)?;
        }
        Delivery::Review { repository: id, number: n, body: text, .. } => {
            repository(json, *id);
            number(json, *n);
            body(json, b"review", b"content", text)?;
        }
        Delivery::Push { repository: id, reference, after, pusher } => {
            repository(json, *id);
            json.key(b"ref");
            json.string(reference);
            json.key(b"after");
            json.string(&crate::forgejo::id::write(after));
            json.key(b"pusher");
            json.object_start();
            json.key(b"id");
            json.unsigned(*pusher);
            json.object_end();
        }
        Delivery::Ref { repository: id, reference, ref_type, .. } => {
            repository(json, *id);
            json.key(b"ref");
            json.string(reference);
            json.key(b"ref_type");
            json.string(match ref_type {
                RefType::Branch => b"branch",
                RefType::Tag => b"tag",
            });
        }
        Delivery::Job { repository: id, commit, .. } => {
            json.key(b"run");
            json.object_start();
            repository(json, *id);
            json.key(b"commit_sha");
            json.string(&crate::forgejo::id::write(commit));
            json.object_end();
        }
    }
    json.object_end();
    Ok(())
}
