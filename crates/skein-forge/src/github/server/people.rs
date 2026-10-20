//! Bounded user and permission answers, measured before writing.
use super::{document, output_limits};
use crate::api;
use skein_json::writer::Encoder;
use skein_lib::{Overflow, Writer};

/// Writes the identity used to fill the users table.
pub fn user(row: &document::User, out: &mut Writer) -> Result<(), Overflow> {
    encode(Some(row), api::Permission::None, out)
}
/// Writes a canonical role name for a repository permission.
pub fn permission(value: api::Permission, out: &mut Writer) -> Result<(), Overflow> {
    encode(None, value, out)
}
fn encode(row: Option<&document::User>, value: api::Permission, out: &mut Writer) -> Result<(), Overflow> {
    let limits = output_limits(out)?;
    let mut measure = Encoder::measure(&limits);
    json(row, value, &mut measure);
    let Ok(length) = measure.measured() else {
        return Err(Overflow);
    };
    let mut encoder = Encoder::write(length, &limits);
    json(row, value, &mut encoder);
    out.put(&encoder.finish())
}
fn json(row: Option<&document::User>, value: api::Permission, json: &mut Encoder) {
    json.object_start();
    if let Some(row) = row {
        json.key(b"id");
        json.unsigned(row.id);
        json.key(b"login");
        json.string(&row.login);
    } else {
        json.key(b"role_name");
        json.string(match value {
            api::Permission::None => b"none",
            api::Permission::Read => b"read",
            api::Permission::Write => b"write",
            api::Permission::Admin => b"admin",
        });
    }
    json.object_end();
}
