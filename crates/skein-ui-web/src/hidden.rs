//! A hidden tab releases its stream after its delay (ui-web.md, 10).
use crate::transport::StreamDuty;
use skein_lib::{Duration, Time};
#[cfg(target_arch = "wasm32")]
pub mod web;

pub struct Hidden {
    delay: Duration,
    background: bool,
    deadline: Option<Time>,
    opening: Option<StreamDuty>,
}
impl Hidden {
    #[must_use]
    pub fn new(delay: Duration) -> Self {
        Self { delay, background: false, deadline: None, opening: None }
    }
    pub fn visibility(&mut self, hidden: bool, now: Time) -> Option<StreamDuty> {
        if hidden == self.background {
            return None;
        }
        self.background = hidden;
        self.deadline = hidden.then(|| now.saturating_add(self.delay));
        if hidden { None } else { self.opening.take() }
    }
    pub fn carry(&mut self, duty: StreamDuty) -> Option<StreamDuty> {
        match duty {
            open @ StreamDuty::Open { .. } if self.background => {
                self.opening = Some(open);
                None
            }
            open @ StreamDuty::Open { .. } => Some(open),
            StreamDuty::Close => {
                self.opening = None;
                Some(StreamDuty::Close)
            }
        }
    }
    pub fn fire(&mut self, now: Time) -> Option<StreamDuty> {
        if self.deadline.is_some_and(|at| now >= at) {
            self.deadline = None;
            Some(StreamDuty::Close)
        } else {
            None
        }
    }
    #[must_use]
    pub fn deadline(&self) -> Option<Time> {
        self.deadline
    }
}
