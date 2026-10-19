use super::super::{LIMITS, REPO, Sent, at};
use crate::{
    Domain, Entry, Event, Key, Limits, LiveRecord, Outcome, RecoveryClock, Request, Resource, Stored, Watch, What,
    fire, max_out, resume, step,
};
use alloc::boxed::Box;
use skein_forge::api::{self, Answer, Error, Op, Read};
use skein_lib::{Env, List, Map, Queue, Token, Wall};
pub(in crate::tests) struct Harness {
    pub(in crate::tests) domain: Domain,
    pub(in crate::tests) env: Env<Limits>,
    pub(in crate::tests) records: Map<Key, Stored>,
    pub(in crate::tests) entries: Map<u64, Entry>,
    pub(in crate::tests) calls: Queue<Sent>,
    pub(in crate::tests) news: List<Answer>,
    pub(in crate::tests) kept: List<Result<(), Error>>,
    pub(in crate::tests) outcomes: List<Outcome>,
    pub(in crate::tests) drift: u32,
    pub(in crate::tests) failures: List<Error>,
    pub(in crate::tests) read_refusals: List<Error>,
}
impl Harness {
    pub(in crate::tests) fn new() -> Harness {
        let limits = Limits { rate: 100, reserve: 0, ..LIMITS };
        Self::with_limits(limits)
    }
    pub(in crate::tests) fn with_limits(limits: Limits) -> Harness {
        let mut h = Harness {
            domain: super::super::configured(&limits),
            env: Env { now: at(100), wall: Wall::EPOCH, limits },
            records: Map::with_capacity(20),
            entries: Map::with_capacity(20),
            calls: Queue::with_capacity(20),
            news: List::with_capacity(30),
            kept: List::with_capacity(10),
            outcomes: List::with_capacity(10),
            drift: 0,
            failures: List::with_capacity(10),
            read_refusals: List::with_capacity(10),
        };
        h.event(Event::Restored { clock: RecoveryClock::Monotonic });
        h
    }
    pub(in crate::tests) fn event(&mut self, event: Event) {
        let mut out = Queue::with_capacity(max_out(&self.env.limits));
        step(&mut self.domain, &self.env, event, &mut out);
        self.take(&mut out);
    }
    pub(in crate::tests) fn take(&mut self, out: &mut Queue<Request>) {
        for _ in 0..out.len() {
            match out.pop().expect("output exists") {
                Request::Save { record } => {
                    let key = match &record {
                        Stored::Live(record) => Key::Live(record.watch.resource.clone()),
                        Stored::Repository(record) => Key::Repository(record.repository),
                    };
                    self.records.insert(key, record).expect("store fits");
                }
                Request::Erase { key } => {
                    self.records.remove(&key).expect("saved row erased");
                }
                Request::Progress { entry } => {
                    self.entries.insert(entry.number, entry).expect("top entry capacity");
                }
                Request::Kept { result, .. } => {
                    self.kept.push(result).expect("kept fits");
                }
                Request::Changed { result: Ok(answer), .. } => {
                    self.news.push(answer).expect("news fits");
                }
                Request::Changed { result: Err(error), .. } => {
                    self.failures.push(error).expect("failure observations fit");
                }
                Request::Drift { .. } => self.drift = self.drift.saturating_add(1),
                Request::Call { call, op, .. } => self.calls.push(Sent { call, op }),
                Request::Outcome { outcome, .. } => {
                    self.outcomes.push(outcome).expect("outcome fits");
                }
                Request::Read { result: Err(error), .. } => {
                    self.read_refusals.push(error).expect("read refusals fit");
                }
                Request::Read { result: Ok(_), .. } => panic!("no successful fresh parent read"),
                Request::ReadAfreshDone | Request::OutboxDone => {
                    panic!("ordinary keep test did not request restart phases")
                }
            }
        }
        self.domain.reclaim();
    }
    pub(in crate::tests) fn keep(&mut self, watches: Box<[Watch]>) {
        self.event(Event::Keep { owner: Token::new(1), watches });
    }
    pub(in crate::tests) fn one(&mut self) -> Sent {
        let mut out = Queue::with_capacity(max_out(&self.env.limits));
        resume(&mut self.domain, &self.env, &mut out);
        self.take(&mut out);
        self.calls.pop().expect("call ready")
    }
    pub(in crate::tests) fn answer(&mut self, sent: Sent, answer: Answer) {
        self.event(Event::Answered { call: sent.call, cost: 1, result: Ok(answer) });
    }
    pub(in crate::tests) fn clock(&mut self) {
        let sent = self.one();
        assert_eq!(sent.op, Op::Read(Read::Items { since: at(u64::MAX), page: 1, kind: None }));
        self.answer(sent, Answer::Items { items: Box::new([]), more: false, now: at(100) });
    }
    pub(in crate::tests) fn fire(&mut self, now: u64) {
        self.env.now = at(now);
        for _ in 0_u32..20 {
            if !self.domain.is_due(self.env.now) {
                break;
            }
            let mut out = Queue::with_capacity(max_out(&self.env.limits));
            fire(&mut self.domain, &self.env, &mut out);
            self.take(&mut out);
        }
    }
    pub(in crate::tests) fn restart(&mut self) {
        let mut saved = List::with_capacity(self.records.len());
        for (_, record) in &self.records {
            saved.push(record.clone()).expect("store bounded");
        }
        self.domain = super::super::configured(&self.env.limits);
        self.calls = Queue::with_capacity(20);
        for record in saved.into_boxed() {
            self.event(Event::Restore { record });
        }
        self.event(Event::Restored { clock: RecoveryClock::Monotonic });
        let listing = self.one();
        self.answer(listing, Answer::Items { items: Box::new([]), more: false, now: self.env.now });
    }
    pub(in crate::tests) fn record(&self, resource: &Resource) -> &LiveRecord {
        match self.records.get(&Key::Live(resource.clone())).expect("saved live row") {
            Stored::Live(row) => row,
            Stored::Repository(_) => panic!("live row shape"),
        }
    }
}
pub(in crate::tests) fn issue() -> Resource {
    Resource { repository: REPO, what: What::Issue(9) }
}
pub(in crate::tests) fn pull() -> Resource {
    Resource { repository: REPO, what: What::Pull(9) }
}
pub(in crate::tests) fn branch() -> Resource {
    Resource { repository: REPO, what: What::Branch(Box::from(&b"topic"[..])) }
}
pub(in crate::tests) fn watch(resource: Resource, participating: bool) -> Watch {
    Watch { resource, participating }
}
pub(in crate::tests) fn summary() -> api::Summary {
    api::Summary {
        number: 9,
        kind: api::Kind::Issue,
        state: api::State::Open,
        title: Box::from(&b"goal"[..]),
        body: Box::new([]),
        author: 1,
        labels: Box::new([]),
        key: None,
        updated: at(100),
    }
}
pub(in crate::tests) fn current() -> api::Pull {
    api::Pull {
        number: 9,
        state: api::State::Open,
        head: Box::from(&b"topic"[..]),
        base: Box::from(&b"main"[..]),
        commit: [1; 32],
        base_commit: Some([2; 32]),
        merged: None,
        mergeable: true,
        ci: api::Ci::Passed,
        reviewers: Box::new([]),
    }
}
pub(in crate::tests) fn comment(id: u64) -> api::Comment {
    api::Comment {
        provenance: api::Provenance::Original,
        id,
        author: 2,
        body: Box::from(&b"hello"[..]),
        key: None,
        created: at(100),
        revision: 1,
    }
}
pub(in crate::tests) fn review(id: u64, time: u64) -> api::Review {
    api::Review {
        provenance: api::Provenance::Original,
        id,
        revision: time,
        author: 2,
        verdict: api::Verdict::Comment,
        commit: [1; 32],
        body: Box::from(&b"review"[..]),
        key: None,
        at: at(time),
        official: false,
    }
}
pub(in crate::tests) fn participation(h: &mut Harness, reviews: Box<[api::Review]>) {
    let item = h.one();
    h.answer(item, Answer::Item { item: summary(), comments: Box::new([]), more: false });
    let pull = h.one();
    h.answer(pull, Answer::Pull(current()));
    let read = h.one();
    assert_eq!(read.op, Op::Read(Read::Reviews { number: 9, page: 1 }));
    h.answer(read, Answer::Reviews { reviews, more: false });
}
pub(in crate::tests) fn matches_write(op: &Op) -> bool {
    match op {
        Op::Write(_) => true,
        Op::Read(_) => false,
    }
}
/// A row saved by normal Keep admission, shared by the measure contract.
pub(in crate::tests) fn measure_row() -> LiveRecord {
    let mut h = Harness::new();
    let resource = Resource { repository: REPO, what: What::Branch(Box::from(&b"kit"[..])) };
    h.keep(Box::new([watch(resource.clone(), false)]));
    h.record(&resource).clone()
}
