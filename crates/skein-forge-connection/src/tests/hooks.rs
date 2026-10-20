use super::fixtures;
use crate::{
    Limits, Shared, Unusable,
    limits::{self, Stacked},
};
use skein_lib::Secret;
#[test]
fn a_hook_secret_past_its_bound_is_unusable_naming_the_forge() {
    let mut settings = fixtures::hook_settings();
    settings.forges.get_mut(0).expect("forge").access.as_mut().expect("fixture access").webhook_secret =
        Some(Secret::new(Box::new([7; 129])));
    assert_eq!(limits::check(&fixtures::limits(), &fixtures::shared(), &settings), Err(Unusable::Secret { forge: 1 }));
}
#[test]
fn a_hook_secret_bound_below_32_is_unusable() {
    let limits = Limits { hook_secret_bytes: 31, ..fixtures::limits() };
    assert_eq!(
        limits::check(&limits, &fixtures::shared(), &fixtures::settings()),
        Err(Unusable::SecretBytes { bytes: 31 })
    );
}
#[test]
fn a_hook_limit_of_zero_is_unusable() {
    let limits = Limits { hooks: 0, ..fixtures::limits() };
    assert_eq!(limits::check(&limits, &fixtures::shared(), &fixtures::settings()), Err(Unusable::Zero));
}
#[test]
fn a_hook_head_limit_of_zero_is_unusable() {
    let limits = Limits { hook_head_bytes: 0, ..fixtures::limits() };
    assert_eq!(limits::check(&limits, &fixtures::shared(), &fixtures::settings()), Err(Unusable::Zero));
}
#[test]
fn a_hook_body_limit_of_zero_is_unusable() {
    let limits = Limits { hook_body_bytes: 0, ..fixtures::limits() };
    assert_eq!(limits::check(&limits, &fixtures::shared(), &fixtures::settings()), Err(Unusable::Zero));
}
#[test]
fn a_hook_secret_limit_of_zero_is_unusable() {
    let limits = Limits { hook_secret_bytes: 0, ..fixtures::limits() };
    assert_eq!(limits::check(&limits, &fixtures::shared(), &fixtures::settings()), Err(Unusable::Zero));
}
#[test]
fn a_hook_deadline_of_zero_is_unusable() {
    let limits = Limits { hook_deadline: skein_lib::Duration::ZERO, ..fixtures::limits() };
    assert_eq!(limits::check(&limits, &fixtures::shared(), &fixtures::settings()), Err(Unusable::Zero));
}
#[test]
fn a_hook_head_past_what_io_holds_is_unusable_as_a_stacked_server() {
    // Only intake differs from the shipped bound: the webhook head cannot fit it.
    let shared = Shared { io: skein_io::Limits { intake: 8191, ..fixtures::shared().io }, ..fixtures::shared() };
    assert_eq!(
        limits::check(&fixtures::limits(), &shared, &fixtures::settings()),
        Err(Unusable::Stack { stacked: Stacked::ServerOverIo, demand: 8192, cap: 8191 })
    );
}
#[test]
fn the_worst_case_grows_with_the_webhook_connections() {
    let limits = fixtures::limits();
    let one = limits::worst_case(
        &Limits { hooks: 1, ..limits },
        &fixtures::shared(),
        1,
        crate::settings::owned_bytes(&fixtures::hook_settings()).expect("owner bytes"),
    )
    .expect("one");
    let two = limits::worst_case(
        &Limits { hooks: 2, ..limits },
        &fixtures::shared(),
        1,
        crate::settings::owned_bytes(&fixtures::hook_settings()).expect("owner bytes"),
    )
    .expect("two");
    assert!(two > one);
}
#[test]
fn each_delivery_becomes_the_hint_forge_md_5_names() {
    for (delivery, change, key) in hint_rows() {
        assert_eq!(
            skein_forge::forgejo::plans::hint::hint(1, fixtures::WORD, &delivery),
            Some(skein_forge::api::Hint {
                repository: skein_forge::api::Repository { forge: 1, repository: 2 },
                change,
                key,
            }),
            "{delivery:?}"
        );
    }
}
type HintRow = (skein_forge::forgejo::delivery::Delivery, skein_forge::api::Change, Option<Box<[u8]>>);
fn hint_rows() -> [HintRow; 7] {
    use fixtures::{HOOK_HEAD as HEAD, HOOK_NUMBER as NUMBER, HOOK_REPOSITORY as REPOSITORY};
    use skein_forge::api::Change;
    use skein_forge::forgejo::{
        delivery::{Delivery, JobEnd},
        request::ReviewEvent,
    };
    [
        (
            Delivery::Issue { repository: REPOSITORY, number: NUMBER, body: fixtures::hook_body() },
            Change::Item(NUMBER),
            Some(Box::from([1, 2])),
        ),
        (
            Delivery::Comment { repository: REPOSITORY, number: NUMBER, body: fixtures::hook_body() },
            Change::Item(NUMBER),
            Some(Box::from([1, 2])),
        ),
        (
            Delivery::Review {
                repository: REPOSITORY,
                number: NUMBER,
                body: fixtures::hook_body(),
                event: ReviewEvent::Approved,
            },
            Change::Item(NUMBER),
            Some(Box::from([1, 2])),
        ),
        (Delivery::Pull { repository: REPOSITORY, number: NUMBER }, Change::Item(NUMBER), None),
        (
            Delivery::Push {
                repository: REPOSITORY,
                reference: Box::from(b"refs/heads/main".as_slice()),
                after: fixtures::hook_id(),
                pusher: 2,
            },
            Change::BranchMoved { branch: Box::from(b"main".as_slice()), head: HEAD, actor: 2 },
            None,
        ),
        (fixtures::hook_ref(b"refs/heads/main"), Change::Branch(Box::from(b"main".as_slice())), None),
        (
            Delivery::Job { repository: REPOSITORY, commit: fixtures::hook_id(), end: JobEnd::Success },
            Change::Commit(HEAD),
            None,
        ),
    ]
}
#[test]
fn a_tag_delivery_hints_nothing() {
    let delivery = fixtures::hook_ref(b"refs/tags/version");
    assert_eq!(skein_forge::forgejo::plans::hint::hint(1, fixtures::WORD, &delivery), None);
}
#[test]
fn a_repository_id_past_u32_hints_nothing() {
    let delivery =
        skein_forge::forgejo::delivery::Delivery::Pull { repository: u64::MAX, number: fixtures::HOOK_NUMBER };
    assert_eq!(skein_forge::forgejo::plans::hint::hint(1, fixtures::WORD, &delivery), None);
}

#[test]
fn signature_shape_is_refused_at_the_head() {
    // SHA-256 signatures have 64 lowercase hex digits (forge-connection.md 6.2).
    const DIGITS: usize = 64;
    let correct = fixtures::hook_signature(fixtures::PULL_PAYLOAD);
    let mut upper = correct;
    for byte in &mut upper {
        *byte = byte.to_ascii_uppercase();
    }
    let mut nonhex = correct;
    nonhex[0] = b'g';
    let mut wrong = correct;
    wrong[0] = if wrong[0] == b'0' { b'1' } else { b'0' };
    for (name, signature, head_status, demands, final_status, hints, closed) in [
        ("short", correct.get(..DIGITS - 1).expect("short signature"), Some(403), 0, Some(403), 0, true),
        ("long", [b'a'; DIGITS + 1].as_slice(), Some(403), 0, Some(403), 0, true),
        ("uppercase", upper.as_slice(), Some(403), 0, Some(403), 0, true),
        ("nonhex", nonhex.as_slice(), Some(403), 0, Some(403), 0, true),
        ("correct", correct.as_slice(), None, 1, Some(204), 1, false),
        ("wrong digest", wrong.as_slice(), None, 1, Some(403), 0, true),
    ] {
        let mut h = fixtures::hook();
        h.write(&fixtures::hook_head(fixtures::PULL_PAYLOAD, Some(b"pull_request"), signature));
        assert_eq!((h.status(), h.body_demands(), h.hints()), (head_status, demands, 0), "{name}: head only");
        h.write(fixtures::PULL_PAYLOAD);
        assert_eq!((h.status(), h.hints(), h.closed()), (final_status, hints, closed), "{name}: terminal");
    }
}
#[test]
fn a_verified_delivery_without_an_event_is_answered_204_without_a_hint() {
    let mut h = fixtures::hook();
    h.write(&fixtures::signed_head(b"{}", None));
    assert_eq!(h.status(), None);
    h.write(b"{}");
    assert_eq!((h.status(), h.hints(), h.closed()), (Some(204), 0, false));
}
