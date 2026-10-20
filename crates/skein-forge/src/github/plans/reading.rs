//! GitHub's issue documents in the neutral vocabulary (forge.md, 5.3).
use crate::{api, forgejo::document::Body, github::document};
use alloc::boxed::Box;
use skein_lib::{Time, Wall};

fn body(word: &[u8], value: Body) -> (Option<Box<[u8]>>, Box<[u8]>) {
    match value {
        Body::Kept(bytes) => (crate::marker::decode(word, &bytes), bytes),
        Body::Long(_) => (None, Box::from([])),
    }
}
pub(super) fn summary(word: &[u8], issue: document::Issue) -> api::Summary {
    let (key, body) = body(word, issue.body);
    api::Summary {
        labels: issue.labels,
        number: issue.number,
        kind: if issue.pull { api::Kind::Pull } else { api::Kind::Issue },
        state: if issue.merged { api::State::Closed } else { issue.state },
        author: issue.author,
        key,
        title: issue.title,
        body,
        updated: Time::from_nanos(issue.updated_at.as_nanos()),
    }
}
fn revision(body: &Body, updated: Option<Wall>) -> Result<u64, api::Error> {
    let digest = match body {
        Body::Kept(bytes) => skein_lib::sha256::digest(bytes),
        Body::Long(length) => {
            let nanos = match updated {
                Some(wall) => wall.as_nanos(),
                None => 0,
            };
            let mut hash = skein_lib::sha256::Sha256::new();
            hash.update(&length.to_be_bytes());
            hash.update(&nanos.to_be_bytes());
            hash.finish()
        }
    };
    let bytes = digest.get(..8).ok_or(api::Error::InvalidAnswer)?;
    let Ok(bytes) = bytes.try_into() else {
        return Err(api::Error::InvalidAnswer);
    };
    Ok(u64::from_be_bytes(bytes))
}
pub(super) fn comment(word: &[u8], value: document::Comment) -> Result<api::Comment, api::Error> {
    let provenance = match &value.body {
        Body::Long(_) => api::Provenance::Unknown,
        Body::Kept(_) => match (value.created_at, value.updated_at) {
            (Some(created), Some(updated)) if updated == created => api::Provenance::Original,
            (Some(created), Some(updated)) if updated > created => api::Provenance::Revised,
            (Some(_) | None, Some(_) | None) => api::Provenance::Unknown,
        },
    };
    let revision = revision(&value.body, value.updated_at)?;
    let created = match value.created_at {
        Some(wall) => Time::from_nanos(wall.as_nanos()),
        None => Time::ZERO,
    };
    let (key, body) = body(word, value.body);
    Ok(api::Comment { id: value.id, provenance, author: value.author, created, revision, key, body })
}
