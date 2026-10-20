use super::fixtures::{SHA, delivery_decode, limits};
use crate::{
    api,
    forgejo::document::Body,
    github::{
        decode::Malformed,
        delivery::{self, Account, Change, Ci, Delivery, Pull},
        guid, id,
        limits::DEPTH,
        server,
    },
    marker,
};
use skein_json::{
    tokenizer,
    writer::{Encoder, Refusal},
};
use skein_lib::{
    List, Writer,
    sha256::{self, Hmac},
};

// The 23 delivery fixtures fit in a capacity rounded up to a power of two.
const CASE_CAPACITY: u32 = 32;
const SIGNATURE_PREFIX: &[u8] = b"sha256=";
const SIGNATURE_PREFIX_WIDTH: usize = SIGNATURE_PREFIX.len();
// SHA-256 produces 32 bytes (skein_lib::sha256::Hmac::finish).
const DIGEST_WIDTH: usize = 32;
const ENCODED_DIGEST_WIDTH: usize = DIGEST_WIDTH * 2;
const SIGNATURE_HEADER_WIDTH: usize = SIGNATURE_PREFIX_WIDTH + ENCODED_DIGEST_WIDTH;
const INSTALLATION_CREATED: &[u8] =
    br#"{"action":"created","installation":{"id":9999999999999999999,"account":{"login":"acme","id":8}}}"#;
const INSTALLATION_DELETED: &[u8] =
    br#"{"action":"deleted","installation":{"id":9999999999999999999,"account":{"login":"acme","id":8}}}"#;
const INSTALLATION_SUSPENDED: &[u8] =
    br#"{"action":"suspend","installation":{"id":9999999999999999999,"account":{"login":"acme","id":8}}}"#;
const INSTALLATION_UNSUSPENDED: &[u8] =
    br#"{"action":"unsuspend","installation":{"id":9999999999999999999,"account":{"login":"acme","id":8}}}"#;
const INSTALLATION_REPOSITORIES_ADDED: &[u8] =
    br#"{"action":"added","installation":{"id":9999999999999999999,"account":{"login":"acme","id":8}}}"#;
const INSTALLATION_REPOSITORIES_REMOVED: &[u8] =
    br#"{"action":"removed","installation":{"id":9999999999999999999,"account":{"login":"acme","id":8}}}"#;
const INSTALLATION_RENAMED: &[u8] =
    br#"{"action":"renamed","installation":{"id":9999999999999999999},"account":{"login":"acme","id":8}}"#;

const ISSUE: &[u8] =
    br#"{"action":"opened","issue":{"number":3,"body":"plain"},"repository":{"id":7,"full_name":"acme/widgets"}}"#;
const PUSH: &[u8] = br#"{"ref":"refs/heads/feat/a","after":"0123456789abcdef0123456789abcdef01234567","deleted":false,"repository":{"id":7},"sender":{"id":9}}"#;
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
fn kept(text: &[u8]) -> Body {
    Body::Kept(Box::from(text))
}
fn pull(action: &[u8]) -> Delivery {
    Delivery::Pull {
        repository: 7,
        number: 3,
        action: Pull {
            action: Box::from(action),
            reason: if action == b"dequeued" { Some(Box::from(*b"CI_FAILURE")) } else { None },
        },
    }
}
fn item_hint(key: Option<Box<[u8]>>) -> api::Hint {
    api::Hint { repository: api::Repository { forge: 2, repository: 7 }, change: api::Change::Item(3), key }
}
fn hint(change: api::Change) -> api::Hint {
    api::Hint { repository: api::Repository { forge: 2, repository: 7 }, change, key: None }
}
fn read(event: &[u8], body: &[u8]) -> Delivery {
    delivery::decode(event, body, &limits()).unwrap()
}
#[derive(Debug)]
struct Case {
    event: &'static [u8],
    body: &'static [u8],
    value: Delivery,
}
fn cases() -> List<Case> {
    let mut cases = List::with_capacity(CASE_CAPACITY);
    for (event, body, value) in [
        (b"issues".as_slice(), ISSUE, Delivery::Issue { repository: 7, number: 3, body: kept(b"plain") }),
        (b"issue_comment", br#"{"action":"created","issue":{"number":3,"body":"ignored"},"comment":{"body":"note"},"repository":{"id":7}}"#, Delivery::Comment { repository: 7, number: 3, body: kept(b"note") }),
        (b"pull_request", br#"{"action":"opened","number":3,"repository":{"id":7}}"#, pull(b"opened")),
        (b"pull_request", br#"{"action":"dequeued","reason":"CI_FAILURE","number":3,"repository":{"id":7}}"#, pull(b"dequeued")),
        (b"pull_request_review", br#"{"action":"submitted","pull_request":{"number":3},"review":{"body":"note"},"repository":{"id":7}}"#, Delivery::Review { repository: 7, number: 3, body: kept(b"note") }),
        (b"pull_request_review_comment", br#"{"action":"created","pull_request":{"number":3},"repository":{"id":7}}"#, Delivery::Remark { repository: 7, number: 3 }),
        (b"push", PUSH, Delivery::Push { repository: 7, branch: Box::from(*b"feat/a"), after: id::commit(SHA).unwrap(), deleted: false, sender: 9 }),
        (b"create", br#"{"ref":"feat/a","ref_type":"branch","repository":{"id":7}}"#, Delivery::Ref { repository: 7, branch: Box::from(*b"feat/a"), created: true }),
        (b"delete", br#"{"ref":"feat/a","ref_type":"branch","repository":{"id":7}}"#, Delivery::Ref { repository: 7, branch: Box::from(*b"feat/a"), created: false }),
        (b"check_run", br#"{"action":"completed","check_run":{"head_sha":"0123456789abcdef0123456789abcdef01234567"},"repository":{"id":7}}"#, Delivery::Ci { repository: 7, commit: id::commit(SHA).unwrap(), event: Ci::CheckRun }),
        (b"check_suite", br#"{"action":"completed","check_suite":{"head_sha":"0123456789abcdef0123456789abcdef01234567"},"repository":{"id":7}}"#, Delivery::Ci { repository: 7, commit: id::commit(SHA).unwrap(), event: Ci::CheckSuite }),
        (b"workflow_job", br#"{"action":"completed","workflow_job":{"head_sha":"0123456789abcdef0123456789abcdef01234567"},"repository":{"id":7}}"#, Delivery::Ci { repository: 7, commit: id::commit(SHA).unwrap(), event: Ci::WorkflowJob }),
        (b"workflow_run", br#"{"action":"completed","workflow_run":{"head_sha":"0123456789abcdef0123456789abcdef01234567"},"repository":{"id":7}}"#, Delivery::Ci { repository: 7, commit: id::commit(SHA).unwrap(), event: Ci::WorkflowRun }),
        (b"status", br#"{"state":"pending","sha":"0123456789abcdef0123456789abcdef01234567","repository":{"id":7}}"#, Delivery::Ci { repository: 7, commit: id::commit(SHA).unwrap(), event: Ci::Status }),
        (b"merge_group", br#"{"action":"checks_requested","merge_group":{"head_sha":"0123456789abcdef0123456789abcdef01234567"},"repository":{"id":7}}"#, Delivery::Ci { repository: 7, commit: id::commit(SHA).unwrap(), event: Ci::ChecksRequested }),
        (b"merge_group", br#"{"action":"destroyed","merge_group":{"head_sha":"0123456789abcdef0123456789abcdef01234567"},"repository":{"id":7}}"#, Delivery::Ci { repository: 7, commit: id::commit(SHA).unwrap(), event: Ci::Destroyed }),
    ] { cases.push(Case { event, body, value }).unwrap(); }
    for (event, body, change) in [
        (b"installation".as_slice(), INSTALLATION_CREATED, Change::Created),
        (b"installation", INSTALLATION_DELETED, Change::Deleted),
        (b"installation", INSTALLATION_SUSPENDED, Change::Suspended),
        (b"installation", INSTALLATION_UNSUSPENDED, Change::Unsuspended),
        (b"installation_repositories", INSTALLATION_REPOSITORIES_ADDED, Change::Repositories),
        (b"installation_repositories", INSTALLATION_REPOSITORIES_REMOVED, Change::Repositories),
        (b"installation_target", INSTALLATION_RENAMED, Change::Renamed { login: Box::from(*b"acme") }),
    ] {
        cases
            .push(Case {
                event,
                body,
                value: Delivery::Installation {
                    installation: 9_999_999_999_999_999_999,
                    account: Account { login: Box::from(*b"acme"), id: 8 },
                    change,
                },
            })
            .unwrap();
    }
    cases
}
fn changed(body: &[u8], from: &[u8], to: &[u8]) -> Box<[u8]> {
    let at = skein_lib::bytes::find(body, from).unwrap();
    let mut out = Writer::new(body.len().saturating_sub(from.len()).saturating_add(to.len()));
    out.put(&body[..at]).unwrap();
    out.put(to).unwrap();
    out.put(&body[at.saturating_add(from.len())..]).unwrap();
    out.finish()
}
fn quoted(text: &[u8]) -> Box<[u8]> {
    let mut measure = Encoder::measure(&limits().writer);
    measure.string(text);
    let mut json = Encoder::write(measure.measured().unwrap(), &limits().writer);
    json.string(text);
    json.finish()
}
fn issue_body(text: &[u8]) -> Box<[u8]> {
    changed(ISSUE, br#""plain""#, &quoted(text))
}
#[test]
fn github_delivery_signature_good() {
    let header = delivery::sign(b"invented webhook secret", ISSUE);
    assert!(header.starts_with(SIGNATURE_PREFIX), "signature prefix");
    assert_eq!(header.len(), SIGNATURE_HEADER_WIDTH);
    for byte in &header[SIGNATURE_PREFIX_WIDTH..] {
        assert!(byte.is_ascii_digit() || (b'a'..=b'f').contains(byte), "lowercase hexadecimal");
    }
    assert_eq!(delivery::signature(&header).unwrap().len(), DIGEST_WIDTH);
    assert!(delivery::verify(b"invented webhook secret", ISSUE, &header), "signed body verifies");
}
#[test]
fn github_delivery_signature_refused() {
    let good = delivery::sign(b"invented webhook secret", ISSUE);
    for header in [
        b"".as_slice(),
        &good[7..],
        b"sha1=0000000000000000000000000000000000000000",
        &good[..70],
        &changed(&good, b"sha256=", b"sha256=0"),
        &changed(&good, b"sha256=", b"sha256=G"),
        &changed(&good, b"sha256=", b"sha256=A"),
        &changed(&good, b"sha256=", b"sha256= "),
    ] {
        assert_eq!(delivery::signature(header), None);
        assert!(!delivery::verify(b"invented webhook secret", ISSUE, header), "invalid header fails verification");
    }
    let mut trailing = Writer::new(72);
    trailing.put(&good).unwrap();
    trailing.put(b"x").unwrap();
    let trailing = trailing.finish();
    assert_eq!(delivery::signature(&trailing), None);
    assert!(!delivery::verify(b"invented webhook secret", ISSUE, &trailing), "trailing bytes fail verification");
    let mut upper = good;
    upper[7] = b'A';
    assert_eq!(delivery::signature(&upper), None);
    assert!(!delivery::verify(b"invented webhook secret", ISSUE, &upper), "uppercase fails verification");
    let mut invalid = good;
    invalid[7] = b'g';
    assert_eq!(delivery::signature(&invalid), None);
    assert!(!delivery::verify(b"invented webhook secret", ISSUE, &invalid), "nonhex fails verification");
    assert!(!delivery::verify(b"another invented secret", ISSUE, &good), "wrong secret fails verification");
    assert!(
        !delivery::verify(b"invented webhook secret", &changed(ISSUE, b"plain", b"plaiN"), &good),
        "changed body fails verification"
    );
}
#[test]
fn github_delivery_signature_chunked() {
    let expected = delivery::signature(&delivery::sign(b"invented webhook secret", ISSUE)).unwrap();
    for chunk in [1, 7, ISSUE.len()] {
        let mut mac = Hmac::new(b"invented webhook secret");
        for part in ISSUE.chunks(chunk) {
            mac.update(part);
        }
        assert!(sha256::equal(&mac.finish(), &expected), "chunked HMAC agrees");
    }
}
#[test]
fn github_delivery_guid_head() {
    assert_eq!(delivery::EVENT_HEADER, b"X-GitHub-Event");
    assert_eq!(delivery::GUID_HEADER, b"X-GitHub-Delivery");
    assert_eq!(delivery::SIGNATURE_HEADER, b"X-Hub-Signature-256");
    let text = b"01234567-89ab-cdef-0123-456789abcdef";
    assert_eq!(&guid::write(&guid::read(text).unwrap()), text);
    assert_eq!(guid::read(b"not a UUID"), None);
}
#[test]
fn github_delivery_issues() {
    let marked = marker::encode(b"example-app", b"key", b"description", 128).unwrap();
    for (text, key) in [(marked.as_ref(), Some(Box::<[u8]>::from(*b"key"))), (b"plain".as_slice(), None)] {
        let value = read(b"issues", &issue_body(text));
        assert_eq!(value, Delivery::Issue { repository: 7, number: 3, body: kept(text) });
        assert_eq!(delivery::hint(2, b"example-app", &value), Some(item_hint(key)));
    }
    for body in [changed(ISSUE, br#""plain""#, b"null"), changed(ISSUE, br#","body":"plain""#, b"")] {
        let value = read(b"issues", &body);
        assert_eq!(value, Delivery::Issue { repository: 7, number: 3, body: kept(b"") });
        assert_eq!(delivery::hint(2, b"example-app", &value), Some(item_hint(None)));
    }
    let text = [b'x'; 129];
    let mut good = limits();
    good.body_bytes = 8;
    let value = delivery::decode(b"issues", &issue_body(&text), &good).unwrap();
    assert_eq!(value, Delivery::Issue { repository: 7, number: 3, body: Body::Long(129) });
    assert_eq!(delivery::hint(2, b"example-app", &value), Some(item_hint(None)));
}
#[test]
fn github_delivery_comments_and_reviews() {
    let marked = marker::encode(b"example-app", b"key", b"note", 128).unwrap();
    let marked_json = quoted(&marked);
    for case in &cases() {
        if [b"issue_comment".as_slice(), b"pull_request_review", b"pull_request_review_comment"].contains(&case.event) {
            assert_eq!(read(case.event, case.body), case.value);
            assert_eq!(delivery::hint(2, b"example-app", &case.value), Some(item_hint(None)));
            if case.event != b"pull_request_review_comment" {
                let body = changed(case.body, br#""note""#, &marked_json);
                assert_eq!(
                    delivery::hint(2, b"example-app", &read(case.event, &body)),
                    Some(item_hint(Some(Box::from(*b"key"))))
                );
                let long = changed(case.body, b"note", &[b'x'; 129]);
                assert_eq!(delivery::hint(2, b"example-app", &read(case.event, &long)), Some(item_hint(None)));
                let body = changed(case.body, br#""note""#, b"null");
                assert_eq!(delivery::hint(2, b"example-app", &read(case.event, &body)), Some(item_hint(None)));
            }
        }
    }
}
#[test]
fn github_delivery_pull_request() {
    let base = br#"{"action":"opened","number":3,"repository":{"id":7},"reason":"CI_FAILURE"}"#;
    for action in ACTIONS {
        let body = changed(base, b"opened", action);
        assert_eq!(read(b"pull_request", &body), pull(action));
        assert_eq!(delivery::hint(2, b"example-app", &pull(action)), Some(item_hint(None)));
    }
    assert_eq!(read(b"pull_request", &changed(base, b"opened", b"unknown")), Delivery::Other);
    assert_eq!(
        delivery::decode(b"pull_request", br#"{"action":"dequeued","number":3,"repository":{"id":7}}"#, &limits()),
        Err(Malformed::Missing { field: b"reason" })
    );
}
#[test]
fn github_delivery_push() {
    let value = read(b"push", PUSH);
    assert_eq!(
        delivery::hint(2, b"example-app", &value),
        Some(hint(api::Change::BranchMoved {
            branch: Box::from(*b"feat/a"),
            head: id::commit(SHA).unwrap(),
            actor: 9
        }))
    );
    let deleted = read(b"push", &changed(PUSH, b"false", b"true"));
    assert_eq!(delivery::hint(2, b"example-app", &deleted), Some(hint(api::Change::Branch(Box::from(*b"feat/a")))));
    assert_eq!(read(b"push", &changed(PUSH, b"refs/heads/", b"refs/tags/")), Delivery::Other);
    for sha in [b"invalid".as_slice(), b"0123456789ABCDEF0123456789abcdef01234567"] {
        assert_eq!(
            delivery::decode(b"push", &changed(PUSH, SHA, sha), &limits()),
            Err(Malformed::Id { field: b"after" })
        );
    }
}
#[test]
fn github_delivery_create_and_delete() {
    for case in &cases() {
        if [b"create".as_slice(), b"delete"].contains(&case.event) {
            assert_eq!(read(case.event, case.body), case.value);
            assert_eq!(
                delivery::hint(2, b"example-app", &case.value),
                Some(hint(api::Change::Branch(Box::from(*b"feat/a"))))
            );
            assert_eq!(read(case.event, &changed(case.body, b"branch", b"tag")), Delivery::Other);
        }
    }
}
#[test]
fn github_delivery_ci_events() {
    for case in &cases() {
        match case.value {
            Delivery::Ci { .. } => {
                assert_eq!(read(case.event, case.body), case.value);
                assert_eq!(
                    delivery::hint(2, b"example-app", &case.value),
                    Some(hint(api::Change::Commit(id::commit(SHA).unwrap())))
                );
                if case.event != b"status" {
                    let action = if case.event == b"merge_group" {
                        if skein_lib::bytes::find(case.body, b"checks_requested").is_some() {
                            b"checks_requested".as_slice()
                        } else {
                            b"destroyed"
                        }
                    } else {
                        b"completed"
                    };
                    assert_eq!(read(case.event, &changed(case.body, action, b"queued")), Delivery::Other);
                }
            }
            Delivery::Issue { .. }
            | Delivery::Comment { .. }
            | Delivery::Pull { .. }
            | Delivery::Review { .. }
            | Delivery::Remark { .. }
            | Delivery::Push { .. }
            | Delivery::Ref { .. }
            | Delivery::Installation { .. }
            | Delivery::Other => {}
        }
    }
}
#[test]
fn github_delivery_installation_events() {
    for case in &cases() {
        if case.event.starts_with(b"installation") {
            assert_eq!(read(case.event, case.body), case.value);
            assert_eq!(delivery::hint(2, b"example-app", &case.value), None);
        }
    }
}
#[test]
fn github_delivery_other_events() {
    for event in [b"ping".as_slice(), b"unknown", b"", b"Issues", b"issues "] {
        assert!(delivery::decoder(event, &limits()).is_none(), "unknown event has no decoder");
        assert_eq!(read(event, b"not JSON"), Delivery::Other);
    }
    assert_eq!(delivery::hint(2, b"example-app", &Delivery::Other), None);
}
#[test]
fn github_delivery_repository_past_32_bits() {
    for (id, repository) in [(b"4294967296".as_slice(), None), (b"4294967295", Some(u32::MAX))] {
        let body = changed(ISSUE, b"7", id);
        let got = delivery::hint(2, b"example-app", &read(b"issues", &body));
        if let Some(repository) = repository {
            assert_eq!(got.unwrap().repository.repository, repository);
        } else {
            assert_eq!(got, None);
        }
    }
}
#[test]
fn github_delivery_malformed() {
    for (body, error) in [
        (b"not JSON".as_slice(), Malformed::Json(tokenizer::Error::Unexpected)),
        (b"{", Malformed::Json(tokenizer::Error::Truncated)),
        (b"{}", Malformed::Missing { field: b"action" }),
        (br#"{"action":null}"#, Malformed::Missing { field: b"action" }),
        (br#"{"action":"opened","issue":{"number":"3"}}"#, Malformed::Shape { field: b"number" }),
        (br#"{"action":"opened","issue":{"number":-1}}"#, Malformed::Range { field: b"number" }),
        (br#"{"action":"opened","issue":{"number":18446744073709551616}}"#, Malformed::Range { field: b"number" }),
        (br#"{"action":"opened","issue":{"number":1.5}}"#, Malformed::Range { field: b"number" }),
        (br#"{"action":"opened","action":"closed"}"#, Malformed::Shape { field: b"action" }),
        (b"[]", Malformed::Shape { field: b"" }),
    ] {
        assert_eq!(delivery::decode(b"issues", body, &limits()), Err(error), "{body:?}");
    }
}
#[test]
fn github_delivery_bounded_by_limits() {
    let mut good = limits();
    good.json.length = 4;
    assert_eq!(delivery::decode(b"issues", ISSUE, &good), Err(Malformed::Json(tokenizer::Error::TooLong)));
    let mut good = limits();
    good.string_bytes = 2;
    assert_eq!(delivery::decode(b"issues", ISSUE, &good), Err(Malformed::TooLong { field: b"action" }));
    let mut good = limits();
    good.json.string = 2;
    assert_eq!(delivery::decode(b"issues", ISSUE, &good), Err(Malformed::Json(tokenizer::Error::StringTooLong)));
    let mut out = Writer::new(2401);
    out.put(b"[").unwrap();
    for n in 0_u32..300 {
        if n != 0 {
            out.put(b",").unwrap();
        }
        out.put(br#"{"x":0}"#).unwrap();
    }
    out.put(b"]").unwrap();
    let objects = super::fixtures::bytes(out);
    let mut out = Writer::new(PUSH.len().saturating_add(objects.len()).saturating_add(11));
    out.put(b"{\"commits\":").unwrap();
    out.put(&objects).unwrap();
    out.put(b",").unwrap();
    out.put(&PUSH[1..]).unwrap();
    assert_eq!(read(b"push", &out.finish()), read(b"push", PUSH));
    let body = changed(ISSUE, b"plain", &[b'x'; 129]);
    let mut good = limits();
    good.body_bytes = 200;
    good.json.string = 200;
    assert_eq!(
        delivery::decode(b"issues", &body, &good),
        Ok(Delivery::Issue { repository: 7, number: 3, body: kept(&[b'x'; 129]) })
    );
}
fn nested_payload(depth: u32) -> Box<[u8]> {
    let mut out =
        Writer::new(ISSUE.len().saturating_add(32).saturating_add(usize::try_from(depth.saturating_mul(2)).unwrap()));
    out.put(b"{\"ignored\":").unwrap();
    for _ in 1..depth {
        out.put(b"[").unwrap();
    }
    out.put(b"0").unwrap();
    for _ in 1..depth {
        out.put(b"]").unwrap();
    }
    out.put(b",").unwrap();
    out.put(&ISSUE[1..]).unwrap();
    super::fixtures::bytes(out)
}
#[test]
fn github_delivery_logged_payload_decodes() {
    let payload = nested_payload(DEPTH.saturating_sub(2));
    let mut out = Writer::new(payload.len().saturating_add(80));
    out.put(br#"{"id":1,"request":{"headers":{},"payload":"#).unwrap();
    out.put(&payload).unwrap();
    out.put(b"}}").unwrap();
    let logged = super::fixtures::bytes(out);
    assert_eq!(tokenizer::validate(&logged, &limits().json), Ok(()));
    let mut budget = limits();
    budget.json.depth = DEPTH.saturating_sub(2);
    assert_eq!(delivery::decode(b"issues", &payload, &budget), Ok(read(b"issues", ISSUE)));
    assert_eq!(
        delivery::decode(b"issues", &nested_payload(DEPTH.saturating_sub(1)), &budget),
        Err(Malformed::Json(tokenizer::Error::TooDeep))
    );
    assert_eq!(
        delivery::decode(b"issues", &nested_payload(DEPTH.saturating_add(1)), &limits()),
        Err(Malformed::Json(tokenizer::Error::TooDeep))
    );
    let mut relaxed = limits();
    relaxed.json.depth = DEPTH.saturating_add(1);
    assert_eq!(
        delivery::decode(b"issues", &nested_payload(DEPTH.saturating_add(1)), &relaxed),
        Err(Malformed::Json(tokenizer::Error::TooDeep))
    );
}
#[test]
fn github_delivery_chunked() {
    for case in &cases() {
        for chunk in [1, 7] {
            assert_eq!(
                delivery_decode(case.event, case.body, chunk, &limits()),
                Ok(case.value.clone()),
                "{:?} chunk {chunk}",
                case.event
            );
        }
    }
}
#[test]
fn github_delivery_encoder_round_trips() {
    let mut values = List::with_capacity(64);
    for case in &cases() {
        values.push(case.value.clone()).unwrap();
    }
    for action in ACTIONS {
        values.push(pull(action)).unwrap();
    }
    values
        .push(Delivery::Issue {
            repository: 7,
            number: 3,
            body: Body::Kept(marker::encode(b"example-app", b"key", b"note", 128).unwrap()),
        })
        .unwrap();
    values.push(read(b"push", &changed(PUSH, b"false", b"true"))).unwrap();
    values.push(Delivery::Other).unwrap();
    for value in &values {
        let sent = server::encode(value, &limits()).unwrap();
        assert_eq!(delivery::decode(sent.event, &sent.body, &limits()), Ok(value.clone()));
    }
    for value in [
        Delivery::Issue { repository: 7, number: 3, body: Body::Long(129) },
        Delivery::Comment { repository: 7, number: 3, body: Body::Long(129) },
        Delivery::Review { repository: 7, number: 3, body: Body::Long(129) },
    ] {
        assert_eq!(server::encode(&value, &limits()), Err(Refusal::TooLong));
    }
    let mut tight = limits();
    tight.writer.length = 2;
    assert_eq!(server::encode(&read(b"issues", ISSUE), &tight), Err(Refusal::TooLong));
}
#[test]
fn github_delivery_sign_the_encoded_body() {
    let sent = server::encode(&read(b"push", PUSH), &limits()).unwrap();
    let signature = delivery::sign(b"invented webhook secret", &sent.body);
    assert!(delivery::verify(b"invented webhook secret", &sent.body, &signature), "encoded body verifies");
}
#[test]
fn github_delivery_terminal_and_encoder_refusals() {
    let mut decoder = delivery::decoder(b"issues", &limits()).unwrap();
    assert_eq!(decoder.step(tokenizer::Event::Done), delivery::Step::Malformed(Malformed::Shape { field: b"" }));
    assert_eq!(
        decoder.step(tokenizer::Event::Token(skein_json::Token::ObjectStart)),
        delivery::Step::Malformed(Malformed::Shape { field: b"" })
    );
    let mut value = pull(b"dequeued");
    match &mut value {
        Delivery::Pull { action, .. } => action.reason = None,
        Delivery::Issue { .. }
        | Delivery::Comment { .. }
        | Delivery::Review { .. }
        | Delivery::Remark { .. }
        | Delivery::Push { .. }
        | Delivery::Ref { .. }
        | Delivery::Ci { .. }
        | Delivery::Installation { .. }
        | Delivery::Other => unreachable!(),
    }
    assert_eq!(server::encode(&value, &limits()), Err(Refusal::Text));
    let invalid = Delivery::Ci { repository: 7, commit: [1; 32], event: Ci::Status };
    assert_eq!(server::encode(&invalid, &limits()), Err(Refusal::Text));
    let invalid = Delivery::Ref { repository: 7, branch: Box::from([b'x'; 129]), created: true };
    assert_eq!(server::encode(&invalid, &limits()), Err(Refusal::TooLong));
    let invalid = Delivery::Issue { repository: 7, number: 3, body: kept(&[b'x'; 129]) };
    assert_eq!(server::encode(&invalid, &limits()), Err(Refusal::TooLong));
}
#[test]
fn github_delivery_installation_shapes() {
    for (event, body, error) in [
        (
            b"installation".as_slice(),
            br#"{"action":"created","installation":{"id":1},"account":{"login":"acme","id":8}}"#.as_slice(),
            Malformed::Missing { field: b"account" },
        ),
        (
            b"installation_target",
            br#"{"action":"renamed","installation":{"id":1,"account":{"login":"acme","id":8}}}"#,
            Malformed::Missing { field: b"account" },
        ),
        (
            b"installation",
            br#"{"action":"created","installation":{"id":1,"account":{"login":null,"id":8}}}"#,
            Malformed::Missing { field: b"login" },
        ),
    ] {
        assert_eq!(delivery::decode(event, body, &limits()), Err(error));
    }
}
#[test]
fn github_delivery_installation_unknown_actions() {
    for (event, body, action, expected) in [
        (b"installation".as_slice(), INSTALLATION_CREATED, b"created".as_slice(), Delivery::Other),
        (b"installation", INSTALLATION_DELETED, b"deleted", Delivery::Other),
        (b"installation", INSTALLATION_SUSPENDED, b"suspend", Delivery::Other),
        (b"installation", INSTALLATION_UNSUSPENDED, b"unsuspend", Delivery::Other),
        (b"installation_repositories", INSTALLATION_REPOSITORIES_ADDED, b"added", Delivery::Other),
        (b"installation_repositories", INSTALLATION_REPOSITORIES_REMOVED, b"removed", Delivery::Other),
        (b"installation_target", INSTALLATION_RENAMED, b"renamed", Delivery::Other),
    ] {
        assert_eq!(read(event, &changed(body, action, b"unknown")), expected);
    }
}
#[test]
fn github_delivery_encoder_refuses_invalid_renamed_login() {
    let invalid = Delivery::Installation {
        installation: 1,
        account: Account { login: Box::from(*b"acme"), id: 8 },
        change: Change::Renamed { login: Box::from(*b"other") },
    };
    assert_eq!(server::encode(&invalid, &limits()), Err(Refusal::Text));
}
