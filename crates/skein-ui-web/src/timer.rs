//! One deadline for the next report (ui-web.md, 5.1).
#[cfg(target_arch = "wasm32")]
pub mod web;
use skein_lib::Time;
#[derive(Default)]
pub struct Timer {
    deadline: Option<Time>,
}
impl Timer {
    pub fn set(&mut self, deadline: Option<Time>) {
        self.deadline = deadline;
    }
    #[must_use]
    pub fn due(&mut self, now: Time) -> bool {
        if self.deadline.is_some_and(|at| at <= now) {
            self.deadline = None;
            true
        } else {
            false
        }
    }
}
