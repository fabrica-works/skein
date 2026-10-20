//! Forge component start (forge-connection.md, sections 2, 3.5, 7 and 8).
use super::boundary::{StartRead, Stopped};
use super::limits::Limits;
use skein_forge::forgejo::plans::start::Heard;
use skein_lib::{Duration, Time};

/// Where a forge's start is; `next` is the wait the next unanswered read takes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Start {
    /// No access: no start runs, and calls are answered `NotConnected`.
    Unconnected,
    /// `GET /api/v1/user` sent or to send.
    Checking { next: Duration },
    /// `GET /api/v1/settings/api` sent or to send.
    Settings { next: Duration },
    /// GitHub's App installations read.
    Listing { next: Duration },
    /// GitHub's accepted installations' Full and Contents mints.
    Minting { next: Duration },
    /// GitHub's bot viewer read on an API token.
    Identity { next: Duration },
    /// Asked again at `until`, from Checking or GitHub's Listing.
    Waiting { until: Time, next: Duration },
    /// Serves calls; `cap` is the forge's page cap.
    Ready { cap: u32 },
    /// Says stopped once, naming the forge, and holds no access until the next connect.
    Stopped(Stopped),
}

/// The start's next state after `heard` (forge-connection.md, 8, "A forge's start").
#[must_use]
pub fn heard(start: Start, heard: Heard, user: u64, now: Time, limits: &Limits) -> Start {
    let (read, next) = match start {
        Start::Checking { next } => (StartRead::User, next),
        Start::Settings { next } => (StartRead::ApiSettings, next),
        Start::Listing { .. }
        | Start::Minting { .. }
        | Start::Identity { .. }
        | Start::Unconnected
        | Start::Waiting { .. }
        | Start::Ready { .. }
        | Start::Stopped(_) => return start,
    };
    let doubled = next.saturating_mul(2).min(limits.start_backoff_max);
    match heard {
        Heard::User { id } if read == StartRead::User && id == user => Start::Settings { next },
        Heard::User { id } if read == StartRead::User => Start::Stopped(Stopped::OtherUser { user: id }),
        Heard::ApiSettings { max_response_items } if read == StartRead::ApiSettings && max_response_items > 0 => {
            Start::Ready { cap: max_response_items }
        }
        Heard::Status { status, .. } if status != 429 && (400..500).contains(&status) => {
            Start::Stopped(Stopped::Refused { read, status })
        }
        Heard::Status { retry: Some(after), .. } if after > next => {
            Start::Waiting { until: now.saturating_add(after), next: doubled }
        }
        Heard::User { .. } | Heard::ApiSettings { .. } | Heard::Status { .. } | Heard::Unanswered => {
            Start::Waiting { until: now.saturating_add(next), next: doubled }
        }
    }
}
