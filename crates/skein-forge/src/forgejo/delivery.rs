//! The fields carried by the webhook hints (forge.md, section 5).
use crate::forgejo::{document::Body, id::ObjectId, request::ReviewEvent};
use alloc::boxed::Box;
use skein_lib::{Decimal, Writer};
/// A job's end: the `workflow_job_<end>` event.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum JobEnd {
    Success,
    Failure,
    Cancelled,
    Skipped,
}
/// A ref's `ref_type`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RefType {
    Branch,
    Tag,
}
/// Fields decoded for the engine and encoded by the fake forge.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Delivery {
    /// `issues`: `number`, `issue.body`.
    Issue { repository: u64, number: u64, body: Body },
    /// `pull_request`: `number`; its key is its branches.
    Pull { repository: u64, number: u64 },
    /// `issue_comment`: `issue.number`, `comment.body`.
    Comment { repository: u64, number: u64, body: Body },
    /// A review: `number`, `review.content`; `event` says which.
    Review { event: ReviewEvent, repository: u64, number: u64, body: Body },
    /// `push`: the full `ref`, the new head, the pusher's id.
    Push { repository: u64, reference: Box<[u8]>, after: ObjectId, pusher: u64 },
    /// `create` (`created`) or `delete`: `ref`, `ref_type`.
    Ref { created: bool, repository: u64, reference: Box<[u8]>, ref_type: RefType },
    /// A job's end: `run.repository.id`, `run.commit_sha`.
    Job { end: JobEnd, repository: u64, commit: ObjectId },
}
impl Delivery {
    /// The branch a push, create or delete names; none for tags or other events.
    #[must_use]
    pub fn branch(&self) -> Option<&[u8]> {
        match self {
            Self::Push { reference, .. } | Self::Ref { reference, ref_type: RefType::Branch, .. } => {
                reference.strip_prefix(b"refs/heads/")
            }
            Self::Issue { .. }
            | Self::Pull { .. }
            | Self::Comment { .. }
            | Self::Review { .. }
            | Self::Ref { ref_type: RefType::Tag, .. }
            | Self::Job { .. } => None,
        }
    }
    /// A branch deletion's push has an all-zero `after` (forge.md,
    /// section 5).
    #[must_use]
    pub fn deleted(&self) -> bool {
        match self {
            Self::Push { after, .. } => after.bytes == [0; 32],
            Self::Issue { .. }
            | Self::Pull { .. }
            | Self::Comment { .. }
            | Self::Review { .. }
            | Self::Ref { .. }
            | Self::Job { .. } => false,
        }
    }
}
/// The org hook's subscribed events (forge.md, section 5).
pub const SUBSCRIBED: [&[u8]; 22] = [
    b"issues",
    b"issue_assign",
    b"issue_label",
    b"issue_milestone",
    b"pull_request",
    b"pull_request_assign",
    b"pull_request_label",
    b"pull_request_milestone",
    b"pull_request_sync",
    b"pull_request_review_request",
    b"issue_comment",
    b"pull_request_comment",
    b"pull_request_review_approved",
    b"pull_request_review_rejected",
    b"pull_request_review_comment",
    b"push",
    b"create",
    b"delete",
    b"workflow_job_success",
    b"workflow_job_failure",
    b"workflow_job_cancelled",
    b"workflow_job_skipped",
];
/// The header naming the delivery family.
pub const EVENT_HEADER: &[u8] = b"X-Forgejo-Event";
/// The header holding the body's HMAC-SHA256 in hexadecimal.
pub const SIGNATURE_HEADER: &[u8] = b"X-Forgejo-Signature";
/// The listener path for a forge's deliveries (forge.md, section 5).
#[must_use]
pub fn hook_target(forge: u16) -> Box<[u8]> {
    let number = Decimal::of(u64::from(forge));
    let mut out = Writer::new(7usize.saturating_add(number.as_bytes().len()));
    out.put(b"/hooks/").expect("prefix fits");
    out.put(number.as_bytes()).expect("u16 fits");
    out.finish()
}
/// The forge named by exactly one canonical listener path.
#[must_use]
pub fn hook_forge(target: &[u8]) -> Option<u16> {
    let text = target.strip_prefix(b"/hooks/")?;
    let number = crate::forgejo::id::unsigned(text)?;
    (Decimal::of(number).as_bytes() == text).then_some(())?;
    u16::try_from(number).ok()
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Event {
    Issue,
    Pull,
    Comment,
    Review(ReviewEvent),
    Push,
    Ref(bool),
    Job(JobEnd),
}
const EVENTS: [(&[u8], Event); 13] = [
    (b"issues", Event::Issue),
    (b"pull_request", Event::Pull),
    (b"issue_comment", Event::Comment),
    (b"pull_request_approved", Event::Review(ReviewEvent::Approved)),
    (b"pull_request_rejected", Event::Review(ReviewEvent::RequestChanges)),
    (b"pull_request_comment", Event::Review(ReviewEvent::Comment)),
    (b"push", Event::Push),
    (b"create", Event::Ref(true)),
    (b"delete", Event::Ref(false)),
    (b"workflow_job_success", Event::Job(JobEnd::Success)),
    (b"workflow_job_failure", Event::Job(JobEnd::Failure)),
    (b"workflow_job_cancelled", Event::Job(JobEnd::Cancelled)),
    (b"workflow_job_skipped", Event::Job(JobEnd::Skipped)),
];
pub(crate) fn event(header: &[u8]) -> Option<Event> {
    for (name, event) in EVENTS {
        if name == header {
            return Some(event);
        }
    }
    None
}
pub(crate) fn header(delivery: &Delivery) -> &'static [u8] {
    let event = match delivery {
        Delivery::Issue { .. } => Event::Issue,
        Delivery::Pull { .. } => Event::Pull,
        Delivery::Comment { .. } => Event::Comment,
        Delivery::Review { event, .. } => Event::Review(*event),
        Delivery::Push { .. } => Event::Push,
        Delivery::Ref { created, .. } => Event::Ref(*created),
        Delivery::Job { end, .. } => Event::Job(*end),
    };
    for (header, value) in EVENTS {
        if value == event {
            return header;
        }
    }
    unreachable!("every variant has a header")
}

/// Subscriptions in the row of forge.md, section 7, for this delivery header.
/// Events in one row all take the same hint, including the fake's over-approximation.
#[must_use]
pub fn subscriptions(header: &[u8]) -> Option<&'static [&'static [u8]]> {
    let row = match event(header)? {
        Event::Issue => 0..4,
        Event::Pull => 4..10,
        Event::Comment => 10..12,
        Event::Review(ReviewEvent::Approved | ReviewEvent::RequestChanges | ReviewEvent::Comment) => 12..15,
        Event::Push => 15..16,
        Event::Ref(true | false) => 16..18,
        Event::Job(JobEnd::Success | JobEnd::Failure | JobEnd::Cancelled | JobEnd::Skipped) => 18..22,
    };
    SUBSCRIBED.get(row)
}
