//! Delivery-log documents use the pushed payload encoder (forge-github.md, 9.3).
use super::{delivery, output_limits};
use crate::{
    forgejo::time,
    github::{
        delivery::Delivery,
        guid,
        limits::{DEPTH, Limits},
        log::Row,
    },
};
use skein_json::{tokenizer, writer::Encoder};
use skein_lib::{Overflow, Writer};

/// Writes a newest-first page supplied by the fake, preserving every id digit.
pub fn deliveries(rows: &[Row], out: &mut Writer) -> Result<(), Overflow> {
    let limits = output_limits(out)?;
    let mut measure = Encoder::measure(&limits);
    list(rows, &mut measure);
    let length = measure.measured().or(Err(Overflow))?;
    let mut json = Encoder::write(length, &limits);
    list(rows, &mut json);
    out.put(&json.finish())
}
fn list(rows: &[Row], json: &mut Encoder) {
    json.array_start();
    for row in rows {
        fields(row, json, false);
    }
    json.array_end();
}
fn fields(row: &Row, json: &mut Encoder, open: bool) {
    json.object_start();
    json.key(b"id");
    json.unsigned(row.id);
    json.key(b"guid");
    json.string(&guid::write(&row.guid));
    json.key(b"event");
    json.string(&row.event);
    json.key(b"action");
    match &row.action {
        Some(action) => json.string(action),
        None => json.null(),
    }
    optional(json, b"installation_id", row.installation_id);
    optional(json, b"repository_id", row.repository_id);
    json.key(b"status_code");
    json.unsigned(u64::from(row.status_code));
    json.key(b"delivered_at");
    json.string(&time::write(row.delivered_at));
    json.key(b"redelivery");
    json.boolean(row.redelivery);
    if !open {
        json.object_end();
    }
}
fn optional(json: &mut Encoder, key: &[u8], value: Option<u64>) {
    json.key(key);
    match value {
        Some(value) => json.unsigned(value),
        None => json.null(),
    }
}
/// Writes a trusted log envelope; its payload is encoded anew, not copied as JSON text.
pub fn logged(row: &Row, payload: &Delivery, signature: &[u8], out: &mut Writer) -> Result<(), Overflow> {
    let writer = output_limits(out)?;
    let limits = Limits {
        writer,
        json: tokenizer::Limits { depth: DEPTH, string: writer.length, number: 32, chunk: 128, length: writer.length },
        string_bytes: writer.length,
        body_bytes: writer.length,
        inner: writer.length,
        target_bytes: writer.length,
    };
    let mut measure = Encoder::measure(&writer);
    whole(row, payload, signature, &limits, &mut measure)?;
    let length = measure.measured().or(Err(Overflow))?;
    let mut json = Encoder::write(length, &writer);
    whole(row, payload, signature, &limits, &mut json)?;
    out.put(&json.finish())
}
fn whole(row: &Row, payload: &Delivery, signature: &[u8], limits: &Limits, json: &mut Encoder) -> Result<(), Overflow> {
    fields(row, json, true);
    json.key(b"request");
    json.object_start();
    json.key(b"headers");
    json.object_start();
    json.key(b"X-GitHub-Event");
    json.string(&row.event);
    json.key(b"X-GitHub-Delivery");
    json.string(&guid::write(&row.guid));
    json.key(b"X-Hub-Signature-256");
    json.string(signature);
    json.object_end();
    json.key(b"payload");
    let event = delivery::write(payload, limits, json).or(Err(Overflow))?;
    if event != row.event.as_ref() {
        return Err(Overflow);
    }
    json.object_end();
    json.key(b"response");
    json.object_start();
    json.key(b"status_code");
    json.unsigned(u64::from(row.status_code));
    json.key(b"headers");
    json.object_start();
    json.object_end();
    json.key(b"payload");
    json.string(b"");
    json.object_end();
    json.object_end();
    Ok(())
}
