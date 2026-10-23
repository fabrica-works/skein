//! Repository roles shared by permission and collaborator documents.
use super::Malformed;
use crate::api;

pub(super) fn permission(word: &[u8]) -> Result<api::Permission, Malformed> {
    match word {
        b"admin" | b"maintain" => Ok(api::Permission::Admin),
        b"write" => Ok(api::Permission::Write),
        b"triage" | b"read" => Ok(api::Permission::Read),
        b"" | b"none" => Ok(api::Permission::None),
        _ => Err(Malformed::Value { field: b"role_name" }),
    }
}

pub(super) fn collaborator() -> crate::github::document::Collaborator {
    crate::github::document::Collaborator { id: 0, permission: api::Permission::None, login: None }
}
