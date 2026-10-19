use super::fixtures::{deliver, limits, small};
use crate::forgejo::{
    client::delivery,
    decode::Malformed,
    delivery::{Delivery, JobEnd, RefType, hook_forge, hook_target},
    document::Body,
    id,
    request::ReviewEvent,
    server,
};
fn body() -> Body {
    Body::Kept(Box::from(*b"marker"))
}
fn cases() -> Box<[(&'static [u8], Delivery)]> {
    let mut cases = skein_lib::List::with_capacity(13);
    for case in [
        (b"issues".as_slice(), Delivery::Issue { repository: 7, number: 3, body: body() }),
        (b"pull_request", Delivery::Pull { repository: 7, number: 3 }),
        (b"issue_comment", Delivery::Comment { repository: 7, number: 3, body: body() }),
        (
            b"push",
            Delivery::Push {
                repository: 7,
                reference: Box::from(*b"refs/heads/topic"),
                after: id::read(b"0000000000000000000000000000000000000001").unwrap(),
                pusher: 9,
            },
        ),
        (
            b"create",
            Delivery::Ref {
                created: true,
                repository: 7,
                reference: Box::from(*b"refs/heads/topic"),
                ref_type: RefType::Branch,
            },
        ),
        (
            b"delete",
            Delivery::Ref {
                created: false,
                repository: 7,
                reference: Box::from(*b"refs/tags/v1"),
                ref_type: RefType::Tag,
            },
        ),
    ] {
        cases.push(case).unwrap();
    }
    for (header, event) in [
        (b"pull_request_approved".as_slice(), ReviewEvent::Approved),
        (b"pull_request_rejected", ReviewEvent::RequestChanges),
        (b"pull_request_comment", ReviewEvent::Comment),
    ] {
        cases.push((header, Delivery::Review { event, repository: 7, number: 3, body: body() })).unwrap();
    }
    for (header, end) in [
        (b"workflow_job_success".as_slice(), JobEnd::Success),
        (b"workflow_job_failure", JobEnd::Failure),
        (b"workflow_job_cancelled", JobEnd::Cancelled),
        (b"workflow_job_skipped", JobEnd::Skipped),
    ] {
        cases
            .push((
                header,
                Delivery::Job {
                    end,
                    repository: 7,
                    commit: id::read(b"0000000000000000000000000000000000000001").unwrap(),
                },
            ))
            .unwrap();
    }
    cases.into_boxed()
}
#[test]
fn each_event_header_selects_the_delivery_it_decodes_to() {
    for (header, expected) in cases() {
        let sent = server::delivery::encode(&expected, &limits()).unwrap();
        assert_eq!(sent.event, header);
        assert_eq!(deliver(header, &sent.body, &limits()), Ok(expected));
    }
    assert!(delivery::decoder(b"unknown", &limits()).is_none());
}
#[test]
fn a_push_skips_its_commits_and_keeps_ref_after_and_pusher() {
    let json = br#"{"commits":[{"message":"ignored","nested":[null,true,{}]}],"repository":{"id":7},"ref":"refs/heads/topic","after":"0000000000000000000000000000000000000001","pusher":{"id":9}}"#;
    assert_eq!(
        deliver(b"push", json, &limits()),
        Ok(Delivery::Push {
            repository: 7,
            reference: Box::from(*b"refs/heads/topic"),
            after: id::read(b"0000000000000000000000000000000000000001").unwrap(),
            pusher: 9
        })
    );
}
#[test]
fn a_delivery_missing_a_field_is_malformed_naming_it() {
    assert_eq!(
        deliver(b"issues", br#"{"repository":{"id":7},"issue":{"body":"marker"}}"#, &limits()),
        Err(Malformed::Missing { field: b"number" })
    );
}
#[test]
fn a_repeated_repository_id_is_refused() {
    assert_eq!(
        deliver(b"pull_request", br#"{"repository":{"id":7,"id":8},"number":3}"#, &limits()),
        Err(Malformed::Shape { field: b"id" })
    );
}
#[test]
fn a_null_delivery_number_is_refused() {
    assert_eq!(
        deliver(b"pull_request", br#"{"repository":{"id":7},"number":null}"#, &limits()),
        Err(Malformed::Shape { field: b"number" })
    );
}
#[test]
fn a_text_delivery_number_is_refused() {
    assert_eq!(
        deliver(b"pull_request", br#"{"repository":{"id":7},"number":"3"}"#, &limits()),
        Err(Malformed::Shape { field: b"number" })
    );
}
#[test]
fn a_delivery_reference_past_string_bytes_is_refused() {
    assert_eq!(
        deliver(b"create", br#"{"repository":{"id":7},"ref":"123456789","ref_type":"branch"}"#, &small()),
        Err(Malformed::TooLong { field: b"ref" })
    );
}
#[test]
fn an_unknown_delivery_ref_type_is_refused() {
    assert_eq!(
        deliver(b"create", br#"{"repository":{"id":7},"ref":"topic","ref_type":"other"}"#, &limits()),
        Err(Malformed::Value { field: b"ref_type" })
    );
}
#[test]
fn a_body_past_body_bytes_reads_long_and_the_delivery_stands() {
    assert_eq!(
        deliver(b"issues", br#"{"repository":{"id":7},"number":3,"issue":{"body":"12345678901234567"}}"#, &small()),
        Ok(Delivery::Issue { repository: 7, number: 3, body: Body::Long(17) })
    );
}
#[test]
fn every_delivery_encodes_and_decodes_unchanged() {
    for (_, expected) in cases() {
        let sent = server::delivery::encode(&expected, &limits()).unwrap();
        assert_eq!(deliver(sent.event, &sent.body, &limits()), Ok(expected));
    }
}
#[test]
fn a_long_delivery_body_is_refused_by_the_encoder() {
    let value = Delivery::Issue { repository: 7, number: 3, body: Body::Long(17) };
    assert_eq!(server::delivery::encode(&value, &limits()), Err(skein_json::writer::Refusal::TooLong));
}
#[test]
fn a_hook_target_reads_back_to_the_forge_it_names() {
    for (forge, target) in [(0, b"/hooks/0".as_slice()), (1, b"/hooks/1"), (65_535, b"/hooks/65535")] {
        assert_eq!(&*hook_target(forge), target);
        assert_eq!(hook_forge(target), Some(forge));
    }
}
#[test]
fn a_target_naming_no_forge_reads_as_none() {
    for target in [
        b"/hooks/".as_slice(),
        b"/hooks/+1",
        b"/hooks/-1",
        b"/hooks/01",
        b"/hooks/65536",
        b"/hooks/1/",
        b"/hooks/1?q",
        b"/hooks/1#x",
        b"/other/1",
        b"/hooks/18446744073709551616",
    ] {
        assert_eq!(hook_forge(target), None, "{target:?}");
    }
}
fn pushed(reference: &[u8], after: &[u8]) -> Delivery {
    Delivery::Push { repository: 7, reference: Box::from(reference), after: id::read(after).unwrap(), pusher: 9 }
}
#[test]
fn branches_are_read_only_from_branch_refs() {
    for (delivery, expected) in [
        (pushed(b"refs/heads/topic", b"0000000000000000000000000000000000000000"), Some(b"topic".as_slice())),
        (pushed(b"refs/tags/v1", b"0000000000000000000000000000000000000001"), None),
        (
            Delivery::Ref {
                created: true,
                repository: 7,
                reference: Box::from(*b"refs/heads/main"),
                ref_type: RefType::Branch,
            },
            Some(b"main".as_slice()),
        ),
        (
            Delivery::Ref {
                created: false,
                repository: 7,
                reference: Box::from(*b"refs/tags/v1"),
                ref_type: RefType::Tag,
            },
            None,
        ),
        (Delivery::Pull { repository: 7, number: 3 }, None),
    ] {
        assert_eq!(delivery.branch(), expected);
    }
}
#[test]
fn only_an_all_zero_push_head_marks_a_deletion() {
    for (delivery, expected) in [
        (pushed(b"refs/heads/topic", b"0000000000000000000000000000000000000000"), true),
        (pushed(b"refs/heads/topic", b"0000000000000000000000000000000000000001"), false),
        (
            Delivery::Ref {
                created: false,
                repository: 7,
                reference: Box::from(*b"refs/heads/topic"),
                ref_type: RefType::Branch,
            },
            false,
        ),
    ] {
        assert_eq!(delivery.deleted(), expected);
    }
}
