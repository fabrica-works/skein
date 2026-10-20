//! One signed GitHub payload and its forge hint (forge-github.md, section 9).
pub use super::decode::delivery::{Decoder, Step, decode, decoder, first, step};
use super::limits::Limits;
use crate::{api, forgejo::document::Body, marker};
use alloc::boxed::Box;
use skein_lib::{
    bytes,
    sha256::{self, Hmac},
};

pub const EVENT_HEADER: &[u8] = b"X-GitHub-Event";
pub const GUID_HEADER: &[u8] = b"X-GitHub-Delivery";
pub const SIGNATURE_HEADER: &[u8] = b"X-Hub-Signature-256";

/// Reads exactly `sha256=` and 64 lowercase hexadecimal digits.
#[must_use]
pub fn signature(header: &[u8]) -> Option<[u8; 32]> {
    let text = header.strip_prefix(b"sha256=")?;
    if text.len() != 64 {
        return None;
    }
    let mut out = [0; 32];
    for (pair, slot) in text.chunks_exact(2).zip(&mut out) {
        *slot = digit(*pair.first()?)?.checked_mul(16)?.checked_add(digit(*pair.get(1)?)?)?;
    }
    Some(out)
}
fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => byte.checked_sub(b'0'),
        b'a'..=b'f' => byte.checked_sub(b'a')?.checked_add(10),
        _ => None,
    }
}
/// The fake's header value; the component can hash the same body in pieces.
#[must_use]
pub fn sign(secret: &[u8], body: &[u8]) -> [u8; 71] {
    let mut mac = Hmac::new(secret);
    mac.update(body);
    let mut out = [0; 71];
    for (slot, byte) in out.iter_mut().zip(b"sha256=") {
        *slot = *byte;
    }
    for (pair, byte) in out.get_mut(7..).expect("signature prefix").chunks_exact_mut(2).zip(mac.finish()) {
        for (slot, nibble) in pair.iter_mut().zip([byte / 16, byte % 16]) {
            *slot = marker::hex(nibble);
        }
    }
    out
}
/// Compares the body MAC in constant time after checking the header's spelling.
#[must_use]
pub fn verify(secret: &[u8], body: &[u8], header: &[u8]) -> bool {
    let Some(expected) = signature(header) else {
        return false;
    };
    let mut mac = Hmac::new(secret);
    mac.update(body);
    sha256::equal(&mac.finish(), &expected)
}

/// GitHub's repository path limits, in bytes.
pub const OWNER_BYTES: u32 = 39;
pub const NAME_BYTES: u32 = 100;

/// The payload's repository path, when both names are present.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Path {
    pub owner: Box<[u8]>,
    pub name: Box<[u8]>,
}

/// A pull action is retained whole within `string_bytes`; only dequeued has a reason.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Pull {
    pub action: Box<[u8]>,
    pub reason: Option<Box<[u8]>>,
}
/// The CI event and, for merge groups, its action. Status state does not affect a hint.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Ci {
    CheckRun,
    CheckSuite,
    Status,
    WorkflowJob,
    WorkflowRun,
    ChecksRequested,
    Destroyed,
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Account {
    pub login: Box<[u8]>,
    pub id: u64,
}
/// Added and removed both ask the owner to read the repositories again.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Change {
    Created,
    Deleted,
    Suspended,
    Unsuspended,
    Repositories,
    Renamed { login: Box<[u8]> },
}
/// Retained fields; action details with identical hints are discarded except on pulls and CI.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Delivery {
    Issue { repository: u64, number: u64, body: Body },
    Comment { repository: u64, number: u64, body: Body },
    Pull { repository: u64, number: u64, action: Pull },
    Review { repository: u64, number: u64, body: Body },
    Remark { repository: u64, number: u64 },
    Push { repository: u64, branch: Box<[u8]>, after: api::Commit, deleted: bool, sender: u64 },
    Ref { repository: u64, branch: Box<[u8]>, created: bool },
    Ci { repository: u64, commit: api::Commit, event: Ci },
    Installation { installation: u64, account: Account, change: Change },
    Other,
}
/// The key belongs to the retained body; an oversized repository never hints.
#[must_use]
pub fn hint(forge: u16, word: &[u8], delivery: &Delivery) -> Option<api::Hint> {
    let (repository, change, body) = match delivery {
        Delivery::Issue { repository, number, body }
        | Delivery::Comment { repository, number, body }
        | Delivery::Review { repository, number, body } => (*repository, api::Change::Item(*number), Some(body)),
        Delivery::Pull { repository, number, .. } | Delivery::Remark { repository, number } => {
            (*repository, api::Change::Item(*number), None)
        }
        Delivery::Push { repository, branch, after, deleted, sender } => {
            let branch = bytes::copy_of(branch);
            let change = if *deleted {
                api::Change::Branch(branch)
            } else {
                api::Change::BranchMoved { branch, head: *after, actor: *sender }
            };
            (*repository, change, None)
        }
        Delivery::Ref { repository, branch, .. } => (*repository, api::Change::Branch(bytes::copy_of(branch)), None),
        Delivery::Ci { repository, commit, .. } => (*repository, api::Change::Commit(*commit), None),
        Delivery::Installation { .. } | Delivery::Other => return None,
    };
    let key = match body {
        Some(Body::Kept(text)) => marker::decode(word, text),
        Some(Body::Long(_)) | None => None,
    };
    Some(api::Hint { repository: api::Repository { forge, repository: u32::try_from(repository).ok()? }, change, key })
}
/// Recognised event roots; every other header bypasses JSON decoding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Event {
    Issue,
    Comment,
    Pull,
    Review,
    Remark,
    Push,
    Ref(bool),
    Ci(Ci),
    Installation,
    Repositories,
    Target,
}
pub(crate) fn event(header: &[u8]) -> Option<Event> {
    Some(match header {
        b"issues" => Event::Issue,
        b"issue_comment" => Event::Comment,
        b"pull_request" => Event::Pull,
        b"pull_request_review" => Event::Review,
        b"pull_request_review_comment" => Event::Remark,
        b"push" => Event::Push,
        b"create" => Event::Ref(true),
        b"delete" => Event::Ref(false),
        b"check_run" => Event::Ci(Ci::CheckRun),
        b"check_suite" => Event::Ci(Ci::CheckSuite),
        b"status" => Event::Ci(Ci::Status),
        b"workflow_job" => Event::Ci(Ci::WorkflowJob),
        b"workflow_run" => Event::Ci(Ci::WorkflowRun),
        b"merge_group" => Event::Ci(Ci::ChecksRequested),
        b"installation" => Event::Installation,
        b"installation_repositories" => Event::Repositories,
        b"installation_target" => Event::Target,
        _ => return None,
    })
}
pub(crate) fn pull_action(action: &[u8]) -> bool {
    // The recognised pull actions are kept as bounded text; unknown actions have no hint.
    const ACTIONS: &[&[u8]] = &[
        b"assigned",
        b"unassigned",
        b"labeled",
        b"unlabeled",
        b"opened",
        b"edited",
        b"closed",
        b"reopened",
        b"synchronize",
        b"converted_to_draft",
        b"locked",
        b"unlocked",
        b"enqueued",
        b"dequeued",
        b"milestoned",
        b"demilestoned",
        b"ready_for_review",
        b"review_requested",
        b"review_request_removed",
        b"auto_merge_enabled",
        b"auto_merge_disabled",
    ];
    ACTIONS.contains(&action)
}
/// A retained string's bound, shared by the encoder.
pub(crate) fn text_fits(text: &[u8], limits: &Limits) -> bool {
    text.len() <= usize::try_from(limits.string_bytes.min(limits.json.string)).expect("u32 fits usize")
}
