//! The durable owner uses the same boundaries with either forge's transport.
use crate::{github::Github, referee::Accounting};
use skein_fake_forge_domain::{self as fake, api as raw};
use skein_forge_connection::{Component, Event, Request};
use skein_forge_machine_world::{fixtures as machine, joint::Joint};
use skein_lib::{Duration, Env, Time, Wall};

pub(crate) enum Driver {
    Forgejo(Box<Joint>),
    Github(Box<Github>),
}
macro_rules! dispatch {
    ($self:expr, $world:ident, $body:expr) => {
        match $self {
            Driver::Forgejo($world) => $body,
            Driver::Github($world) => $body,
        }
    };
}
impl Driver {
    pub fn now(&self) -> Time {
        dispatch!(self, w, w.env.now)
    }
    pub fn wall(&self) -> Wall {
        dispatch!(self, w, w.env.wall)
    }
    pub fn component(&self) -> &Component {
        dispatch!(self, w, &w.component)
    }
    pub fn fake_env(&self) -> &Env<fake::Config> {
        dispatch!(self, w, &w.fake_env)
    }
    pub fn domain(&self) -> &fake::Domain {
        dispatch!(self, w, &w.domain)
    }
    pub fn observations(&self) -> &[fake::Observation] {
        dispatch!(self, w, w.observations.as_slice())
    }
    pub fn events(&self) -> &[String] {
        dispatch!(self, w, w.events.as_slice())
    }
    pub fn connected(&self) -> u64 {
        dispatch!(self, w, w.connected)
    }
    pub fn closed(&self) -> bool {
        dispatch!(self, w, w.closed)
    }
    pub fn busy(&self) -> bool {
        dispatch!(self, w, w.busy())
    }
    pub fn tick(&mut self) {
        dispatch!(self, w, w.tick());
    }
    pub fn run_for(&mut self, span: Duration) {
        dispatch!(self, w, w.run_for(span));
    }
    pub fn time(&mut self, now: Time) {
        dispatch!(self, w, w.time(now));
    }
    pub fn replace(&mut self, component: Component) {
        dispatch!(self, w, w.replace(component));
    }
    pub fn call(&mut self, call: u64, repository: skein_forge::api::Repository, op: skein_forge::api::Op) {
        dispatch!(self, w, w.call(call, repository, op));
    }
    pub fn action(&mut self, request: Request) {
        dispatch!(self, w, w.component.down(&w.env, request, &mut w.above, &mut w.below));
    }
    pub fn pop(&mut self) -> Option<Event> {
        dispatch!(self, w, w.above.pop())
    }
    pub fn answers(&mut self) -> std::collections::BTreeMap<u64, skein_forge_connection::Answered> {
        dispatch!(self, w, std::mem::take(&mut w.answers))
    }
    pub fn hints(&mut self) -> Vec<skein_forge_connection::hooks::Hinted> {
        match self {
            Self::Forgejo(w) => w.hints.drain(..).collect(),
            Self::Github(_) => Vec::new(),
        }
    }
    pub fn path(&self) -> &[u8] {
        match self {
            Self::Forgejo(_) => b"org/repo",
            Self::Github(_) => crate::github_fixture::PATH,
        }
    }
    pub fn direct(&mut self, op: raw::Op) -> Result<raw::Answer, raw::Error> {
        dispatch!(self, w, w.direct(op))
    }
    pub fn issue(&mut self, title: &[u8], body: &[u8]) -> u64 {
        match self {
            Self::Forgejo(w) => w.issue(title, body),
            Self::Github(w) => {
                let raw::Answer::Created(id) = w
                    .direct(raw::Op::Write(raw::Write::CreateIssue {
                        title: Box::from(title),
                        body: Box::from(body),
                        labels: Box::from([]),
                    }))
                    .expect("person issue")
                else {
                    panic!("issue id")
                };
                id
            }
        }
    }
    pub fn close(&mut self, number: u64) {
        match self {
            Self::Forgejo(w) => w.close(number),
            Self::Github(w) => {
                w.direct(raw::Op::Write(raw::Write::Close { number })).expect("person close");
            }
        }
    }
    pub fn accounting(&self) -> Accounting {
        match self {
            Self::Forgejo(w) => {
                let requests = w.request_counts();
                Accounting {
                    reported: 0,
                    written: requests.written,
                    served: w.tally().total.checked_sub(w.start_counts().served).expect("startup entrances served"),
                    cut_in_head: requests.cut_in_head,
                }
            }
            Self::Github(w) => w.accounting(),
        }
    }
    pub fn lines(&self) -> &[Vec<u8>] {
        match self {
            Self::Forgejo(w) => w.request_lines(),
            Self::Github(w) => w.lines(),
        }
    }
    pub fn delay_creation(&mut self) {
        match self {
            Self::Forgejo(w) => {
                let latency = crate::fixtures::LIMITS.lifetime.saturating_mul(2);
                w.fake_env.limits.latency_min = latency;
                w.fake_env.limits.latency_max = latency;
            }
            Self::Github(w) => w.lose_creation(),
        }
    }
    pub fn restore_latency(&mut self) {
        match self {
            Self::Forgejo(w) => {
                let config = machine::fake_config();
                w.fake_env.limits.latency_min = config.latency_min;
                w.fake_env.limits.latency_max = config.latency_max;
            }
            Self::Github(_) => {}
        }
    }
    pub fn secrets(&mut self) -> Vec<skein_lib::Secret> {
        match self {
            Self::Forgejo(_) => Vec::new(),
            Self::Github(w) => std::mem::take(&mut w.secrets),
        }
    }
    pub fn mints(&self) -> u64 {
        match self {
            Self::Forgejo(_) => 0,
            Self::Github(w) => w.mints,
        }
    }
    pub fn rotate(&mut self, key: &'static [u8]) {
        match self {
            Self::Forgejo(_) => panic!("GitHub key replacement"),
            Self::Github(w) => w.rotate(key),
        }
    }
}
