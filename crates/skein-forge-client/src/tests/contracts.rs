//! Contracts at the owner's boundary (forge-client.md, sections 2 and 7).
use super::{LIMITS, configured};
use crate::{Domain, Limits};

const KEY_BYTES: u32 = 7; // version + two length bytes + kit + one identity byte
const DEPLOYMENT: [u8; 16] = [7; 16];

#[test]
fn keys_frame_the_namespace_and_opaque_identity() {
    let key = crate::effect_key(b"kit", b"a", KEY_BYTES);
    assert_eq!(key.as_deref(), Some(b"\x01\x00\x03kita".as_slice()));
}

#[test]
fn invalid_key_frames_are_refused() {
    const OVERLONG: usize = 65536; // one past the u16 namespace length
    let long = skein_lib::bytes::zeroed(OVERLONG);
    for (name, namespace, identity, max) in [
        ("empty namespace", b"".as_slice(), b"a".as_slice(), KEY_BYTES),
        ("long namespace", long.as_ref(), b"a", u32::MAX),
        ("empty identity", b"kit", b"", KEY_BYTES),
        ("short cap", b"kit", b"a", KEY_BYTES - 1),
    ] {
        assert_eq!(crate::effect_key(namespace, identity, max), None, "{name}");
    }
}

#[test]
fn the_largest_namespace_keeps_its_identity() {
    const REPEATS: usize = 21845; // 65535 bytes / kit's three bytes
    const MAX_KEY: u32 = 65539; // version + length + largest namespace + identity
    let mut namespace = skein_lib::List::with_capacity(65535);
    for _ in 0..REPEATS {
        for byte in b"kit" {
            namespace.push(*byte).expect("largest namespace room");
        }
    }
    let key = crate::effect_key(namespace.as_slice(), b"a", MAX_KEY).expect("largest namespace fits");
    assert_eq!(key.len(), 65539);
    assert_eq!(key.get(..3), Some([1, 255, 255].as_slice()));
    assert_eq!(key.last(), Some(&b'a'));
}

#[test]
fn opaque_identities_are_stable_and_distinct() {
    let first = crate::effect_key(b"kit", b"a", KEY_BYTES);
    assert_eq!(first.as_deref(), Some(b"\x01\x00\x03kita".as_slice()));
    assert_eq!(first, crate::effect_key(b"kit", b"a", KEY_BYTES));
    assert_ne!(first, crate::effect_key(b"kit", b"b", KEY_BYTES));
    assert_ne!(first, crate::effect_key(b"b", b"a", KEY_BYTES));
}

#[test]
fn binding_replaces_the_namespace_before_restore() {
    let mut h = Harness::new(LIMITS);
    h.domain = configured(&LIMITS);
    assert!(h.step(Event::Restored { clock: crate::RecoveryClock::Monotonic }).is_empty());
    let mut d = h.domain;
    assert!(d.bind_deployment(DEPLOYMENT, &LIMITS));
    let key = crate::effect_key(&DEPLOYMENT, b"a", LIMITS.op_bytes).expect("deployment frame fits");
    assert!(crate::identity::historical(&d, &key));
}

#[test]
fn binding_without_configuration_changes_nothing() {
    let mut d = Domain::new(&LIMITS);
    assert!(!d.bind_deployment(DEPLOYMENT, &LIMITS));
    assert_eq!(d.config, None);
}

#[test]
fn binding_past_op_bytes_preserves_the_namespace() {
    const KIT_BYTES: u32 = 3; // namespace kit, smaller than the deployment id
    let mut d = configured(&LIMITS);
    assert!(!d.bind_deployment(DEPLOYMENT, &Limits { op_bytes: KIT_BYTES, ..LIMITS }));
    assert_eq!(d.config.expect("configured").namespace.as_ref(), b"kit");
}

use super::{Harness, REPO, at};
use crate::{Cached, Condition, Delivery, Echo, Effect, Event, RepositoryRecord, Request, Stored};
use alloc::boxed::Box;
use skein_forge::api::{self, Answer, Error, Read, Write};
use skein_lib::Token;

#[test]
fn stored_measure_uses_fixed_widths() {
    let repository = Stored::Repository(RepositoryRecord { repository: REPO, mark: None, clock: at(100) });
    assert_eq!(crate::measure::stored(&repository), Some(0));
    let mut row = super::fixtures::keep::measure_row();
    assert_eq!(crate::measure::stored(&Stored::Live(row.clone())), Some(3));
    row.comments = Box::new([Delivery { id: 1, revision: 1 }]);
    row.echoes = Box::new([Echo::Comment(1)]);
    assert_eq!(crate::measure::stored(&Stored::Live(row.clone())), Some(35));
    row.reviews = Box::new([Delivery { id: 2, revision: 1 }]);
    assert_eq!(crate::measure::stored(&Stored::Live(row.clone())), Some(51));
    let mut item = super::fixtures::keep::summary();
    item.title = Box::new([]);
    item.labels = Box::new([Box::from(&b"kit"[..])]);
    row.cached = Box::new(Cached { item: Some(item), pull: None, tip: None });
    assert_eq!(crate::measure::stored(&Stored::Live(row.clone())), Some(70));
    let mut pull = super::fixtures::keep::current();
    pull.head = Box::from(&b"kit"[..]);
    pull.base = Box::from(&b"kit"[..]);
    pull.reviewers = Box::new([7, 8]);
    row.cached.pull = Some(pull);
    assert_eq!(crate::measure::stored(&Stored::Live(row)), Some(92));
}

#[test]
fn effect_measure_uses_fixed_widths() {
    for (name, write, condition, expected) in [
        ("reviewers", Write::SetReviewers { number: 1, reviewers: Box::new([7, 8]) }, Condition::None, 16),
        (
            "pull",
            Write::OpenPull {
                title: Box::new([]),
                body: Box::new([]),
                head: Box::from(&b"kit"[..]),
                base: Box::from(&b"kit"[..]),
            },
            Condition::None,
            6,
        ),
        ("merge", Write::Merge { number: 1, head: [1; 32] }, Condition::Merge { base: Box::from(&b"kit"[..]) }, 3),
    ] {
        assert_eq!(crate::measure::effect(&Effect { write, condition }), Some(expected), "{name}");
    }
}

fn measured_read(read: &Read, answer: &Answer, cap: u32, accepted: bool) {
    let mut h = Harness::new(Limits { answer_bytes: cap, ..LIMITS });
    assert!(h.step(Event::Read { owner: Token::new(1), repository: REPO, read: read.clone() }).is_empty());
    let sent = h.one();
    let expected = if accepted { Ok(answer.clone()) } else { Err(Error::TooLarge) };
    assert_eq!(
        h.answer(&sent, 1, Ok(answer.clone())).as_ref(),
        &[Request::Read { owner: Token::new(1), result: expected }]
    );
}

#[test]
fn answer_admission_uses_fixed_widths() {
    let mut item = super::fixtures::keep::summary();
    item.title = Box::from(&b"kit"[..]);
    let mut empty = item.clone();
    empty.title = Box::new([]);
    let mut comment = super::fixtures::keep::comment(1);
    comment.body = Box::from(&b"kit"[..]);
    comment.key = Some(Box::from(&b"a"[..]));
    let mut pull = super::fixtures::keep::current();
    pull.head = Box::from(&b"kit"[..]);
    pull.base = Box::from(&b"kit"[..]);
    pull.reviewers = Box::new([7, 8]);
    let attempt = api::JobAttempt { head: [1; 32], run: 1, job: 1, attempt: 1 };
    for (read, answer, bytes) in [
        (
            Read::Items { since: at(0), page: 1, kind: None },
            Answer::Items { items: Box::new([item]), more: false, now: at(100) },
            131,
        ),
        (
            Read::Item { number: 9, after: 0 },
            Answer::Item { item: empty.clone(), comments: Box::new([comment]), more: false },
            100,
        ),
        (Read::Pull { number: 9 }, Answer::Pull(pull.clone()), 22),
        (
            Read::Job { attempt, max_bytes: 4 },
            Answer::Job {
                attempt,
                step: Some(api::Step { number: 1, name: Box::from(&b"kit"[..]) }),
                log: Box::from(&b"ab"[..]),
                truncated: false,
            },
            5,
        ),
    ] {
        assert_eq!(skein_forge::measure::answer(&answer), Some(u64::from(bytes)), "{read:?}");
        measured_read(&read, &answer, bytes, true);
        measured_read(&read, &answer, bytes.checked_sub(1).expect("positive cap"), false);
    }
    empty.labels = Box::new([Box::new([]), Box::new([]), Box::new([]), Box::new([]), Box::new([])]);
    pull.reviewers = Box::new([1, 2, 3, 4, 5]);
    for (read, answer) in [
        (
            Read::Branches,
            Answer::Branches(Box::new([Box::new([]), Box::new([]), Box::new([]), Box::new([]), Box::new([])])),
        ),
        (Read::Item { number: 9, after: 0 }, Answer::Item { item: empty, comments: Box::new([]), more: false }),
        (Read::Pull { number: 9 }, Answer::Pull(pull)),
        (
            Read::Protection { branch: Box::from(&b"kit"[..]) },
            Answer::Protection(Some(api::Protection {
                branch: Box::from(&b"kit"[..]),
                contexts: Box::new([Box::new([]), Box::new([]), Box::new([]), Box::new([]), Box::new([])]),
                approvals: 0,
                dismiss_stale: false,
            })),
        ),
    ] {
        assert!(!crate::measure::answer_rows(&answer, LIMITS.rows), "{read:?}");
        measured_read(&read, &answer, LIMITS.answer_bytes, false);
    }
    assert!(crate::measure::answer_rows(&Answer::Done, 0));
}

#[test]
fn keep_count_refusal_precedes_restore_busy() {
    let mut h = Harness::new(LIMITS);
    let mut watches = skein_lib::List::with_capacity(5);
    for number in 0..5 {
        watches
            .push(crate::Watch {
                resource: crate::Resource { repository: REPO, what: crate::What::Issue(number) },
                participating: false,
            })
            .expect("five watches");
    }
    let watches = watches.into_boxed();
    assert_eq!(
        h.step(Event::Keep { owner: Token::new(1), watches }).as_ref(),
        &[Request::Kept { owner: Token::new(1), result: Err(Error::TooLarge) }]
    );
}

#[test]
fn make_unready_precedes_missing_writer() {
    let mut h = Harness::new(LIMITS);
    let entry = super::fixtures::entry(Write::Close { number: 9 });
    assert_eq!(
        h.step(Event::Make { entry }).as_ref(),
        &[Request::Outcome { entry: 1, task: 7, outcome: crate::Outcome::Failed(Error::Busy) }]
    );
}

#[test]
fn read_size_refusal_precedes_full_slab() {
    let mut h = Harness::new(LIMITS);
    for owner in 1..=8 {
        h.read(owner);
    }
    assert!(!h.domain.has_room());
    let got = h.step(Event::Read {
        owner: Token::new(9),
        repository: REPO,
        read: Read::Branch { branch: Box::new([b'a'; 257]) },
    });
    assert_eq!(got.as_ref(), &[Request::Read { owner: Token::new(9), result: Err(Error::TooLarge) }]);
}

#[test]
fn binding_after_identity_use_preserves_the_namespace() {
    let row = super::fixtures::keep::measure_row();
    let entry = super::fixtures::entry(Write::Close { number: 9 });
    for event in [
        Event::Restore { record: Stored::Live(row) },
        Event::Restore {
            record: Stored::Repository(RepositoryRecord { repository: REPO, mark: None, clock: at(100) }),
        },
        Event::Keep { owner: Token::new(1), watches: Box::new([]) },
        Event::Make { entry: entry.clone() },
        Event::Remake { entry },
    ] {
        let mut h = Harness::new(LIMITS);
        h.domain = configured(&LIMITS);
        h.step(event);
        assert!(!h.domain.bind_deployment(DEPLOYMENT, &LIMITS));
        assert_eq!(h.domain.config.as_ref().expect("configured").namespace.as_ref(), b"kit");
    }
}

#[test]
fn an_answered_busy_is_an_invalid_read_answer() {
    let mut h = Harness::new(LIMITS);
    h.read(1);
    let sent = h.one();
    assert_eq!(
        h.answer(&sent, 1, Err(Error::Busy)).as_ref(),
        &[Request::Read { owner: Token::new(1), result: Err(Error::InvalidAnswer) }]
    );
    assert!(h.facts().contains(&crate::Fact::Failed { priority: crate::Priority::Fresh, error: Error::InvalidAnswer }));
}

#[test]
fn an_answered_busy_makes_a_write_uncertain() {
    let mut h = super::fixtures::outbox::Harness::new();
    h.make(1, super::fixtures::outbox::comment(), Condition::None);
    h.clock();
    let write = h.one();
    h.answer(write, Err(Error::Busy));
    assert_eq!(h.store.get(&1).expect("progress saved").failures, 1);
    assert_eq!(h.outcomes.as_slice(), &[(1, crate::Outcome::Uncertain)]);
    assert_eq!(h.one().op, api::Op::Read(Read::Item { number: 9, after: 0 }));
    assert_eq!(h.writes, 1);
}
