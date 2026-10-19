//! Reconcile's bounded orgs, deadline and wait (forge-setup.md, 4.4).
use crate::given::{Place, Refused};
use skein_lib::Duration;
/// Limits checked before a setup is read.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Limits {
    pub orgs: u32,             // Orgs per forge.
    pub deadline: Duration,    // Whole reconcile run.
    pub backoff: Duration,     // First retry delay.
    pub backoff_max: Duration, // Largest retry delay.
}
/// Checks both relationships, naming the offending pair.
pub fn check(limits: &Limits) -> Result<(), Refused> {
    if limits.backoff > limits.backoff_max {
        return Err(Refused { place: Place::Backoff, other: Some(Place::BackoffMax) });
    }
    if limits.backoff_max >= limits.deadline {
        return Err(Refused { place: Place::BackoffMax, other: Some(Place::Deadline) });
    }
    Ok(())
}
