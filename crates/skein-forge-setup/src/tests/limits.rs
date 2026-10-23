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

#[test]
fn a_zero_reachable_limit_is_refused() {
    let mut value = fixtures::limits();
    value.reachable = 0;
    assert_eq!(limits::check(&value), Err(Refused { place: Place::ReachableLimit, other: None }));
}
#[test]
fn zero_limits_are_refused_in_design_order() {
    let mut value = fixtures::limits();
    value.orgs = 0;
    value.repositories = 0;
    value.reachable = 0;
    value.deadline = Duration::ZERO;
    value.backoff = Duration::ZERO;
    value.backoff_max = Duration::ZERO;
    for place in [
        Place::OrgsLimit,
        Place::RepositoriesLimit,
        Place::ReachableLimit,
        Place::Deadline,
        Place::Backoff,
        Place::BackoffMax,
    ] {
        assert_eq!(limits::check(&value), Err(Refused { place, other: None }));
        match place {
            Place::OrgsLimit => value.orgs = 1,
            Place::RepositoriesLimit => value.repositories = 1,
            Place::ReachableLimit => value.reachable = 1,
            Place::Deadline => value.deadline = fixtures::limits().deadline,
            Place::Backoff => value.backoff = fixtures::limits().backoff,
            Place::BackoffMax => value.backoff_max = fixtures::limits().backoff_max,
            Place::Word
            | Place::BotLogin
            | Place::BotEmail
            | Place::AdminName
            | Place::ApiName
            | Place::GitName
            | Place::Orgs
            | Place::Org(_)
            | Place::OrgOwner
            | Place::Repositories
            | Place::Repository(_)
            | Place::FilesLimit
            | Place::FileBytesLimit
            | Place::CreateOrg
            | Place::CreateRepository
            | Place::Files
            | Place::FilePath(_)
            | Place::FileContent(_)
            | Place::Plan
            | Place::Page => unreachable!("limit rows"),
        }
    }
}
