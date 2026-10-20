//! One bounded plan and its outstanding owner or forge need (6.3).
use super::{Answer, Need};
use crate::{
    given::Given,
    item::{self, Item},
    reconcile::{self, Event, Reconcile, Reply, Why},
};
use skein_forge::forgejo::request::Request;
use skein_lib::{Duration, Queue, Secret, Time, Token};
/// The forge's settings given, with the plan's adopted repositories.
pub(crate) fn given_for(forge: &Given, adopted: &[u64]) -> Given {
    let mut given = forge.clone();
    given.adopted = adopted.into();
    given
}
#[derive(Debug)]
pub(super) enum Owner {
    Read(Item),
    Write,
}
#[derive(Debug)]
pub(super) struct Send {
    pub request: Request,
    pub token: Secret,
}
#[derive(Debug)]
pub(super) struct Run {
    pub forge: u16,
    pub machine: Reconcile,
    pub deadline: Time,
    pub pending: Option<reconcile::Need>,
    pub owner: Option<Owner>,
    pub send: Option<Send>,
    pub connection: Option<Token>,
    pub retry: bool,
    pub retry_at: Option<Time>,
    pub backoff: Duration,
    pub backoff_max: Duration,
    pub answered: bool,
}
impl Run {
    pub fn new(given: &Given, now: Time, out: &mut Queue<reconcile::Need>) -> Self {
        let mut run = Self {
            forge: given.forge,
            machine: Reconcile::new(given).expect("the owner checks a plan's adopted repositories"),
            deadline: now.saturating_add(given.limits.deadline),
            pending: None,
            owner: None,
            send: None,
            connection: None,
            retry: false,
            retry_at: None,
            backoff: given.limits.backoff,
            backoff_max: given.limits.backoff_max,
            answered: false,
        };
        run.step(Event::Begin, out);
        run
    }
    pub fn step(&mut self, event: Event, out: &mut Queue<reconcile::Need>) {
        reconcile::step(&mut self.machine, event, out);
        self.pending = out.pop();
    }
    pub fn halt(&mut self, why: Why, out: &mut Queue<reconcile::Need>) {
        reconcile::halt(&mut self.machine, why, out);
        if let Some(need) = out.pop() {
            self.pending = Some(need);
        }
        self.owner = None;
        self.send = None;
        self.retry_at = None;
        self.retry = false;
    }
    pub fn answer(&mut self, answer: Answer, token_bytes: u32, out: &mut Queue<reconcile::Need>) {
        let Some(owner) = self.owner.take() else {
            return;
        };
        let event = match answer {
            Answer::Written => Event::Written,
            Answer::Read(mut value) => {
                match owner {
                    Owner::Read(item) => {
                        if let Some(bytes) = &value
                            && !item::fits(item, bytes.expose(), token_bytes)
                        {
                            value = None;
                        }
                    }
                    Owner::Write => (),
                }
                Event::Read(value)
            }
        };
        self.step(event, out);
    }
    pub fn reply(&mut self, reply: Reply, out: &mut Queue<reconcile::Need>) {
        let retry = match &reply {
            Reply::Down => true,
            Reply::Status(status) => (500..600).contains(status),
            Reply::Document(_) | Reply::Rows(_) | Reply::Empty => false,
        };
        if !self.answered && retry {
            self.retry = true;
            return;
        }
        self.answered = true;
        self.send = None;
        self.step(Event::Reply(reply), out);
    }
    pub fn closed(&mut self, now: Time) {
        self.connection = None;
        if self.retry {
            self.retry = false;
            self.retry_at = Some(now.saturating_add(self.backoff));
            self.backoff = self.backoff.saturating_mul(2).min(self.backoff_max);
        }
    }
    pub fn owner_need(&mut self, need: Need) -> Need {
        self.owner = Some(match &need {
            Need::Read(item) => Owner::Read(*item),
            Need::Write { .. } => Owner::Write,
        });
        need
    }
}
