//! One bounded plan and its outstanding owner or forge need (6.3).
use super::{Answer, Need, Plan};
use crate::{
    adopt::{self, Adopt},
    call::{Auth, Call},
    check::{self, Check},
    given::Given,
    item::{self, Bounds, Item},
    reachable::{self, Reachable},
    reconcile::{self, Event, Reconcile, Reply, Why},
    take_over::{self, TakeOver},
};
use alloc::boxed::Box;
use skein_forge::forgejo::request::Request;
use skein_lib::{Duration, Queue, Secret, Time, Token};
/// The forge's settings given, with the plan's adopted repositories.
pub(crate) fn given_for(forge: &Given, adopted: &[u64]) -> Given {
    let mut given = forge.clone();
    given.adopted = adopted.into();
    given
}
/// One machine per plan, sharing the host's need and connection loop.
#[derive(Debug)]
pub(crate) enum Machine {
    Reconcile(Box<Reconcile>),
    Check(Check),
    TakeOver(TakeOver),
    Adopt(Adopt),
    Reachable(Reachable),
}
impl Machine {
    pub(crate) fn step(&mut self, event: Event, out: &mut Queue<reconcile::Need>) {
        match self {
            Self::Reconcile(machine) => reconcile::step(machine, event, out),
            Self::Check(machine) => check::step(machine, event, out),
            Self::TakeOver(machine) => take_over::step(machine, event, out),
            Self::Adopt(machine) => adopt::step(machine, event, out),
            Self::Reachable(machine) => reachable::step(machine, event, out),
        }
    }
    /// The host halts only machines that have received Begin.
    pub(crate) fn halt(&mut self, why: Why, out: &mut Queue<reconcile::Need>) {
        match self {
            Self::Reconcile(machine) => reconcile::halt(machine, why, out),
            Self::Check(_) | Self::TakeOver(_) | Self::Adopt(_) | Self::Reachable(_) => self.halt_begun(why, out),
        }
    }
    fn halt_begun(&mut self, why: Why, out: &mut Queue<reconcile::Need>) {
        let mut stopped = Queue::with_capacity(1);
        self.step(Event::Begin, &mut stopped);
        match stopped.pop() {
            Some(reconcile::Need::Stop(mut failure)) => {
                failure.why = why;
                out.push(reconcile::Need::Stop(failure));
            }
            Some(
                reconcile::Need::Send { .. }
                | reconcile::Need::Read(_)
                | reconcile::Need::Write { .. }
                | reconcile::Need::Draw(_)
                | reconcile::Need::Done(_),
            )
            | None => (),
        }
    }
}
#[derive(Debug)]
pub(super) enum Owner {
    Read(Item),
    Write,
}
#[derive(Debug)]
pub(super) struct Send {
    pub call: Call,
    pub auth: Auth,
}
/// What a Forgejo forge sends: the request and the token it carries.
pub(crate) fn forgejo_pair(call: Call, auth: Auth) -> Result<(Request, Secret), Why> {
    match call {
        Call::Forgejo(request) => match auth {
            Auth::Token(token) => Ok((request, token)),
            Auth::App { .. } | Auth::Bearer(_) | Auth::None => Err(Why::Unexpected),
        },
        Call::GitHub(_) => Err(Why::Unexpected),
    }
}
#[derive(Debug)]
pub(super) struct Run {
    pub bounds: Bounds,
    pub forge: u16,
    pub writes: bool,
    pub machine: Machine,
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
    pub fn new(given: &Given, plan: Plan, bounds: Bounds, now: Time, out: &mut Queue<reconcile::Need>) -> Self {
        let writes = plan.writes();
        let machine = match plan {
            Plan::Reconcile { adopted, .. } => Machine::Reconcile(Box::new(
                Reconcile::new(&given_for(given, &adopted)).expect("the owner checks a plan's adopted repositories"),
            )),
            Plan::Check { forge } => Machine::Check(Check::new(forge)),
            Plan::TakeOver { admin_token, .. } => {
                let taken = if item::token(admin_token.expose(), bounds.token_bytes).is_some() {
                    admin_token
                } else {
                    Secret::new(Box::new([]))
                };
                Machine::TakeOver(TakeOver::new(given, taken).expect("host checks the given"))
            }
            Plan::Adopt { repository, .. } => Machine::Adopt(
                Adopt::new(given, repository.get()).expect("host checks the given and the repository is nonzero"),
            ),
            Plan::Reachable { installation, page, .. } => Machine::Reachable(
                Reachable::new(given, installation, page.get()).expect("host checks the given and the page is nonzero"),
            ),
        };
        let mut run = Self {
            bounds,
            forge: given.forge,
            writes,
            machine,
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
        self.machine.step(event, out);
        self.pending = out.pop();
    }
    pub fn halt(&mut self, why: Why, out: &mut Queue<reconcile::Need>) {
        self.machine.halt(why, out);
        if let Some(need) = out.pop() {
            self.pending = Some(need);
        }
        self.owner = None;
        self.send = None;
        self.retry_at = None;
        self.retry = false;
    }
    pub fn answer(&mut self, answer: Answer, out: &mut Queue<reconcile::Need>) {
        let Some(owner) = self.owner.take() else {
            return;
        };
        let event = match answer {
            Answer::Written => Event::Written,
            Answer::Read(mut value) => {
                match owner {
                    Owner::Read(item) => {
                        if let Some(bytes) = &value
                            && !item::fits(item, bytes.expose(), &self.bounds)
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
            Reply::Document(_)
            | Reply::Rows(_)
            | Reply::Empty
            | Reply::GithubDocument(_)
            | Reply::GithubRows(_)
            | Reply::Unsigned => false,
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
