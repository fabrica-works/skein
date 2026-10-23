//! GitHub's bounded inputs, state life, deadline and wait (forge-setup.md, 5.1 to 5.7).
use super::given::{Place, Refused};
use skein_lib::Duration;
/// Limits checked before a GitHub plan is read (forge-setup.md, 5.1 to 5.7).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Limits {
    pub key_bytes: u32,        // The App's key, DER: at most.
    pub name_bytes: u32,       // The App's slug: at most.
    pub page_bytes: u32,       // Each form target and manifest JSON: at most.
    pub installations: u32,    // Accepted installations: at most.
    pub reachable: u32,        // Rows of one picker page: at most.
    pub pages: u32,            // Pages of GET /app/installations a reconcile reads: at most.
    pub state_life: Duration,  // A state's life, and a stray's age.
    pub deadline: Duration,    // Whole run.
    pub backoff: Duration,     // First retry delay.
    pub backoff_max: Duration, // Largest retry delay.
}
/// A state's life is at most the conversion code's hour (forge-setup.md, 5.7).
pub const STATE_LIFE_MAX: Duration = Duration::from_secs(3600);
/// Checks every limit and both relations, naming the offending pair.
pub fn check(limits: &Limits) -> Result<(), Refused> {
    for (zero, place) in [
        (limits.key_bytes == 0, Place::KeyBytes),
        (limits.name_bytes == 0, Place::NameBytes),
        (limits.page_bytes == 0, Place::PageBytes),
        (limits.installations == 0, Place::Installations),
        (limits.reachable == 0, Place::Reachable),
        (limits.pages == 0, Place::Pages),
        (limits.state_life == Duration::ZERO, Place::StateLife),
        (limits.deadline == Duration::ZERO, Place::Deadline),
        (limits.backoff == Duration::ZERO, Place::Backoff),
        (limits.backoff_max == Duration::ZERO, Place::BackoffMax),
    ] {
        if zero {
            return Err(Refused { place, other: None });
        }
    }
    if limits.state_life > STATE_LIFE_MAX {
        return Err(Refused { place: Place::StateLife, other: None });
    }
    if limits.backoff > limits.backoff_max {
        return Err(Refused { place: Place::Backoff, other: Some(Place::BackoffMax) });
    }
    if limits.backoff_max >= limits.deadline {
        return Err(Refused { place: Place::BackoffMax, other: Some(Place::Deadline) });
    }
    Ok(())
}
