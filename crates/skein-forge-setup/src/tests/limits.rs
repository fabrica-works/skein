use super::fixtures;
use crate::{
    given::{Place, Refused},
    limits,
};
use skein_lib::Duration;
// A relation's adjacent values differ by one nanosecond (forge-setup.md, 4.4).
const ONE_NS: Duration = Duration::from_nanos(1);
#[test]
fn limits_at_both_relations_are_accepted() {
    let mut equality = fixtures::limits();
    equality.backoff = equality.backoff_max;
    let mut below = fixtures::limits();
    below.backoff_max = Duration::from_nanos(below.deadline.as_nanos() - ONE_NS.as_nanos());
    for (label, value) in [("backoff equality", equality), ("maximum below deadline", below)] {
        assert_eq!(limits::check(&value), Ok(()), "{label}");
    }
}
#[test]
fn a_limit_one_past_its_relation_names_the_pair() {
    let mut backoff = fixtures::limits();
    backoff.backoff = backoff.backoff_max.checked_add(ONE_NS).unwrap();
    let mut maximum = fixtures::limits();
    maximum.backoff_max = maximum.deadline;
    for (label, value, place, other) in [
        ("backoff above maximum", backoff, Place::Backoff, Place::BackoffMax),
        ("maximum at deadline", maximum, Place::BackoffMax, Place::Deadline),
    ] {
        assert_eq!(limits::check(&value), Err(Refused { place, other: Some(other) }), "{label}");
    }
}
#[test]
fn the_first_broken_relation_wins() {
    let mut value = fixtures::limits();
    value.backoff = value.deadline.checked_add(Duration::from_secs(1)).unwrap();
    value.backoff_max = value.deadline;
    assert_eq!(limits::check(&value), Err(Refused { place: Place::Backoff, other: Some(Place::BackoffMax) }));
}
