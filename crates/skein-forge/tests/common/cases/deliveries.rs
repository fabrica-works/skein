#![expect(
    clippy::disallowed_types,
    reason = "the document test harness retains finite fixture rows and mutation buffers in ordinary Rust collections"
)]
use super::super::forgejo::sha;
#[must_use]
pub fn delivery_cases() -> Vec<(&'static str, skein_forge::forgejo::delivery::Delivery)> {
    use skein_forge::forgejo::{
        delivery::{Delivery, JobEnd, RefType},
        document::Body,
        request::ReviewEvent,
    };
    let mut cases = Vec::new();
    for name in ["issues", "issue_assign", "issue_label", "issue_milestone"] {
        cases.push((
            name,
            Delivery::Issue { repository: 1, number: 1, body: Body::Kept(Box::from(*b"<!-- kit:key 01 -->\n")) },
        ));
    }
    for name in [
        "pull_request",
        "pull_request_assign",
        "pull_request_label",
        "pull_request_milestone",
        "pull_request_sync",
        "pull_request_review_request",
    ] {
        cases.push((name, Delivery::Pull { repository: 1, number: 2 }));
    }
    for (name, number, text) in [
        ("issue_comment", 1, b"<!-- kit:key 02 -->\n".as_slice()),
        ("pull_request_comment", 2, b"<!-- kit:key 03 -->\n"),
    ] {
        cases.push((name, Delivery::Comment { repository: 1, number, body: Body::Kept(Box::from(text)) }));
    }
    for (name, event) in [
        ("pull_request_review_approved", ReviewEvent::Approved),
        ("pull_request_review_rejected", ReviewEvent::RequestChanges),
        ("pull_request_review_comment", ReviewEvent::Comment),
    ] {
        cases.push((
            name,
            Delivery::Review {
                event,
                repository: 1,
                number: 2,
                body: Body::Kept(Box::from(*b"<!-- kit:key 04 -->\n")),
            },
        ));
    }
    cases.push((
        "push",
        Delivery::Push { repository: 1, reference: Box::from(*b"refs/heads/topic"), after: sha(13), pusher: 1 },
    ));
    for (name, created, reference, ref_type) in [
        ("create", true, b"refs/heads/topic".as_slice(), RefType::Branch),
        ("delete", false, b"refs/heads/gone", RefType::Branch),
        ("create-tag", true, b"refs/tags/v1", RefType::Tag),
        ("delete-tag", false, b"refs/tags/v1", RefType::Tag),
    ] {
        cases.push((name, Delivery::Ref { created, repository: 1, reference: Box::from(reference), ref_type }));
    }
    for (name, end) in [
        ("workflow_job_success", JobEnd::Success),
        ("workflow_job_failure", JobEnd::Failure),
        ("workflow_job_cancelled", JobEnd::Cancelled),
        ("workflow_job_skipped", JobEnd::Skipped),
    ] {
        cases.push((name, Delivery::Job { end, repository: 1, commit: sha(48) }));
    }
    cases
}
