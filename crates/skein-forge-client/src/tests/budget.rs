use super::{Harness, LIMITS, at, terminal};
use crate::{Limits, Priority};
use skein_forge::api::{Error, Op, Read};

#[test]
fn a_304_is_charged_nothing() {
    for priority in [Priority::Fresh, Priority::Write, Priority::Keep, Priority::Slow] {
        let mut h = Harness::new(LIMITS);
        h.queued(1, priority);
        let sent = h.one();
        terminal(&h.answer_free(&sent, 1, 1, Err(Error::Forbidden)), 1, Error::Forbidden);
        assert_eq!(h.domain.calls.remaining(), (4, 0));
    }
}

#[test]
fn a_free_answer_settles_as_the_cost_less_its_free() {
    for (cost, free) in [(0_u32, 0_u32), (1, 0), (1, 1), (4, 0), (4, 1), (4, 3), (4, 4)] {
        let mut h = Harness::new(LIMITS);
        h.read(1);
        let sent = h.one();
        drop(h.answer_free(&sent, cost, free, Err(Error::Forbidden)));

        let mut twin = Harness::new(LIMITS);
        twin.read(1);
        let sent = twin.one();
        drop(twin.answer(&sent, cost.saturating_sub(free), Err(Error::Forbidden)));
        assert_eq!(h.domain.calls.remaining(), twin.domain.calls.remaining());
        if cost == 4 && free == 1 {
            assert_eq!(h.domain.calls.remaining(), (1, 3));
        }
        if cost == 4 && free == 3 {
            assert_eq!(h.domain.calls.remaining(), (3, 1));
        }
    }
}

#[test]
fn free_above_cost_is_clamped() {
    for (cost, free) in [(1, 5), (0, 3)] {
        let mut h = Harness::new(LIMITS);
        h.read(1);
        let sent = h.one();
        drop(h.answer_free(&sent, cost, free, Err(Error::Forbidden)));

        let mut twin = Harness::new(LIMITS);
        twin.read(1);
        let sent = twin.one();
        drop(twin.answer_free(&sent, cost, cost, Err(Error::Forbidden)));
        assert_eq!(h.domain.calls.remaining(), twin.domain.calls.remaining());
    }
}

#[test]
fn a_late_free_answer_cannot_credit_the_new_window() {
    let mut h = Harness::new(LIMITS);
    h.read(1);
    let old = h.one();
    h.env.now = at(161);
    h.read(2);
    let current = h.one();
    assert_eq!(h.domain.calls.remaining(), (3, 1));
    drop(h.answer_free(&old, 1, 1, Err(Error::Forbidden)));
    assert_eq!(h.domain.calls.remaining(), (3, 1));
    drop(h.answer(&current, 1, Err(Error::Forbidden)));
}

#[test]
fn a_free_answer_reopens_the_last_reservation() {
    let mut h = Harness::new(Limits { rate: 1, reserve: 0, ..LIMITS });
    h.read(1);
    let sent = h.one();
    h.read(2);
    assert!(h.send().is_none());
    terminal(&h.answer_free(&sent, 1, 1, Err(Error::Forbidden)), 1, Error::Forbidden);
    assert_eq!(h.domain.calls.remaining(), (1, 0));
    assert_eq!(h.one().op, Op::Read(Read::Pull { number: 2 }));
}
