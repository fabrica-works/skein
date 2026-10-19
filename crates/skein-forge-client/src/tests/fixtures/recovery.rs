use super::{Harness, at, configured};
use crate::tests::{LIMITS, REPO};
use crate::{Delivery, Event, Fact, LiveRecord, RecoveryClock, RepositoryRecord, Request, Stored};
use alloc::boxed::Box;
use skein_forge::api::{Answer, Error, Read};
use skein_lib::{List, Token};

pub(in crate::tests) fn repository() -> RepositoryRecord {
    RepositoryRecord { repository: REPO, mark: Some(at(100)), clock: at(100) }
}
pub(in crate::tests) fn live() -> LiveRecord {
    super::keep::measure_row()
}
pub(in crate::tests) fn ready() -> Harness {
    let mut h = Harness::new(LIMITS);
    h.domain = configured(&LIMITS);
    assert!(h.step(Event::Restored { clock: RecoveryClock::Monotonic }).is_empty());
    h
}
pub(in crate::tests) fn paired(h: &mut Harness, row: LiveRecord) {
    h.step(Event::Restore { record: Stored::Live(row) });
    h.step(Event::Restore { record: Stored::Repository(repository()) });
}
pub(in crate::tests) fn failed_restore(name: &str, rows: Box<[Stored]>) {
    let mut h = Harness::new(LIMITS);
    for record in rows {
        h.step(Event::Restore { record });
    }
    h.step(Event::Restored { clock: RecoveryClock::Monotonic });
    assert!(h.domain.restoration_failed(), "{name}");
    assert_busy(&mut h, name);
    let mut fresh = Harness::new(LIMITS);
    paired(&mut fresh, live());
    fresh.step(Event::Restored { clock: RecoveryClock::Monotonic });
    assert!(!fresh.domain.restoration_failed(), "corrected {name}");
    assert_eq!(
        fresh.step(Event::Keep { owner: Token::new(1), watches: Box::new([live().watch]) }).as_ref(),
        &[Request::Kept { owner: Token::new(1), result: Ok(()) }],
        "corrected {name}"
    );
    assert!(h.domain.restoration_failed(), "failed instance {name}");
}
pub(in crate::tests) fn bad_live(name: &str, row: LiveRecord) {
    failed_restore(name, Box::new([Stored::Live(row), Stored::Repository(repository())]));
}
pub(in crate::tests) fn assert_busy(h: &mut Harness, name: &str) {
    assert_eq!(
        h.step(Event::Keep { owner: Token::new(1), watches: Box::new([live().watch]) }).as_ref(),
        &[Request::Kept { owner: Token::new(1), result: Err(Error::Busy) }],
        "{name}"
    );
}
pub(in crate::tests) fn deliveries(count: u32, duplicate: bool) -> Box<[Delivery]> {
    let mut rows = List::with_capacity(count);
    for id in 1..=count {
        rows.push(Delivery { id: if duplicate { 1 } else { u64::from(id) }, revision: 1 }).expect("fixture capacity");
    }
    rows.into_boxed()
}
pub(in crate::tests) fn refused_read(read: Read) {
    let mut h = Harness::new(LIMITS);
    assert_eq!(
        h.step(Event::Read { owner: Token::new(1), repository: REPO, read: read.clone() }).as_ref(),
        &[Request::Read { owner: Token::new(1), result: Err(Error::TooLarge) }],
        "{read:?}"
    );
    assert!(h.send().is_none());
}
pub(in crate::tests) fn uncertain_comment() -> super::outbox::Harness {
    let mut h = super::outbox::Harness::new();
    h.make(1, super::outbox::comment(), crate::Condition::None);
    h.clock();
    let write = h.one();
    h.answer(write, Err(Error::Timeout));
    h
}
pub(in crate::tests) fn empty_item() -> Answer {
    Answer::Item { item: super::outbox::summary(), comments: Box::new([]), more: false }
}
pub(in crate::tests) fn failed_fact(h: &mut super::outbox::Harness, error: Error) {
    let mut failures = List::with_capacity(LIMITS.facts);
    for _ in 0..LIMITS.facts {
        match h.domain.pop_fact() {
            Some(Fact::Failed { error, .. }) => failures.push(error).expect("bounded facts"),
            Some(Fact::Sent { .. } | Fact::Refused | Fact::Spent { .. } | Fact::Limited { .. }) => {}
            None => break,
        }
    }
    assert_eq!(failures.into_boxed().as_ref(), &[error]);
}

pub(in crate::tests) fn repeated<T: Clone>(value: T, count: u32) -> Box<[T]> {
    let mut values = List::with_capacity(count);
    for _ in 0..count {
        assert!(values.push(value.clone()).is_ok());
    }
    values.into_boxed()
}

pub(in crate::tests) fn keyed_write(review: bool, key: &[u8]) -> crate::Effect {
    let write = if review {
        skein_forge::api::Write::Review {
            number: 9,
            key: Box::from(key),
            verdict: skein_forge::api::Verdict::Approve,
            body: Box::new([]),
        }
    } else {
        skein_forge::api::Write::Post { number: 9, key: Box::from(key), body: Box::new([]) }
    };
    crate::Effect {
        write,
        condition: if review { crate::Condition::Review { head: [1; 32] } } else { crate::Condition::None },
    }
}
pub(in crate::tests) fn keyed_answer(review: bool, key: &[u8]) -> Answer {
    if review {
        let mut row = super::keep::review(9, 100);
        row.author = 1;
        row.key = Some(Box::from(key));
        Answer::Reviews { reviews: Box::new([row]), more: false }
    } else {
        let mut row = super::keep::comment(9);
        row.author = 1;
        row.key = Some(Box::from(key));
        Answer::Item { item: super::outbox::summary(), comments: Box::new([row]), more: false }
    }
}
pub(in crate::tests) fn recovering_key(review: bool, key: &[u8]) -> super::outbox::Harness {
    let mut h = super::outbox::Harness::new();
    let effect = keyed_write(review, key);
    h.make(1, effect.write, effect.condition);
    h.clock();
    if review {
        let check = h.one();
        h.answer(check, Ok(Answer::Pull(super::keep::current())));
    }
    let write = h.one();
    h.answer(write, Err(Error::Timeout));
    h
}
pub(in crate::tests) fn resting_echo(review: bool) -> (super::keep::Harness, crate::Resource) {
    let mut h = super::keep::Harness::with_limits(crate::Limits {
        poll: skein_lib::Duration::from_secs(60),
        rate: 100,
        reserve: 0,
        ..LIMITS
    });
    let resource = if review { super::keep::pull() } else { super::keep::issue() };
    h.keep(Box::new([super::keep::watch(resource.clone(), true)]));
    h.clock();
    if review {
        super::keep::participation(&mut h, Box::new([]));
    } else {
        let sent = h.one();
        h.answer(sent, Answer::Item { item: super::keep::summary(), comments: Box::new([]), more: false });
    }
    (h, resource)
}
pub(in crate::tests) fn land_echo(h: &mut super::keep::Harness, review: bool) {
    let effect = keyed_write(review, b"\x01\x00\x03kita");
    let mut entry = super::entry(effect.write);
    entry.effect.condition = effect.condition;
    h.event(Event::Make { entry });
    h.clock();
    if review {
        let check = h.one();
        h.answer(check, Answer::Pull(super::keep::current()));
    }
    let sent = h.one();
    h.news.clear();
    h.answer(sent, if review { Answer::Reviewed(9) } else { Answer::Commented(9) });
    if review {
        let find = h.one();
        h.answer(find, keyed_answer(true, b"\x01\x00\x03kita"));
    }
}
