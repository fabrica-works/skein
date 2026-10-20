//! Reconcile's bounded orgs, repositories, deadline and wait (forge-setup.md, 4.4).
use crate::given::{Place, Refused};
use skein_lib::Duration;
/// Limits checked before a setup is read.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Limits {
    pub orgs: u32,             // Orgs per forge.
    pub repositories: u32,     // Repositories adopted per forge.
    pub deadline: Duration,    // Whole reconcile run.
    pub backoff: Duration,     // First retry delay.
    pub backoff_max: Duration, // Largest retry delay.
}
/// Checks both relationships, naming the offending pair.
pub fn check(limits: &Limits) -> Result<(), Refused> {
    for (zero, place) in [
        (limits.orgs == 0, Place::OrgsLimit),
        (limits.repositories == 0, Place::RepositoriesLimit),
        (limits.deadline == Duration::ZERO, Place::Deadline),
        (limits.backoff == Duration::ZERO, Place::Backoff),
        (limits.backoff_max == Duration::ZERO, Place::BackoffMax),
    ] {
        if zero {
            return Err(Refused { place, other: None });
        }
    }
    if limits.backoff > limits.backoff_max {
        return Err(Refused { place: Place::Backoff, other: Some(Place::BackoffMax) });
    }
    if limits.backoff_max >= limits.deadline {
        return Err(Refused { place: Place::BackoffMax, other: Some(Place::Deadline) });
    }
    Ok(())
}
