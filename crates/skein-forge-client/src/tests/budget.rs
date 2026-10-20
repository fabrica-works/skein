use super::{Harness, LIMITS, at, terminal};
use crate::{Fact, Limits, Priority};
use skein_forge::api::{self, Error, Op, Read};
use skein_lib::{Duration, Wall};

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

#[test]
fn a_forge_window_below_the_budgets_becomes_the_budgets() {
    let mut h = Harness::new(LIMITS);
    h.read(1);
    h.read(2);
    let first = h.one();
    assert_eq!(h.domain.calls.remaining(), (3, 1));
    drop(h.answer_rate(&first, 1, h.rate(1, 0), Err(Error::Forbidden)));
    assert_eq!(h.domain.calls.remaining(), (1, 1));
    assert_eq!(h.one().op, Op::Read(Read::Pull { number: 2 }));
    assert!(h.facts().contains(&Fact::Spent { until: at(160) }));
    h.read(3);
    assert!(h.send().is_none());
}

#[test]
fn a_forge_window_at_or_above_the_budgets_changes_nothing() {
    for remaining in [3, 5000] {
        let mut h = Harness::new(LIMITS);
        h.read(1);
        let sent = h.one();
        assert_eq!(h.domain.calls.remaining(), (3, 1));
        drop(h.answer_rate(&sent, 1, h.rate(remaining, 30), Err(Error::Forbidden)));
        assert_eq!(h.domain.calls.remaining(), (3, 1));
    }
}

#[test]
fn an_answer_without_a_rate_changes_nothing() {
    let mut h = Harness::new(LIMITS);
    h.read(1);
    let sent = h.one();
    drop(h.answer(&sent, 1, Err(Error::Forbidden)));
    assert_eq!(h.domain.calls.remaining(), (3, 1));
}

#[test]
fn the_least_remaining_of_several_installations_wins() {
    let mut h = Harness::new(Limits { rate: 100, reserve: 0, ..LIMITS });
    for owner in 1..=3 {
        h.read(owner);
    }
    let first = h.one();
    let second = h.one();
    let third = h.one();
    for (sent, remaining, left) in [(&first, 50, 50), (&second, 70, 50), (&third, 20, 20)] {
        drop(h.answer_rate(sent, 1, h.rate(remaining, 30), Err(Error::Forbidden)));
        assert_eq!(h.domain.calls.remaining(), (left, 3));
    }
}

#[test]
fn a_forge_window_of_zero_holds_every_class_until_its_reset() {
    for priority in [Priority::Fresh, Priority::Write, Priority::Keep, Priority::Slow] {
        let mut h = Harness::new(LIMITS);
        h.queued(1, priority);
        let sent = h.one();
        let before = h.domain.calls.remaining();
        drop(h.answer_rate(&sent, 1, h.rate(0, 30), Err(Error::Forbidden)));
        assert_eq!(h.domain.calls.remaining(), before);
        h.queued(2, priority);
        assert!(h.send().is_none());
        assert!(h.facts().contains(&Fact::Limited { reset: at(130) }));
        assert_eq!(h.domain.next_deadline(), Some(at(130)));
        h.env.now = at(129);
        h.fire();
        assert!(h.send().is_none());
        h.env.now = at(130);
        h.fire();
        assert!(h.send().is_some());
    }
}

#[test]
fn the_forge_reset_is_at_most_a_window_on() {
    let mut h = Harness::new(LIMITS);
    h.read(1);
    let sent = h.one();
    drop(h.answer_rate(&sent, 1, h.rate(0, 10_000), Err(Error::Forbidden)));
    h.read(2);
    assert!(h.send().is_none());
    assert_eq!(h.domain.next_deadline(), Some(at(160)));
    assert!(h.facts().contains(&Fact::Limited { reset: at(160) }));
}

#[test]
fn a_forge_reset_not_after_the_wall_holds_nothing() {
    for reset_secs in [200, 199] {
        let mut h = Harness::new(LIMITS);
        h.env.wall = Wall::from_nanos(at(200).as_nanos());
        h.read(1);
        let sent = h.one();
        let rate = api::Rate { limit: 5000, remaining: 0, reset_at: Wall::from_nanos(at(reset_secs).as_nanos()) };
        drop(h.answer_rate(&sent, 1, rate, Err(Error::Forbidden)));
        h.read(2);
        assert!(h.send().is_some());
        for fact in h.facts() {
            match fact {
                Fact::Limited { .. } => panic!("a reset at or before the wall holds nothing"),
                Fact::Sent { .. } | Fact::Failed { .. } | Fact::Spent { .. } | Fact::Refused => {}
            }
        }
    }
}

#[test]
fn the_later_of_two_forge_resets_is_kept() {
    for (first_reset, second_reset) in [(40, 10), (10, 40)] {
        let mut h = Harness::new(LIMITS);
        h.read(1);
        h.read(2);
        let first = h.one();
        let second = h.one();
        drop(h.answer_rate(&first, 1, h.rate(0, first_reset), Err(Error::Forbidden)));
        drop(h.answer_rate(&second, 1, h.rate(0, second_reset), Err(Error::Forbidden)));
        assert_eq!(h.domain.next_deadline(), Some(at(140)));
    }
}

#[test]
fn a_forge_reset_and_a_rate_refusal_keep_the_later() {
    for refusal_first in [true, false] {
        let mut h = Harness::new(LIMITS);
        h.read(1);
        h.read(2);
        let first = h.one();
        let second = h.one();
        let refusal = Err(Error::RateLimited { after: Duration::from_secs(40) });
        if refusal_first {
            drop(h.answer(&first, 1, refusal));
            drop(h.answer_rate(&second, 1, h.rate(0, 10), Err(Error::Forbidden)));
        } else {
            drop(h.answer_rate(&first, 1, h.rate(0, 10), Err(Error::Forbidden)));
            drop(h.answer(&second, 1, refusal));
        }
        assert_eq!(h.domain.next_deadline(), Some(at(140)));
    }
}

#[test]
fn a_forge_window_reported_after_the_budgets_ended_applies_to_the_new_one() {
    let mut h = Harness::new(LIMITS);
    h.read(1);
    let sent = h.one();
    h.env.now = at(161);
    drop(h.answer_rate(&sent, 1, h.rate(1, 0), Err(Error::Forbidden)));
    assert_eq!(h.domain.calls.remaining(), (1, 0));
    h.read(2);
    assert_eq!(h.one().op, Op::Read(Read::Pull { number: 2 }));
}

#[test]
fn a_failed_answer_reports_its_rate_too() {
    for error in [Error::Forbidden, Error::Timeout] {
        let mut h = Harness::new(LIMITS);
        h.read(1);
        let sent = h.one();
        let out = h.answer_rate(&sent, 1, h.rate(1, 30), Err(error));
        assert_eq!(h.domain.calls.remaining(), (1, 1));
        if error == Error::Forbidden {
            terminal(&out, 1, error);
        }
    }
}

#[test]
fn a_refund_after_the_forge_capped_the_window_stays_under_the_cap() {
    let mut h = Harness::new(LIMITS);
    h.read(1);
    h.read(2);
    let first = h.one();
    let second = h.one();
    assert_eq!(h.domain.calls.remaining(), (2, 2));
    drop(h.answer_rate(&first, 1, h.rate(1, 30), Err(Error::Forbidden)));
    assert_eq!(h.domain.calls.remaining(), (1, 2));
    drop(h.answer_free(&second, 1, 1, Err(Error::Forbidden)));
    assert_eq!(h.domain.calls.remaining(), (1, 1));
}

#[test]
fn a_forge_figure_equal_to_what_is_left_still_caps_later_refunds() {
    let mut h = Harness::new(LIMITS);
    h.read(1);
    h.read(2);
    let first = h.one();
    let second = h.one();
    assert_eq!(h.domain.calls.remaining(), (2, 2));
    drop(h.answer_rate(&first, 1, h.rate(2, 30), Err(Error::Forbidden)));
    assert_eq!(h.domain.calls.remaining(), (2, 2));
    drop(h.answer_free(&second, 1, 1, Err(Error::Forbidden)));
    assert_eq!(h.domain.calls.remaining(), (2, 1));
}

#[test]
fn the_last_unit_the_forge_left_holds_every_class_until_its_reset() {
    let mut h = Harness::new(LIMITS);
    for owner in 1..=3 {
        h.read(owner);
    }
    let first = h.one();
    drop(h.answer_rate(&first, 1, h.rate(1, 30), Err(Error::Forbidden)));
    assert_eq!(h.domain.calls.remaining(), (1, 1));
    let second = h.one();
    let facts = h.facts();
    assert!(facts.contains(&Fact::Limited { reset: at(130) }));
    for fact in facts {
        match fact {
            Fact::Spent { .. } => panic!("the forge's reset governs the capped window"),
            Fact::Sent { .. } | Fact::Failed { .. } | Fact::Limited { .. } | Fact::Refused => {}
        }
    }
    assert_eq!(h.domain.next_deadline(), Some(at(130)));
    assert!(h.send().is_none());
    drop(h.answer_rate(&second, 1, h.rate(0, 30), Err(Error::Forbidden)));
    assert_eq!(h.domain.next_deadline(), Some(at(130)));
    h.env.now = at(129);
    h.fire();
    assert!(h.send().is_none());
    h.env.now = at(130);
    h.fire();
    assert_eq!(h.one().op, Op::Read(Read::Pull { number: 3 }));
    assert_eq!(h.domain.calls.remaining(), (3, 1));
}

#[test]
fn a_forge_reset_does_not_end_a_window_the_client_spent_alone() {
    let mut h = Harness::new(LIMITS);
    for owner in 1..=5 {
        h.read(owner);
    }
    let first = h.one();
    for _ in 0..3_u32 {
        drop(h.one());
    }
    assert!(h.facts().contains(&Fact::Spent { until: at(160) }));
    drop(h.answer_rate(&first, 1, h.rate(0, 30), Err(Error::Forbidden)));
    assert_eq!(h.domain.next_deadline(), Some(at(130)));
    h.env.now = at(130);
    h.fire();
    assert!(h.send().is_none());
    h.env.now = at(160);
    h.fire();
    assert_eq!(h.one().op, Op::Read(Read::Pull { number: 5 }));
}

#[test]
fn a_cap_whose_reset_has_passed_is_spent_until_the_windows_end() {
    let mut h = Harness::new(LIMITS);
    for owner in 1..=3 {
        h.read(owner);
    }
    let first = h.one();
    drop(h.answer_rate(&first, 1, h.rate(1, 30), Err(Error::Forbidden)));
    h.env.now = at(131);
    assert_eq!(h.one().op, Op::Read(Read::Pull { number: 2 }));
    let facts = h.facts();
    assert!(facts.contains(&Fact::Spent { until: at(160) }));
    for fact in facts {
        match fact {
            Fact::Limited { .. } => panic!("the cap's reset has passed"),
            Fact::Sent { .. } | Fact::Failed { .. } | Fact::Spent { .. } | Fact::Refused => {}
        }
    }
    assert!(h.send().is_none());
}
