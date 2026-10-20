use super::{Entry, Event, List, Map, RecoveryClock, Resource, Time, World, configured};

impl World {
    pub fn restart(&mut self) {
        // Volatile budget knowledge belongs to the old live generation.
        // The independent forge keeps its window and can answer 429 again.
        self.blocked = Time::ZERO;
        self.generation = self.generation.checked_add(1).expect("world generation bound");
        self.stats.restarts = self.stats.restarts.saturating_add(1);
        self.client = configured(&self.env.limits, self.seed);
        // Decision reads belong to the former process, and are reissued by
        // the parent after cold recovery, rather than restored as effects.
        self.readers = Map::with_capacity(16);
        self.read_results = Map::with_capacity(16);
        let mut records = List::with_capacity(self.durable.len());
        for (_, record) in &self.durable {
            records.push(record.clone()).expect("world store limit");
        }
        for record in records.into_boxed() {
            self.event(Event::Restore { record });
        }
        self.event(Event::Restored { clock: RecoveryClock::Monotonic });
        let mut writers = List::with_capacity(self.writers.len());
        for resource in &self.writers {
            writers.push(resource.clone()).expect("writer claim limit");
        }
        for resource in writers.into_boxed() {
            self.event(Event::Writer { resource, taken: true });
        }
        self.outbox_paused = true;
        self.event(Event::PauseOutbox);
        let mut entries = List::with_capacity(self.entries.len());
        for (_, entry) in &self.entries {
            entries.push(entry.clone()).expect("top outbox limit");
        }
        for entry in entries.into_boxed() {
            assert!(!self.held.contains_key(&entry.number), "held entries stay with the parent");
            self.event(Event::Make { entry });
        }
        self.event(Event::ReadAfresh);
    }

    /// Record a durable branch claim and tell the client; ends on taken=false.
    pub fn writer(&mut self, resource: Resource, taken: bool) {
        if taken {
            if !self.writers.as_slice().contains(&resource) {
                self.writers.push(resource.clone()).expect("writer claim limit");
            }
        } else {
            let mut remaining = List::with_capacity(self.env.limits.resources);
            for claimed in &self.writers {
                if *claimed != resource {
                    remaining.push(claimed.clone()).expect("writer claim limit");
                }
            }
            self.writers = remaining;
        }
        self.event(Event::Writer { resource, taken });
    }

    /// Hand a held entry back on the person's choice, preserving its identity.
    pub fn remake(&mut self, entry: Entry) {
        let saved = self.held.remove(&entry.number).expect("a person remakes a held entry");
        assert_eq!(saved.task, entry.task);
        assert_eq!(saved.repository, entry.repository);
        assert_eq!(saved.effect, entry.effect);
        let entry = Entry { start: saved.start, attempt: saved.attempt, failures: saved.failures, ..entry };
        assert!(self.entries.insert(entry.number, entry.clone()).expect("top outbox limit").is_none());
        self.event(Event::Remake { entry });
    }

    /// The parent's observed `ReadAfreshDone` and `OutboxDone` totals.
    #[must_use]
    pub const fn recovery_completions(&self) -> (u32, u32) {
        (self.read_afresh_done, self.outbox_done)
    }
}
