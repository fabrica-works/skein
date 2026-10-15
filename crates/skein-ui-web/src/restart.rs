//! Fail-stop restarts, outside the application's small store (ui-web.md, 5).
use skein_lib::{Duration, Wall};

#[cfg(target_arch = "wasm32")]
pub mod web;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Reload,
    Failed,
}

pub struct Restart {
    limit: usize,
    within: Duration,
    key: String,
    words: String,
}
impl Restart {
    #[must_use]
    pub fn new(limit: usize, within: Duration, key: &str, words: &str) -> Self {
        Self { limit, within, key: key.into(), words: words.into() }
    }
    fn recent(&self, now: Wall, date: Wall) -> bool {
        now.as_nanos().saturating_sub(date.as_nanos()) < self.within.as_nanos()
    }
    #[must_use]
    pub fn failed(&self, now: Wall, dates: &[Wall]) -> bool {
        dates.iter().filter(|date| self.recent(now, **date)).count() > self.limit
    }
    pub fn panicked(&self, now: Wall, dates: &mut Vec<Wall>) -> Action {
        dates.retain(|date| self.recent(now, *date));
        dates.push(now);
        if dates.len() > self.limit { Action::Failed } else { Action::Reload }
    }
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }
    #[must_use]
    pub fn words(&self) -> &str {
        &self.words
    }
}
