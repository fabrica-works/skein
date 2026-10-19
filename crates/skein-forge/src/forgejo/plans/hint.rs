//! Verified delivery fields mapped to the forge client's hints (forge.md, 5).
use crate::api;
use crate::forgejo::{delivery::Delivery, document::Body};
use crate::marker;
use skein_lib::bytes;

#[must_use]
pub fn hint(forge: u16, word: &[u8], delivery: &Delivery) -> Option<api::Hint> {
    let (repository, change, body) = match delivery {
        Delivery::Issue { repository, number, body }
        | Delivery::Comment { repository, number, body }
        | Delivery::Review { repository, number, body, .. } => (*repository, api::Change::Item(*number), Some(body)),
        Delivery::Pull { repository, number } => (*repository, api::Change::Item(*number), None),
        Delivery::Push { repository, after, pusher, .. } => {
            let branch = bytes::copy_of(delivery.branch()?);
            let change = if delivery.deleted() {
                api::Change::Branch(branch)
            } else {
                api::Change::BranchMoved { branch, head: after.bytes, actor: *pusher }
            };
            (*repository, change, None)
        }
        Delivery::Ref { repository, .. } => {
            (*repository, api::Change::Branch(bytes::copy_of(delivery.branch()?)), None)
        }
        Delivery::Job { repository, commit, .. } => (*repository, api::Change::Commit(commit.bytes), None),
    };
    let key = match body {
        Some(Body::Kept(text)) => marker::decode(word, text),
        Some(Body::Long(_)) | None => None,
    };
    Some(api::Hint { repository: api::Repository { forge, repository: u32::try_from(repository).ok()? }, change, key })
}
