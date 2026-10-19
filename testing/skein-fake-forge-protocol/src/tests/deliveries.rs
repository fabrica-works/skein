use super::fixtures::{self, Harness, boxed};
use crate::{Service, delivery};
use skein_fake_forge_domain::{
    self as domain,
    api::{Change, Check, Ended, JobRef},
};
use skein_forge::forgejo::{
    delivery::{Delivery, JobEnd, RefType},
    document::Body,
    id::{ObjectFormat, ObjectId},
    request::ReviewEvent,
};
use skein_lib::Secret;
const REPOSITORY: u64 = 2;
const ITEM: u64 = 3;
const COMMIT_SEVEN: ObjectId = ObjectId {
    format: ObjectFormat::Sha1,
    bytes: [0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
};
fn decoded(post: &delivery::Post) -> Result<Delivery, skein_forge::forgejo::decode::Malformed> {
    fixtures::deliver(fixtures::delivery_event(post), &post.body, &fixtures::limits().documents)
}

const PUSH_BODY: &[u8] = br#"{"repository":{"id":2},"ref":"refs/heads/main","after":"0000000000000007000000000000000000000000","pusher":{"id":2}}"#;
// openssl dgst -sha256 -hmac fixture-hook-secret, over PUSH_BODY with no newline.
const PUSH_HEAD: &[u8] = b"POST /hooks/1 HTTP/1.1\r\nHost: 127.0.0.1:7200\r\nContent-Type: application/json\r\nContent-Length: 116\r\nX-Forgejo-Event: push\r\nX-Forgejo-Signature: 59d76cdeaea4277b5fe9055ba0c6f03f8c3ce07dfc2e298df9d624cce4c99df3\r\n\r\n";

fn hook(change: Change) -> domain::Request {
    domain::Request::Hook {
        repository: boxed(b"org/repo"),
        change,
        number: Some(3),
        branch: Some(boxed(b"main")),
        commit: Some(7),
        by: Some(2),
        body: match change {
            Change::Issue | Change::Comment | Change::Review => Some(boxed(b"fixture body")),
            Change::Pull | Change::Job | Change::Push => None,
        },
        ended: (change == Change::Job).then_some(Ended {
            job: JobRef { run: 4, job: 5, attempt: 1 },
            commit: 7,
            state: Check::Passed,
        }),
    }
}
fn delivered(change: Change) -> delivery::Post {
    Service::new(fixtures::hooked_config(), &fixtures::limits(), 3)
        .expect("service")
        .delivery(&hook(change), &fixtures::limits())
        .expect("configured delivery")
}
#[test]
fn a_push_hook_is_delivered_as_one_signed_post_to_the_forges_path() {
    let post = delivered(Change::Push);
    assert_eq!(&*post.body, PUSH_BODY);
    assert_eq!(&*post.head, PUSH_HEAD);
    assert_eq!(
        decoded(&post),
        Ok(Delivery::Push {
            repository: REPOSITORY,
            reference: boxed(b"refs/heads/main"),
            after: COMMIT_SEVEN,
            pusher: fixtures::PERSON
        })
    );
}
#[test]
fn an_issue_hook_is_delivered_under_the_issues_event_naming_its_number() {
    let post = delivered(Change::Issue);
    assert_eq!(&*post.body, br#"{"repository":{"id":2},"number":3,"issue":{"body":"fixture body"}}"#);
    assert_eq!(fixtures::delivery_event(&post), b"issues");
    assert_eq!(
        decoded(&post),
        Ok(Delivery::Issue { repository: REPOSITORY, number: ITEM, body: Body::Kept(boxed(b"fixture body")) })
    );
}
#[test]
fn a_jobs_end_is_delivered_naming_its_commit() {
    let post = delivered(Change::Job);
    assert_eq!(
        &*post.body,
        br#"{"run":{"repository":{"id":2},"commit_sha":"0000000000000007000000000000000000000000"}}"#
    );
    assert_eq!(fixtures::delivery_event(&post), b"workflow_job_success");
    assert_eq!(
        decoded(&post),
        Ok(Delivery::Job { end: JobEnd::Success, repository: REPOSITORY, commit: COMMIT_SEVEN })
    );
}
#[test]
fn a_failed_jobs_end_is_delivered_under_its_failure_event() {
    let service = Service::new(fixtures::hooked_config(), &fixtures::limits(), 3).expect("service");
    let mut request = hook(Change::Job);
    if let domain::Request::Hook { ended, .. } = &mut request {
        ended.as_mut().expect("job end").state = Check::Failed;
    }
    let post = service.delivery(&request, &fixtures::limits()).expect("job end");
    assert_eq!(fixtures::delivery_event(&post), b"workflow_job_failure");
    assert_eq!(
        decoded(&post),
        Ok(Delivery::Job { end: JobEnd::Failure, repository: REPOSITORY, commit: COMMIT_SEVEN })
    );
}
#[test]
fn a_comment_hook_delivers_its_body_with_the_items_number() {
    let post = delivered(Change::Comment);
    assert_eq!(&*post.body, br#"{"repository":{"id":2},"issue":{"number":3},"comment":{"body":"fixture body"}}"#);
    assert_eq!(fixtures::delivery_event(&post), b"issue_comment");
    assert_eq!(
        decoded(&post),
        Ok(Delivery::Comment { repository: REPOSITORY, number: ITEM, body: Body::Kept(boxed(b"fixture body")) })
    );
}
#[test]
fn a_review_hook_delivers_its_content_with_the_items_number() {
    let post = delivered(Change::Review);
    assert_eq!(&*post.body, br#"{"repository":{"id":2},"number":3,"review":{"content":"fixture body"}}"#);
    assert_eq!(fixtures::delivery_event(&post), b"pull_request_approved");
    assert_eq!(
        decoded(&post),
        Ok(Delivery::Review {
            event: ReviewEvent::Approved,
            repository: REPOSITORY,
            number: ITEM,
            body: Body::Kept(boxed(b"fixture body"))
        })
    );
}
#[test]
fn a_deleted_branch_is_delivered_as_a_delete_event() {
    let service = Service::new(fixtures::hooked_config(), &fixtures::limits(), 3).expect("service");
    let mut request = hook(Change::Push);
    if let domain::Request::Hook { commit, .. } = &mut request {
        *commit = None;
    }
    let post = service.delivery(&request, &fixtures::limits()).expect("deletion");
    assert_eq!(&*post.body, br#"{"repository":{"id":2},"ref":"refs/heads/main","ref_type":"branch"}"#);
    assert_eq!(fixtures::delivery_event(&post), b"delete");
    assert_eq!(
        decoded(&post),
        Ok(Delivery::Ref {
            created: false,
            repository: REPOSITORY,
            reference: boxed(b"refs/heads/main"),
            ref_type: RefType::Branch
        })
    );
}
#[test]
fn a_hook_with_no_endpoint_is_not_delivered() {
    let service = Service::new(fixtures::config(), &fixtures::limits(), 3).expect("service");
    assert_eq!(service.delivery(&hook(Change::Push), &fixtures::limits()), None);
}
#[test]
fn a_delivery_endpoint_with_an_empty_secret_is_refused() {
    let mut config = fixtures::hooked_config();
    config.hooks.as_mut().expect("endpoint").secret = Secret::new(boxed(b""));
    assert_eq!(Service::new(config, &fixtures::limits(), 3).err(), Some(crate::Error::Limits));
}
#[test]
fn a_delivery_endpoint_with_an_empty_host_is_refused() {
    let mut config = fixtures::hooked_config();
    config.hooks.as_mut().expect("endpoint").host = boxed(b"");
    assert_eq!(Service::new(config, &fixtures::limits(), 3).err(), Some(crate::Error::Limits));
}
#[test]
fn the_delivery_signature_is_rfc_4231s_for_its_key_and_data() {
    assert_eq!(
        &delivery::sign(&Secret::new(boxed(b"Jefe")), b"what do ya want for nothing?"),
        b"5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
}
#[test]
fn a_domain_push_is_delivered_signed_to_the_world() {
    let mut harness = Harness::hooked();
    assert_eq!(harness.push(b"work"), 2);
    harness.run();
    let post = harness.delivered(b"push");
    assert_eq!(&*post.body, br#"{"repository":{"id":2},"ref":"refs/heads/work","after":"0000000000000002000000000000000000000000","pusher":{"id":2}}"#);
    // openssl dgst -sha256 -hmac fixture-hook-secret, over the literal body above.
    assert_eq!(&*post.head, b"POST /hooks/1 HTTP/1.1\r\nHost: 127.0.0.1:7200\r\nContent-Type: application/json\r\nContent-Length: 116\r\nX-Forgejo-Event: push\r\nX-Forgejo-Signature: 50992968a6fe1ac343d9d06a2300294cd5f968a95ebae27937467d665827fd90\r\n\r\n");
    assert_eq!(
        decoded(post),
        Ok(Delivery::Push {
            repository: REPOSITORY,
            reference: boxed(b"refs/heads/work"),
            after: ObjectId {
                format: ObjectFormat::Sha1,
                bytes: [0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            },
            pusher: fixtures::PERSON
        })
    );
}

#[test]
fn service_refuses_limits_without_a_worst_case() {
    let config = fixtures::config();
    let limits = crate::Limits { calls: 0, ..fixtures::limits() };
    assert_eq!(crate::worst_case(&limits, &config), None);
    assert_eq!(Service::new(config, &limits, 3).err(), Some(crate::Error::Limits));
}

#[test]
fn a_signature_holds_for_its_body_only() {
    let post = delivered(Change::Push);
    let secret = Secret::new(boxed(fixtures::FIXTURE_SECRET));
    let signature = *b"59d76cdeaea4277b5fe9055ba0c6f03f8c3ce07dfc2e298df9d624cce4c99df3";
    assert_eq!(delivery::sign(&secret, &post.body), signature);
    assert_ne!(delivery::sign(&secret, b"{}"), signature);
}
