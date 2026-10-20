use crate::fixtures::{self, LIMITS, Shape};
use skein_fake_forge_domain::{
    Observation,
    api::{self as raw, Kind},
};
use skein_forge::api::{Answer, Error, Op, Read, Repository, Write};
use skein_forge_client::{
    self as client, Attempt, Entry, Event, Key, Outcome, RecoveryClock, Request, Resource, Stored,
};
use skein_forge_connection::Component;
use skein_forge_machine_world::{
    fixtures as machine,
    joint::{self, Joint, REPO},
};
use skein_lib::{Duration, Env, Map, Queue, Time, Token};
use std::collections::VecDeque;

const TURN: Duration = Duration::from_millis(10);
const MIN_TURN: Duration = Duration::from_nanos(100_000);
const TURNS: u32 = 100_000;

#[derive(Debug)]
struct Pending {
    client: Token,
    generation: u64,
    write: bool,
}

struct Delivery {
    generation: Option<u64>,
    event: Event,
    write: bool,
}

struct Release {
    decision: usize,
    write: bool,
    covered: bool,
}

struct Delivered {
    token_generation: u64,
    client_generation: u64,
}

enum Decision {
    Make(Box<Entry>),
    Output(Vec<Request>),
}

/// Scripted durable owner; finish settles its accepted calls.
pub struct World {
    client: client::Domain,
    env: Env<client::Limits>,
    out: Queue<Request>,
    joint: Joint,
    durable: Map<Key, Stored>,
    entries: Map<u64, Entry>,
    held: Map<u64, Entry>,
    writers: Vec<Resource>,
    pending: Map<u64, Pending>,
    deliveries: VecDeque<Delivery>,
    decisions: Vec<Decision>,
    releases: Vec<Release>,
    delivered: Vec<Delivered>,
    outcomes: Vec<(u64, Outcome)>,
    read_results: Map<Token, Result<Answer, Error>>,
    changes: Map<Resource, Answer>,
    owner_sequence: u64,
    hints: u64,
    shape: Shape,
    sequence: u64,
    generation: u64,
    discarded: u64,
    lose_creation: bool,
    slow_creation: bool,
    seed: u64,
}

impl World {
    #[must_use]
    pub fn new(seed: u64, cut_chance: u32) -> Self {
        Self::with(seed, cut_chance, Shape::Plain)
    }

    pub(crate) fn with(seed: u64, cut_chance: u32, shape: Shape) -> Self {
        let config = match shape {
            Shape::Plain => machine::protocol_config(),
            Shape::Hooked => machine::hooked_protocol_config(),
        };
        let joint = Joint::with(
            seed,
            cut_chance,
            config,
            shape.limits(),
            shape.settings(),
            fixtures::shared(),
            joint::checks(1000),
        );
        let mut world = Self {
            client: configured(seed),
            env: Env { now: joint.now(), wall: joint.env.wall, limits: LIMITS },
            out: Queue::with_capacity(client::max_out(&LIMITS)),
            joint,
            durable: Map::with_capacity(LIMITS.resources + LIMITS.repositories),
            entries: Map::with_capacity(LIMITS.entries),
            held: Map::with_capacity(LIMITS.entries),
            writers: Vec::new(),
            pending: Map::with_capacity(LIMITS.calls),
            deliveries: VecDeque::new(),
            decisions: Vec::new(),
            releases: Vec::new(),
            delivered: Vec::new(),
            outcomes: Vec::new(),
            read_results: Map::with_capacity(LIMITS.pending),
            changes: Map::with_capacity(LIMITS.resources),
            owner_sequence: 0,
            hints: 0,
            shape,
            sequence: 0,
            generation: 0,
            discarded: 0,
            lose_creation: false,
            slow_creation: false,
            seed,
        };
        world.event(Event::Restored { clock: RecoveryClock::Monotonic });
        world
    }

    pub fn event(&mut self, event: Event) {
        match &event {
            Event::Make { entry } | Event::Remake { entry } => {
                assert_eq!(entry.repository, REPO);
                self.held.remove(&entry.number);
                self.entries.insert(entry.number, entry.clone()).expect("owner outbox capacity");
                self.decisions.push(Decision::Make(Box::new(entry.clone())));
            }
            Event::Read { repository, .. } => assert_eq!(*repository, REPO),
            Event::Writer { resource, taken } => {
                if *taken {
                    if !self.writers.contains(resource) {
                        self.writers.push(resource.clone());
                    }
                } else {
                    self.writers.retain(|claimed| claimed != resource);
                }
            }
            Event::Hint { .. } => self.hints += 1,
            Event::Keep { .. }
            | Event::Pushed { .. }
            | Event::Withdraw { .. }
            | Event::Restore { .. }
            | Event::Restored { .. }
            | Event::PauseOutbox
            | Event::ReadAfresh
            | Event::SettleOutbox
            | Event::Answered { .. } => {}
        }
        self.sync_env();
        client::step(&mut self.client, &self.env, event, &mut self.out);
        self.commit();
        self.client.reclaim();
    }

    pub fn run_for(&mut self, span: Duration) {
        let end = self.joint.now().saturating_add(span);
        for _ in 0..TURNS {
            if !self.iterate(end) {
                return;
            }
        }
        panic!("protocol world did not reach its horizon, seed {}", self.seed);
    }

    /// One read through the client, as an owner's decision read.
    /// # Errors
    /// Returns the client's terminal refusal or failed read.
    pub fn read(&mut self, mut read: Read) -> Result<Answer, Error> {
        let end = self.joint.now().saturating_add(Duration::from_secs(5));
        for _ in 0..100 {
            self.owner_sequence = self.owner_sequence.checked_add(1).expect("owner read sequence bound");
            let owner = Token::new(self.owner_sequence);
            let retry = read.clone();
            self.event(Event::Read { owner, repository: REPO, read });
            let mut result = None;
            for _ in 0..TURNS {
                if let Some(answer) = self.read_results.remove(&owner) {
                    result = Some(answer);
                    break;
                }
                assert!(self.iterate(end), "owner read exceeded 5 s, seed {}", self.seed);
            }
            let result = result.unwrap_or_else(|| panic!("owner read did not settle, seed {}", self.seed));
            if result != Err(Error::Busy) {
                return result;
            }
            assert!(self.iterate(end), "owner read exceeded 5 s, seed {}", self.seed);
            read = retry;
        }
        panic!("owner read busy after 100 tries, seed {}", self.seed);
    }

    /// The fake's items, read past the client: (number, title, open), by number.
    #[must_use]
    pub fn snapshot(&self) -> Vec<(u64, Box<[u8]>, bool)> {
        let mut rows = Vec::new();
        let mut page = 1;
        loop {
            let read = raw::Read::Items {
                order: raw::Order::LeastUpdated,
                state: None,
                kind: None,
                labels: Box::new([]),
                author: None,
                since: Time::ZERO,
                page,
                limit: self.joint.fake_env.limits.limits.page_size,
            };
            let raw::Answer::Items { items, more, .. } =
                self.joint.domain.inspect(&self.joint.fake_env.limits, b"org/repo", &read).expect("fake snapshot")
            else {
                panic!("fake snapshot must return items, seed {}", self.seed)
            };
            rows.extend(
                items.into_vec().into_iter().map(|item| (item.number, item.title, item.state == raw::State::Open)),
            );
            if !more {
                rows.sort_by_key(|row| row.0);
                return rows;
            }
            page += 1;
        }
    }

    pub fn person_issue(&mut self, title: &[u8]) -> u64 {
        self.joint.issue(title, b"person")
    }

    pub fn person_edit(&mut self, number: u64, title: &[u8]) {
        self.joint
            .direct(raw::Op::Write(raw::Write::EditItem { number, title: Some(Box::from(title)), body: None }))
            .expect("person edits item");
    }

    pub fn person_close(&mut self, number: u64) {
        self.joint.close(number);
    }

    /// Hints routed to a client.
    #[must_use]
    pub fn hints(&self) -> u64 {
        self.hints
    }

    /// The latest answer the client reported for a kept resource.
    #[must_use]
    pub fn changed(&self, resource: &Resource) -> Option<&Answer> {
        self.changes.get(resource)
    }

    pub fn restart(&mut self) {
        self.route();
        self.generation = self.generation.checked_add(1).expect("owner generation bound");
        let component = Component::new(self.shape.settings(), &self.shape.limits(), &fixtures::shared())
            .expect("component admits client limits");
        self.joint.replace(component);
        // The replaced component owns no calls from the former process.
        self.pending = Map::with_capacity(LIMITS.calls);
        self.client = configured(self.seed);
        let records: Vec<_> = self.durable.iter().map(|(_, record)| record.clone()).collect();
        for record in records {
            self.event(Event::Restore { record });
        }
        self.event(Event::Restored { clock: RecoveryClock::Monotonic });
        for resource in self.writers.clone() {
            self.event(Event::Writer { resource, taken: true });
        }
        self.event(Event::PauseOutbox);
        let entries: Vec<_> = self.entries.iter().map(|(_, entry)| entry.clone()).collect();
        for entry in entries {
            assert!(!self.held.contains_key(&entry.number));
            self.event(Event::Make { entry });
        }
        self.event(Event::ReadAfresh);
    }

    pub fn finish(&mut self) {
        let end = self.joint.now().saturating_add(Duration::from_secs(60));
        for _ in 0..TURNS {
            self.route();
            if self.pending.is_empty()
                && self.deliveries.is_empty()
                && !self.client.is_ready()
                && !self.client.is_due(self.joint.now())
            {
                assert!(self.entries.is_empty() && self.durable.is_empty(), "owner outbox and store settled");
                assert_eq!(self.joint.domain.observations_lost(), 0);
                return;
            }
            assert!(self.iterate(end), "protocol world did not finish within 60 s, seed {}", self.seed);
        }
        panic!("protocol world did not settle, seed {}", self.seed);
    }

    #[must_use]
    pub fn creations(&self) -> u32 {
        u32::try_from(
            self.joint
                .observations
                .iter()
                .filter(|observation| matches!(observation, Observation::Opened { kind: Kind::Issue, by: 1, .. }))
                .count(),
        )
        .expect("world creation count")
    }

    #[must_use]
    pub fn made(&self, entry: u64) -> bool {
        self.outcomes.iter().any(|(number, outcome)| *number == entry && matches!(outcome, Outcome::Made { .. }))
    }

    /// Writes without committed Progress; old-generation answers delivered
    /// to the new client.
    #[must_use]
    pub fn owner_counts(&self) -> (u64, u64) {
        let uncovered = self
            .releases
            .iter()
            .filter(|release| release.write && (!release.covered || release.decision >= self.decisions.len()))
            .count();
        let stale =
            self.delivered.iter().filter(|delivery| delivery.token_generation != delivery.client_generation).count();
        (u64::try_from(uncovered).expect("release count"), u64::try_from(stale).expect("delivery count"))
    }

    /// The latest attempt the decision log committed for the entry, kept
    /// after the entry settles.
    #[must_use]
    pub fn attempt(&self, entry: u64) -> Option<Attempt> {
        for decision in self.decisions.iter().rev() {
            match decision {
                Decision::Make(saved) => {
                    if saved.number == entry {
                        return saved.attempt;
                    }
                }
                Decision::Output(output) => {
                    for request in output.iter().rev() {
                        if let Request::Progress { entry: saved } = request
                            && saved.number == entry
                        {
                            return saved.attempt;
                        }
                    }
                }
            }
        }
        None
    }

    #[must_use]
    pub fn released_writes(&self) -> u64 {
        u64::try_from(self.releases.iter().filter(|release| release.write).count()).expect("release count")
    }

    #[must_use]
    pub fn discarded(&self) -> u64 {
        self.discarded
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn lose_creation_answer(&mut self, entry: Entry) {
        self.lose_creation = true;
        self.event(Event::Make { entry });
        let end = self.joint.now().saturating_add(Duration::from_secs(20));
        for _ in 0..TURNS {
            self.route();
            if self
                .deliveries
                .iter()
                .any(|delivery| delivery.write && matches!(delivery.event, Event::Answered { result: Err(_), .. }))
            {
                return;
            }
            assert!(self.iterate(end), "lost creation terminal not queued within 20 s, seed {}", self.seed);
        }
        panic!("lost creation did not settle, seed {}", self.seed);
    }

    fn sync_env(&mut self) {
        self.env.now = self.joint.now();
        self.env.wall = self.joint.env.wall;
    }

    fn iterate(&mut self, end: Time) -> bool {
        self.route();
        if let Some(delivery) = self.deliveries.pop_front() {
            if let Some(generation) = delivery.generation {
                if generation != self.generation {
                    self.discarded += 1;
                    return true;
                }
                self.delivered.push(Delivered { token_generation: generation, client_generation: self.generation });
            }
            self.event(delivery.event);
        } else if self.client.is_ready() {
            self.sync_env();
            client::resume(&mut self.client, &self.env, &mut self.out);
            self.commit();
            self.client.reclaim();
        } else if self.client.is_due(self.joint.now()) {
            self.sync_env();
            client::fire(&mut self.client, &self.env, &mut self.out);
            self.commit();
            self.client.reclaim();
        } else {
            let now = self.joint.now();
            if now >= end {
                return false;
            }
            let mut next = self.client.next_deadline().unwrap_or(end).min(end);
            if !self.pending.is_empty() {
                next = next.min(now.saturating_add(TURN));
            }
            next = next.max(now.saturating_add(MIN_TURN)).min(end);
            self.joint.run_for(next.saturating_since(now));
        }
        true
    }

    fn route(&mut self) {
        for (token, answer) in std::mem::take(&mut self.joint.answers) {
            let pending = self.pending.remove(&token).expect("terminal names an owner call");
            self.deliveries.push_back(Delivery {
                generation: Some(pending.generation),
                event: Event::Answered { call: pending.client, cost: answer.cost, result: answer.result },
                write: pending.write,
            });
        }
        for hinted in self.joint.hints.drain(..) {
            self.deliveries.push_back(Delivery {
                generation: None,
                event: Event::Hint { hint: hinted.hint },
                write: false,
            });
        }
        if self.slow_creation && self.creations() > 0 {
            let config = machine::fake_config();
            self.joint.fake_env.limits.latency_min = config.latency_min;
            self.joint.fake_env.limits.latency_max = config.latency_max;
            self.slow_creation = false;
        }
    }

    fn commit(&mut self) {
        let mut decision = Vec::new();
        let mut calls = Vec::new();
        let mut settle = false;
        while let Some(request) = self.out.pop() {
            match &request {
                Request::Save { record } => {
                    let key = match record {
                        Stored::Live(record) => Key::Live(record.watch.resource.clone()),
                        Stored::Repository(record) => Key::Repository(record.repository),
                        Stored::Writer(writer) => Key::Writer(writer.forge),
                    };
                    self.durable.insert(key, record.clone()).expect("owner store capacity");
                }
                Request::Erase { key } => {
                    self.durable.remove(key).expect("stored row erased");
                }
                Request::Progress { entry } => {
                    self.entries.insert(entry.number, entry.clone()).expect("owner outbox capacity");
                }
                Request::Outcome { entry, outcome, .. } => {
                    match outcome {
                        Outcome::Uncertain | Outcome::Queued => {}
                        Outcome::Made { .. } | Outcome::Failed(_) | Outcome::Raced { .. } | Outcome::Withdrawn => {
                            self.entries.remove(entry).expect("stored entry settled");
                        }
                        Outcome::Held => {
                            let saved = self.entries.remove(entry).expect("stored entry held");
                            self.held.insert(*entry, saved).expect("held entry capacity");
                        }
                    }
                    self.outcomes.push((*entry, *outcome));
                }
                Request::Call { call, repository, op } => calls.push((*call, *repository, op.clone())),
                Request::ReadAfreshDone => settle = true,
                Request::Kept { result, .. } => assert_eq!(*result, Ok(())),
                Request::Changed { resource, result } => {
                    if let Ok(answer) = result {
                        self.changes.insert(resource.clone(), answer.clone()).expect("owner changed capacity");
                    }
                }
                Request::Read { owner, result } => {
                    assert!(self.read_results.insert(*owner, result.clone()).expect("owner read capacity").is_none());
                }
                Request::Drift { .. } | Request::OutboxDone => {}
            }
            decision.push(request);
        }
        self.decisions.push(Decision::Output(decision));
        for (call, repository, op) in calls {
            self.submit(call, repository, op);
        }
        if settle {
            self.event(Event::SettleOutbox);
        }
    }

    fn submit(&mut self, call: Token, repository: Repository, op: Op) {
        assert_eq!(repository, REPO);
        let (write, covered) = match &op {
            Op::Write(write) => {
                (true, self.entries.iter().any(|(_, entry)| entry.attempt.is_some() && entry.effect.write == *write))
            }
            Op::Read(_) => (false, true),
        };
        self.sequence = self.sequence.checked_add(1).expect("owner call sequence bound");
        self.pending
            .insert(self.sequence, Pending { client: call, generation: self.generation, write })
            .expect("owner call capacity");
        self.releases.push(Release { decision: self.decisions.len() - 1, write, covered });
        if self.lose_creation && matches!(op, Op::Write(Write::CreateIssue { .. })) {
            let latency = LIMITS.lifetime.saturating_mul(2);
            self.joint.fake_env.limits.latency_min = latency;
            self.joint.fake_env.limits.latency_max = latency;
            self.lose_creation = false;
            self.slow_creation = true;
        }
        self.joint.call(self.sequence, repository, op);
    }
}

fn configured(seed: u64) -> client::Domain {
    client::Domain::configured(
        &LIMITS,
        seed,
        client::Config {
            namespace: Box::from(b"kit".as_slice()),
            writers: Box::new([client::Writer { forge: 1, author: 1 }]),
        },
    )
    .expect("owner authenticated writer")
}
