use super::fixtures::{Harness, configured};
use super::fixtures::{keep as kept, outbox as writing, recovery as fixture};
use super::{LIMITS, REPO, at};
use crate::{Condition, Echo, Event, Fact, Limits, Made, Outcome, RecoveryClock, Request, Resource, Stored, What};
use alloc::boxed::Box;
use skein_forge::api::{self, Answer, Change, Error, Op, Read, Write};
use skein_lib::{Duration, List, Token, Wall};

#[test]
fn restart_reads_before_settling_the_outbox() {
    let mut h = kept::Harness::new();
    let mut row = fixture::live();
    row.watch = kept::watch(kept::issue(), false);
    h.domain = configured(&h.env.limits);
    h.event(Event::Restore { record: Stored::Live(row) });
    h.event(Event::Restore { record: Stored::Repository(fixture::repository()) });
    h.event(Event::Restored { clock: RecoveryClock::Monotonic });
    h.event(Event::Writer { resource: kept::issue(), taken: true });
    h.event(Event::PauseOutbox);
    let saved = fixture::uncertain_comment().store.get(&1).expect("saved attempt").clone();
    h.event(Event::Make { entry: saved });
    h.event(Event::ReadAfresh);
    let listing = h.one();
    assert_eq!(listing.op, Op::Read(Read::Items { since: at(100), page: 1, kind: None }));
    h.answer(listing, Answer::Items { items: Box::new([]), more: false, now: at(100) });
    let object = h.one();
    assert_eq!(object.op, Op::Read(Read::Item { number: 9, after: u64::MAX }));
    h.answer(object, fixture::empty_item());
    assert_eq!(h.fresh_done, 1);
    assert!(h.send().is_none());
    h.event(Event::SettleOutbox);
    let find = h.one();
    assert_eq!(find.op, Op::Read(Read::Item { number: 9, after: 0 }));
    h.answer(find, fixture::empty_item());
    assert_eq!(h.outbox_done, 1);
    assert!(h.send().is_none());
    assert_eq!(h.fresh_done, 1);
}
#[test]
fn remake_releases_a_held_entry() {
    let mut h = fixture::uncertain_comment();
    let saved = h.store.get(&1).expect("saved uncertain entry").clone();
    let find = h.one();
    h.answer(find, Ok(fixture::empty_item()));
    h.fire(111);
    let find = h.one();
    h.answer(find, Ok(fixture::empty_item()));
    assert_eq!(h.outcomes.as_slice(), &[(1, Outcome::Uncertain), (1, Outcome::Held)]);
    h.store.insert(1, saved.clone()).expect("durable held entry");
    h.step(Event::Remake { entry: saved });
    let find = h.one();
    h.answer(find, Ok(fixture::empty_item()));
    let write = h.one();
    assert_eq!(write.op, Op::Write(writing::comment()));
    h.answer(write, Ok(Answer::Commented(9)));
    assert_eq!(h.writes, 2);
    assert_eq!(h.outcomes.as_slice().last(), Some(&(1, Outcome::Made { made: Made::Commented(9), found: false })));
}
#[test]
fn restore_refuses_bad_pages() {
    for (name, review, page) in [
        ("comment zero", false, 0),
        ("comment overflow", false, LIMITS.inbox + 1),
        ("review zero", true, 0),
        ("review overflow", true, LIMITS.inbox + 1),
    ] {
        let mut row = fixture::live();
        if review {
            row.review_page = page;
        } else {
            row.comment_page = page;
        }
        fixture::bad_live(name, row);
    }
}
#[test]
fn restore_refuses_bad_stamps() {
    for (name, review, count, duplicate) in [
        ("comments overflow", false, LIMITS.inbox + 1, false),
        ("comments repeated", false, 2, true),
        ("reviews overflow", true, LIMITS.inbox + 1, false),
        ("reviews repeated", true, 2, true),
    ] {
        let mut row = fixture::live();
        if review {
            row.reviews = fixture::deliveries(count, duplicate);
        } else {
            row.comments = fixture::deliveries(count, duplicate);
        }
        fixture::bad_live(name, row);
    }
}
#[test]
fn restore_refuses_oversized_caches() {
    for (name, pull, bytes) in [
        ("item bytes", false, true),
        ("item rows", false, false),
        ("pull bytes", true, true),
        ("pull rows", true, false),
    ] {
        let mut row = fixture::live();
        if pull {
            let mut value = kept::current();
            if bytes {
                value.head = fixture::repeated(b'a', LIMITS.answer_bytes + 1);
            } else {
                value.reviewers = fixture::repeated(7, LIMITS.rows + 1);
            }
            row.cached.pull = Some(value);
        } else {
            let mut value = kept::summary();
            if bytes {
                value.title = fixture::repeated(b'a', LIMITS.answer_bytes + 1);
            } else {
                value.labels = Box::new([Box::new([]), Box::new([]), Box::new([]), Box::new([]), Box::new([])]);
            }
            row.cached.item = Some(value);
        }
        fixture::bad_live(name, row);
    }
}
#[test]
fn restore_refuses_oversized_echoes() {
    let mut row = fixture::live();
    row.echoes = fixture::repeated(Echo::Comment(1), LIMITS.rows + 1);
    fixture::bad_live("echo overflow", row);
}
#[test]
fn restore_refuses_unpaired_rows() {
    fixture::failed_restore("missing repository", Box::new([Stored::Live(fixture::live())]));
    fixture::failed_restore("missing live", Box::new([Stored::Repository(fixture::repository())]));
}
#[test]
fn restore_refuses_capacity_overflow() {
    for (name, repositories, count) in
        [("resources", false, LIMITS.resources + 1), ("repositories", true, LIMITS.repositories + 1)]
    {
        let mut rows = List::with_capacity(count + 1);
        for index in 0..count {
            if repositories {
                let mut row = fixture::repository();
                row.repository.repository += index;
                rows.push(Stored::Repository(row)).expect("fixture capacity");
            } else {
                let mut row = fixture::live();
                row.watch.resource.what = What::Issue(u64::from(index));
                rows.push(Stored::Live(row)).expect("fixture capacity");
            }
        }
        if !repositories {
            rows.push(Stored::Repository(fixture::repository())).expect("fixture capacity");
        }
        fixture::failed_restore(name, rows.into_boxed());
    }
}
#[test]
fn restore_refuses_invalid_watches() {
    for (name, what, participating) in [
        ("empty branch", What::Branch(Box::new([])), false),
        ("long branch", What::Branch(fixture::repeated(b'a', LIMITS.op_bytes + 1)), false),
        ("participating repository", What::Repository, true),
        ("participating branch", kept::branch().what, true),
    ] {
        let mut row = fixture::live();
        row.watch = kept::watch(Resource { repository: REPO, what }, participating);
        fixture::bad_live(name, row);
    }
}
#[test]
fn keep_refuses_invalid_participation() {
    for what in [What::Repository, kept::branch().what] {
        let mut h = fixture::ready();
        let watch = kept::watch(Resource { repository: REPO, what }, true);
        assert_eq!(
            h.step(Event::Keep { owner: Token::new(1), watches: Box::new([watch]) }).as_ref(),
            &[Request::Kept { owner: Token::new(1), result: Err(Error::Refused) }]
        );
        assert!(h.send().is_none());
    }
}
#[test]
fn keep_refuses_invalid_branch_names() {
    for branch in [Box::from(&b""[..]), fixture::repeated(b'a', LIMITS.op_bytes + 1)] {
        let mut h = fixture::ready();
        let watch = kept::watch(Resource { repository: REPO, what: What::Branch(branch) }, false);
        assert_eq!(
            h.step(Event::Keep { owner: Token::new(1), watches: Box::new([watch]) }).as_ref(),
            &[Request::Kept { owner: Token::new(1), result: Err(Error::TooLarge) }]
        );
        assert!(h.domain.cached(&kept::branch()).is_none());
        assert!(h.send().is_none());
    }
}
#[test]
fn keep_is_busy_around_failed_restore() {
    let mut h = Harness::new(LIMITS);
    fixture::assert_busy(&mut h, "before restore");
    let mut bad = fixture::live();
    bad.comment_page = 0;
    h.step(Event::Restore { record: Stored::Live(bad) });
    fixture::paired(&mut h, fixture::live());
    h.step(Event::Restored { clock: RecoveryClock::Monotonic });
    fixture::assert_busy(&mut h, "corrected rows stay failed");
    h.step(Event::Read { owner: Token::new(1), repository: REPO, read: Read::Pull { number: 9 } });
    let sent = h.one();
    h.answer(&sent, 1, Ok(Answer::Pull(kept::current())));
    assert!(h.domain.restoration_failed());
    fixture::assert_busy(&mut h, "after successful read");
}
#[test]
fn oversized_hints_read_nothing() {
    for (name, resource, change, key) in [
        ("branch", kept::branch(), Change::Branch(fixture::repeated(b'a', LIMITS.op_bytes + 1)), None),
        ("key", kept::issue(), Change::Item(9), Some(fixture::repeated(b'a', LIMITS.op_bytes + 1))),
    ] {
        let mut h = kept::Harness::new();
        h.keep(Box::new([kept::watch(resource, false)]));
        h.clock();
        let sent = h.one();
        let answer = if name == "branch" { Answer::Commit([1; 32]) } else { fixture::empty_item() };
        h.answer(sent, answer);
        while h.domain.pop_fact().is_some() {}
        let deadline = h.domain.next_deadline();
        h.event(Event::Hint { hint: api::Hint { repository: REPO, change, key } });
        assert_eq!(h.domain.pop_fact(), Some(Fact::Refused), "{name}");
        assert_eq!(h.domain.pop_fact(), None, "{name}");
        assert_eq!(h.domain.next_deadline(), deadline, "{name}");
        assert!(h.send().is_none(), "{name}");
    }
}
#[test]
fn worst_case_rejects_invalid_limits() {
    for (name, limits) in [
        ("pending", Limits { pending: 0, ..LIMITS }),
        ("calls", Limits { calls: 0, ..LIMITS }),
        ("rate", Limits { rate: 0, ..LIMITS }),
        ("window", Limits { window: Duration::ZERO, ..LIMITS }),
        ("read attempts", Limits { read_attempts: 0, ..LIMITS }),
        ("backoff", Limits { backoff: Duration::ZERO, ..LIMITS }),
        ("entries", Limits { entries: 0, ..LIMITS }),
        ("write attempts", Limits { write_attempts: 0, ..LIMITS }),
        ("lifetime", Limits { lifetime: Duration::ZERO, ..LIMITS }),
        ("margin", Limits { clock_margin: Duration::ZERO, ..LIMITS }),
        ("rows", Limits { rows: 0, ..LIMITS }),
        ("inbox", Limits { inbox: 0, ..LIMITS }),
        ("resources", Limits { resources: 0, ..LIMITS }),
        ("repositories", Limits { repositories: 0, ..LIMITS }),
        ("poll", Limits { poll: Duration::ZERO, ..LIMITS }),
        ("hinted", Limits { hinted: Duration::ZERO, ..LIMITS }),
        ("slow", Limits { slow: Duration::ZERO, ..LIMITS }),
        ("settle", Limits { settle: Duration::ZERO, ..LIMITS }),
        ("parallel", Limits { calls: LIMITS.pending + 1, ..LIMITS }),
        ("reserve", Limits { reserve: LIMITS.rate + 1, ..LIMITS }),
        ("backoff order", Limits { backoff: LIMITS.backoff_max.saturating_add(Duration::from_secs(1)), ..LIMITS }),
        ("poll order", Limits { poll: LIMITS.poll_max.saturating_add(Duration::from_secs(1)), ..LIMITS }),
        ("backoff max", Limits { backoff_max: Duration::ZERO, ..LIMITS }),
        ("poll max", Limits { poll_max: Duration::ZERO, ..LIMITS }),
    ] {
        assert_eq!(crate::worst_case(&limits), None, "{name}");
    }
}
#[test]
fn worst_case_accepts_permitted_zeroes() {
    for (name, limits) in [
        ("op bytes", Limits { op_bytes: 0, ..LIMITS }),
        ("answer bytes", Limits { answer_bytes: 0, ..LIMITS }),
        ("reserve", Limits { reserve: 0, ..LIMITS }),
        ("facts", Limits { facts: 0, ..LIMITS }),
    ] {
        assert!(crate::worst_case(&limits).is_some(), "{name}");
    }
}
#[test]
fn max_out_covers_floor_and_batch() {
    let empty = Limits { resources: 0, repositories: 0, ..LIMITS };
    assert_eq!(crate::max_out(&empty), 4);
    assert_eq!(crate::max_out(&LIMITS), 15);
}
#[test]
fn invalid_reads_make_no_call() {
    for read in [
        Read::Items { since: at(100), page: 0, kind: None },
        Read::Reviews { number: 9, page: 0 },
        Read::Statuses { commit: [1; 32], page: 0 },
        Read::Remarks { number: 9, review: 1, page: 0 },
        Read::PullFiles { number: 9, head: [1; 32], page: 0 },
        Read::Collaborators { page: 0 },
        Read::File { head: [1; 32], path: Box::new([]), max_bytes: 1 },
        Read::File { head: [1; 32], path: Box::from(&b"kit"[..]), max_bytes: 0 },
        Read::File { head: [1; 32], path: Box::from(&b"kit"[..]), max_bytes: LIMITS.answer_bytes + 1 },
        Read::Job { attempt: api::JobAttempt { head: [1; 32], run: 1, job: 1, attempt: 1 }, max_bytes: 0 },
        Read::Job {
            attempt: api::JobAttempt { head: [1; 32], run: 1, job: 1, attempt: 1 },
            max_bytes: LIMITS.answer_bytes + 1,
        },
    ] {
        fixture::refused_read(read);
    }
}
#[test]
fn rate_refused_writes_save_a_new_attempt() {
    let mut h = writing::Harness::new();
    h.make(1, writing::comment(), Condition::None);
    h.clock();
    let sent = h.one();
    let saved = h.store.get(&1).expect("saved attempt").clone();
    assert_eq!(saved.attempt.expect("saved attempt").deadline, at(111));
    let call = sent.call;
    h.answer(sent, Err(Error::RateLimited { after: Duration::from_secs(2) }));
    h.env.wall = Wall::from_nanos(at(102).as_nanos());
    h.fire(102);
    let resent = h.one();
    assert_ne!(resent.call, call);
    assert_eq!(resent.op, Op::Write(writing::comment()));
    let entry = h.store.get(&1).expect("resent attempt");
    assert_eq!(entry.start, saved.start);
    assert_eq!(entry.failures, 0);
    assert_eq!(
        entry.attempt,
        Some(crate::Attempt {
            sent: at(102),
            deadline: at(113),
            wall: h.env.wall,
            expires: Wall::from_nanos(at(113).as_nanos())
        })
    );
    h.answer(resent, Err(Error::Timeout));
    let find = h.one();
    assert_eq!(find.op, Op::Read(Read::Item { number: 9, after: 0 }));
    assert_eq!(h.store.get(&1).expect("find keeps attempt").attempt.expect("attempt").deadline, at(113));
}
#[test]
fn attempted_or_unknown_withdrawals_are_noops() {
    let mut h = fixture::uncertain_comment();
    let find = h.one();
    while h.domain.pop_fact().is_some() {}
    h.outcomes.clear();
    for number in [1, 99] {
        let mut out = skein_lib::Queue::with_capacity(crate::max_out(&h.env.limits));
        crate::step(&mut h.domain, &h.env, Event::Withdraw { entry: number }, &mut out);
        assert!(out.is_empty(), "{number}");
        assert_eq!(h.domain.pop_fact(), None, "{number}");
    }
    let mut comment = kept::comment(9);
    comment.author = 1;
    comment.key = Some(crate::effect_key(b"kit", b":task:milestone", LIMITS.op_bytes).expect("key fits"));
    h.answer(find, Ok(Answer::Item { item: writing::summary(), comments: Box::new([comment]), more: false }));
    assert_eq!(h.outcomes.as_slice(), &[(1, Outcome::Made { made: Made::Commented(9), found: true })]);
    assert_eq!(h.writes, 1);
}
#[test]
fn echo_landing_rearms_the_resting_resource() {
    for (name, review) in [("comment", false), ("review", true)] {
        let (mut h, resource) = fixture::resting_echo(review);
        assert_eq!(h.domain.next_deadline(), Some(at(160)), "{name}");
        fixture::land_echo(&mut h, review);
        assert_eq!(
            h.record(&resource).echoes.as_ref(),
            &[if review { Echo::Review(9) } else { Echo::Comment(9) }],
            "{name}"
        );
        assert_eq!(h.domain.next_deadline(), Some(at(101)), "{name}");
        h.fire(101);
        let object = h.one();
        assert_eq!(object.op, Op::Read(Read::Item { number: 9, after: 0 }), "{name}");
        let mut own = kept::comment(9);
        own.author = 1;
        h.answer(
            object,
            Answer::Item {
                item: kept::summary(),
                comments: if review { Box::new([]) } else { Box::new([own]) },
                more: false,
            },
        );
        assert!(h.news.is_empty(), "{name}");
    }
}
#[test]
fn issue_find_requires_a_believed_key_without_provenance() {
    for (name, key, author, found) in [
        ("kit", &b"\x01\x00\x03kita"[..], 1, true),
        ("writer", &b"\x01\x00\x03kita"[..], 2, false),
        ("wrong", &b"\x01\x00\x03kitb"[..], 1, false),
        ("foreign", &b"\x01\x00\x03fooa"[..], 1, false),
        ("unframed", &b"a"[..], 1, false),
    ] {
        let expected = if name == "foreign" || name == "unframed" { key } else { &b"\x01\x00\x03kita"[..] };
        let mut h = writing::Harness::new();
        h.make(
            1,
            Write::CreateIssue { key: Box::from(expected), title: Box::new([]), body: Box::new([]) },
            Condition::None,
        );
        h.clock();
        let write = h.one();
        h.answer(write, Err(Error::Timeout));
        let mut item = writing::summary();
        item.key = Some(Box::from(key));
        item.author = author;
        let find = h.one();
        h.answer(find, Ok(Answer::Items { items: Box::new([item.clone()]), more: false, now: at(100) }));
        if found {
            assert_eq!(
                h.outcomes.as_slice().last(),
                Some(&(1, Outcome::Made { made: Made::Created(9), found: true })),
                "{name}"
            );
        } else {
            assert_eq!(h.outcomes.as_slice(), &[(1, Outcome::Uncertain)], "{name}");
            h.fire(111);
            let find = h.one();
            h.answer(find, Ok(Answer::Items { items: Box::new([item]), more: false, now: at(111) }));
            assert_eq!(h.outcomes.as_slice().last(), Some(&(1, Outcome::Held)), "{name}");
        }
        assert_eq!(h.writes, 1, "{name}");
    }
}
#[test]
fn comment_and_review_finds_require_believed_original_keys() {
    for (name, key) in
        [("kit", &b"\x01\x00\x03kita"[..]), ("foreign", &b"\x01\x00\x03fooa"[..]), ("unframed", &b"a"[..])]
    {
        for review in [false, true] {
            let mut h = fixture::recovering_key(review, key);
            let answer = fixture::keyed_answer(review, key);
            let find = h.one();
            h.answer(find, Ok(answer.clone()));
            if name == "kit" {
                let made = if review { Made::Reviewed(9) } else { Made::Commented(9) };
                assert_eq!(
                    h.outcomes.as_slice().last(),
                    Some(&(1, Outcome::Made { made, found: true })),
                    "{name} review={review}"
                );
            } else {
                assert_eq!(h.outcomes.as_slice(), &[(1, Outcome::Uncertain)], "{name} review={review}");
                h.fire(111);
                let find = h.one();
                h.answer(find, Ok(answer));
                assert_eq!(h.outcomes.as_slice().last(), Some(&(1, Outcome::Held)), "{name} review={review}");
            }
            assert_eq!(h.writes, 1, "{name} review={review}");
        }
    }
}
#[test]
fn oversized_and_lost_write_answers_are_uncertain() {
    let branches = Answer::Branches(Box::new([Box::from(&b"kit"[..])]));
    assert!(crate::bounds::answer(&branches, &Limits { answer_bytes: 19, ..LIMITS }));
    assert!(!crate::bounds::answer(&branches, &Limits { answer_bytes: 18, ..LIMITS }));
    for (name, answer, error) in
        [("oversized", Ok(branches), Error::InvalidAnswer), ("lost", Err(Error::Timeout), Error::Timeout)]
    {
        let mut h = writing::Harness::new();
        h.env.limits.answer_bytes = 18;
        h.make(1, writing::comment(), Condition::None);
        h.clock();
        let write = h.one();
        h.answer(write, answer);
        fixture::failed_fact(&mut h, error);
        assert_eq!(h.store.get(&1).expect("uncertain entry saved").failures, 1, "{name}");
        assert_eq!(h.outcomes.as_slice(), &[(1, Outcome::Uncertain)], "{name}");
        assert_eq!(h.one().op, Op::Read(Read::Item { number: 9, after: 0 }), "{name}");
        assert_eq!(h.writes, 1, "{name}");
    }
}
#[test]
fn exists_after_a_write_finds_under_its_attempt() {
    let mut h = writing::Harness::new();
    h.make(
        1,
        Write::OpenPull {
            head: Box::from(&b"topic"[..]),
            base: Box::from(&b"main"[..]),
            title: Box::new([]),
            body: Box::new([]),
        },
        Condition::None,
    );
    h.clock();
    let write = h.one();
    let saved = h.store.get(&1).expect("durable attempt").clone();
    assert_eq!(saved.attempt.expect("attempt").deadline, at(111));
    h.answer(write, Err(Error::Exists));
    let find = h.one();
    assert_eq!(find.op, Op::Read(Read::PullFor { head: Box::from(&b"topic"[..]), base: Box::from(&b"main"[..]) }));
    assert_eq!(h.store.get(&1).expect("find retains entry"), &saved);
    h.answer(find, Ok(Answer::Pull(kept::current())));
    assert_eq!(h.outcomes.as_slice(), &[(1, Outcome::Made { made: Made::Created(9), found: true })]);
    assert_eq!(h.writes, 1);
}
