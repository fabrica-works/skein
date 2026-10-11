//! Per-call deadlines (llm-connection.md, section 6). This table stores the
//! five absolute monotonic times, activity anchors and terminal measures.
//! It never reads a clock: the owner supplies one `Env` snapshot to every
//! entry point. `due` names a phase that must end through the LLM client;
//! `sent`, `response_bytes` and `activity` record only existing stream observations.

use skein_lib::Time;

use crate::boundary::{Deadlines, Measures};

/// The call or connection phase, selecting all armed deadlines in one place.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Phase {
    Waiting,
    Connecting,
    Handshaking,
    Head,
    Streaming,
    Draining,
    Idle,
    Closing,
    Closed,
}

/// Which deadline passed: a call deadline or an idle connection keep.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Due {
    Call(skein_llm::Phase),
    Keep,
}

/// One call's absolute deadlines and arming state.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Table {
    config: Deadlines,
    sent_at: Option<Time>,
    event_at: Option<Time>,
    measures: Measures,
    connect: Option<Time>,
    handshake: Option<Time>,
    head: Option<Time>,
    idle: Option<Time>,
    whole: Option<Time>,
    keep: Option<Time>,
}

impl Table {
    pub(crate) fn new(config: Deadlines, now: Time) -> Table {
        Table {
            config,
            sent_at: None,
            event_at: None,
            measures: Measures::none(),
            connect: at(now, config.connect),
            handshake: None,
            head: None,
            idle: None,
            whole: at(now, config.whole),
            keep: None,
        }
    }

    /// Reconcile the complete deadline set after every phase transition.
    pub(crate) fn arm(&mut self, phase: Phase, now: Time, idle_keep: skein_lib::Duration) {
        let (connect, handshake, head, idle, whole, keep) = match phase {
            Phase::Waiting => (false, false, false, false, true, false),
            Phase::Connecting => (true, false, false, false, true, false),
            Phase::Handshaking => (false, true, false, false, true, false),
            Phase::Head => (false, false, true, false, true, false),
            Phase::Streaming => (false, false, false, true, true, false),
            Phase::Draining => (false, false, false, true, false, false),
            Phase::Idle => (false, false, false, false, false, true),
            Phase::Closing | Phase::Closed => (false, false, false, false, false, false),
        };
        self.connect = selected(self.connect, connect, now, self.config.connect);
        self.handshake = selected(self.handshake, handshake, now, self.config.handshake);
        if !head {
            self.head = None;
        }
        self.idle = selected(self.idle, idle, now, self.config.idle);
        if !whole {
            self.whole = None;
        }
        self.keep = selected(self.keep, keep, now, Some(idle_keep));
    }

    pub(crate) fn sent(&mut self, now: Time, request_bytes: u64) {
        if self.sent_at.is_none() {
            self.sent_at = Some(now);
            self.measures.request_bytes = Some(request_bytes);
        }
        self.head = at(now, self.config.head);
    }

    pub(crate) fn response_bytes(&mut self, now: Time) {
        if self.measures.first_byte.is_none()
            && let Some(sent) = self.sent_at
        {
            self.measures.first_byte = Some(now.saturating_since(sent));
        }
    }

    pub(crate) const fn measures(&self) -> Measures {
        self.measures
    }

    pub(crate) fn activity(&mut self, now: Time, events: u64) {
        let gap = match self.event_at {
            Some(previous) => Some(now.saturating_since(previous)),
            None if events > 1 => Some(skein_lib::Duration::ZERO),
            None => None,
        };
        if let Some(gap) = gap {
            self.measures.largest_gap = Some(match self.measures.largest_gap {
                Some(previous) => previous.max(gap),
                None => gap,
            });
        }
        self.event_at = Some(now);
        self.idle = at(now, self.config.idle);
    }

    pub(crate) fn next(&self) -> Option<Time> {
        let mut earliest = self.connect;
        earliest = earlier(earliest, self.handshake);
        earliest = earlier(earliest, self.head);
        earliest = earlier(earliest, self.idle);
        earliest = earlier(earliest, self.whole);
        earlier(earliest, self.keep)
    }

    pub(crate) fn due(&self, now: Time) -> Option<Due> {
        if expired(self.connect, now) {
            Some(Due::Call(skein_llm::Phase::Connect))
        } else if expired(self.handshake, now) {
            Some(Due::Call(skein_llm::Phase::Handshake))
        } else if expired(self.head, now) {
            Some(Due::Call(skein_llm::Phase::Head))
        } else if expired(self.idle, now) {
            Some(Due::Call(skein_llm::Phase::Idle))
        } else if expired(self.whole, now) {
            Some(Due::Call(skein_llm::Phase::Whole))
        } else if expired(self.keep, now) {
            Some(Due::Keep)
        } else {
            None
        }
    }
}

#[expect(clippy::manual_map, reason = "the step subset excludes closures")]
fn at(now: Time, duration: Option<skein_lib::Duration>) -> Option<Time> {
    match duration {
        Some(duration) => Some(now.saturating_add(duration)),
        None => None,
    }
}

fn earlier(left: Option<Time>, right: Option<Time>) -> Option<Time> {
    match left {
        Some(left) => match right {
            Some(right) => Some(left.min(right)),
            None => Some(left),
        },
        None => right,
    }
}

fn expired(deadline: Option<Time>, now: Time) -> bool {
    match deadline {
        Some(deadline) => now >= deadline,
        None => false,
    }
}

fn selected(previous: Option<Time>, enabled: bool, now: Time, duration: Option<skein_lib::Duration>) -> Option<Time> {
    if enabled {
        match previous {
            Some(previous) => Some(previous),
            None => at(now, duration),
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skein_lib::Duration;

    #[test]
    fn every_phase_selects_its_complete_deadline_set() {
        let duration = Some(Duration::from_secs(1));
        let config =
            Deadlines { connect: duration, handshake: duration, head: duration, idle: duration, whole: duration };
        for (phase, expected) in [
            (Phase::Waiting, [false, false, false, false, true, false]),
            (Phase::Connecting, [true, false, false, false, true, false]),
            (Phase::Handshaking, [false, true, false, false, true, false]),
            (Phase::Head, [false, false, true, false, true, false]),
            (Phase::Streaming, [false, false, false, true, true, false]),
            (Phase::Draining, [false, false, false, true, false, false]),
            (Phase::Idle, [false, false, false, false, false, true]),
            (Phase::Closing, [false; 6]),
            (Phase::Closed, [false; 6]),
        ] {
            let mut table = Table::new(config, Time::ZERO);
            table.sent(Time::ZERO, 1);
            table.arm(phase, Time::ZERO, Duration::from_secs(10));
            assert_eq!(
                [
                    table.connect.is_some(),
                    table.handshake.is_some(),
                    table.head.is_some(),
                    table.idle.is_some(),
                    table.whole.is_some(),
                    table.keep.is_some()
                ],
                expected,
                "{phase:?}"
            );
        }
    }

    #[test]
    fn a_terminal_keeps_drain_idleness_and_closing_disarms_even_past_due_times() {
        let mut table = Table::new(
            Deadlines { idle: Some(Duration::from_secs(1)), whole: Some(Duration::from_secs(2)), ..Deadlines::none() },
            Time::ZERO,
        );
        table.arm(Phase::Streaming, Time::ZERO, Duration::from_secs(10));
        table.arm(Phase::Draining, Time::ZERO, Duration::from_secs(10));
        let later = Time::from_nanos(3_000_000_000);
        assert_eq!(table.due(later), Some(Due::Call(skein_llm::Phase::Idle)));
        table.arm(Phase::Closing, later, Duration::from_secs(10));
        assert_eq!(table.next(), None);
        assert_eq!(table.due(later), None);
    }
    #[test]
    fn connecting_after_waiting_arms_connect_now_and_retains_the_original_whole() {
        let mut table = Table::new(
            Deadlines {
                connect: Some(Duration::from_secs(1)),
                whole: Some(Duration::from_secs(5)),
                ..Deadlines::none()
            },
            Time::ZERO,
        );
        table.arm(Phase::Waiting, Time::ZERO, Duration::from_secs(10));
        assert_eq!(table.next(), Some(Time::from_nanos(5_000_000_000)));
        table.arm(Phase::Connecting, Time::from_nanos(3_000_000_000), Duration::from_secs(10));
        assert_eq!(table.connect, Some(Time::from_nanos(4_000_000_000)));
        assert_eq!(table.whole, Some(Time::from_nanos(5_000_000_000)));
    }
}

#[cfg(test)]
mod measure_tests {
    use super::*;
    use skein_lib::Duration;

    #[test]
    fn measured_request_above_u32_max_is_exact_and_later_upload_does_not_move_send_time() {
        let mut table = Table::new(Deadlines::none(), Time::ZERO);
        let bytes = u64::from(u32::MAX).checked_add(u64::from(u32::MAX)).unwrap();
        table.sent(Time::from_nanos(100), bytes);
        table.sent(Time::from_nanos(200), bytes);
        table.response_bytes(Time::from_nanos(300));
        assert_eq!(
            table.measures(),
            Measures { request_bytes: Some(bytes), first_byte: Some(Duration::from_nanos(200)), largest_gap: None }
        );
    }

    #[test]
    fn one_response_event_is_unobserved_as_a_gap_and_two_simultaneous_events_have_a_real_zero_gap() {
        let mut table = Table::new(Deadlines::none(), Time::ZERO);
        table.activity(Time::from_nanos(100), 1);
        assert_eq!(table.measures().largest_gap, None);
        table.activity(Time::from_nanos(100), 1);
        assert_eq!(table.measures().largest_gap, Some(Duration::ZERO));
    }
}
