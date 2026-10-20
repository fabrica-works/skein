//! One GitHub forge's bounded delivery-log poll (forge-connection.md, 6.5 and 8.7).
use crate::Limits;
use alloc::boxed::Box;
use skein_forge::{api, github::log};
use skein_lib::{Env, Time, Token};

#[derive(Debug)]
pub(crate) enum State {
    Resting,
    Reading,
    Delivery { id: u64 },
    Holding { hint: Option<api::Hint>, offer: bool },
}
#[derive(Debug)]
pub(crate) struct Poll {
    newest: Option<[u8; 16]>,
    validator: Option<api::Validator>,
    ask: Option<log::Ask>,
    rows: Box<[log::Row]>,
    row: usize,
    pages: u32,
    next: Option<log::Cursor>,
    installations: bool,
}
#[derive(Debug)]
pub(crate) struct Puller {
    pub newest: Option<[u8; 16]>,
    pub validator: Option<api::Validator>,
    pub due: Option<Time>,
    pub state: State,
    pub source: Option<Token>,
    pub poll: Option<Poll>,
}
pub(crate) enum Action {
    Idle,
    Progress,
    Installations,
    Gap { reread: bool },
    Hint(api::Hint),
}
impl Puller {
    pub fn new() -> Self {
        Self { newest: None, validator: None, due: None, state: State::Resting, source: None, poll: None }
    }
    pub fn pause(&mut self) {
        self.state = State::Resting;
        self.poll = None;
        self.source = None;
        self.due = None;
    }
    pub fn finish(&mut self, env: &Env<Limits>, commit: bool) {
        if let Some(poll) = self.poll.take()
            && commit
        {
            self.newest = poll.newest.or(self.newest);
            self.validator = poll.validator;
        }
        self.state = State::Resting;
        self.due = Some(env.now.saturating_add(env.limits.log_poll));
    }
    // Admission has had an entry point to refuse the preceding offer.
    pub fn admitted(&mut self) {
        match &self.state {
            State::Holding { hint: None, .. } => self.state = State::Reading,
            State::Resting | State::Reading | State::Delivery { .. } | State::Holding { hint: Some(_), .. } => {}
        }
    }
    pub fn refused(&mut self, hint: api::Hint) {
        match &mut self.state {
            State::Holding { hint: held, offer } => {
                *held = Some(hint);
                *offer = false;
            }
            State::Resting | State::Reading | State::Delivery { .. } => {}
        }
    }
    pub fn offer(&mut self) {
        match &mut self.state {
            State::Holding { offer, .. } => *offer = true,
            State::Resting | State::Reading | State::Delivery { .. } => {}
        }
    }
    pub fn hold(&mut self, hint: api::Hint) {
        self.state = State::Holding { hint: Some(hint), offer: true };
    }
    pub fn reading(&mut self) {
        self.state = State::Reading;
    }
    pub fn request(&mut self) -> Option<super::component::OwnPlan> {
        match self.state {
            State::Reading => {
                let ask = self.poll.as_mut().expect("active poll").ask.take()?;
                Some(super::component::OwnPlan::GitHubLogPage(Box::new(
                    skein_forge::github::plans::start::LogPage::new(ask),
                )))
            }
            State::Delivery { id } => Some(super::component::OwnPlan::GitHubLogDelivery(Box::new(
                skein_forge::github::plans::start::LogDelivery::new(id, {
                    let poll = self.poll.as_ref().expect("active poll");
                    let at = poll.row.checked_sub(1).expect("selected row");
                    poll.rows.get(at).expect("selected row").event.clone()
                }),
            ))),
            State::Resting | State::Holding { .. } => None,
        }
    }
    pub fn wants_request(&self) -> bool {
        match self.state {
            State::Reading => self.poll.as_ref().expect("active poll").ask.is_some(),
            State::Delivery { .. } => true,
            State::Resting | State::Holding { .. } => false,
        }
    }
    pub fn page(&mut self, page: log::Page) {
        let poll = self.poll.as_mut().expect("active poll");
        if poll.pages == 0 {
            poll.validator = page.validator;
        }
        poll.pages = poll.pages.checked_add(1).expect("bounded log pages");
        poll.rows = page.rows;
        poll.row = 0;
        poll.next = page.next;
    }
    pub fn fire(&mut self, env: &Env<Limits>) -> bool {
        match self.state {
            State::Resting => {}
            State::Reading | State::Delivery { .. } | State::Holding { .. } => return false,
        }
        let due = self.due.get_or_insert(env.now.saturating_add(env.limits.log_poll));
        if *due > env.now {
            return false;
        }
        self.due = None;
        self.poll = Some(Poll {
            newest: None,
            validator: None,
            ask: Some(log::Ask::First { validator: self.validator.clone() }),
            rows: Box::from([]),
            row: 0,
            pages: 0,
            next: None,
            installations: false,
        });
        self.state = State::Reading;
        true
    }
    pub fn local(&mut self, env: &Env<Limits>) -> Action {
        match &mut self.state {
            State::Resting => {
                if self.fire(env) {
                    Action::Progress
                } else {
                    Action::Idle
                }
            }
            State::Holding { hint, offer } => {
                if *offer {
                    match hint.take() {
                        Some(hint) => Action::Hint(hint),
                        None => Action::Idle,
                    }
                } else {
                    Action::Idle
                }
            }
            State::Delivery { .. } => Action::Idle,
            State::Reading => self.row(env),
        }
    }
    fn row(&mut self, env: &Env<Limits>) -> Action {
        let poll = self.poll.as_mut().expect("active poll");
        if poll.ask.is_some() {
            return Action::Idle;
        }
        let Some(row) = poll.rows.get(poll.row) else {
            if let Some(cursor) = poll.next.take() {
                if poll.pages >= env.limits.log_pages {
                    return self.end(env, true);
                }
                poll.rows = Box::from([]);
                poll.ask = Some(log::Ask::Next { cursor });
                return Action::Progress;
            }
            return self.end(env, false);
        };
        poll.row = poll.row.checked_add(1).expect("bounded page index");
        if row.redelivery {
            return Action::Progress;
        }
        if poll.newest.is_none() {
            poll.newest = Some(row.guid);
        }
        if self.newest.is_none() || self.newest == Some(row.guid) {
            return self.end(env, false);
        }
        if env.wall.as_nanos().saturating_sub(row.delivered_at.as_nanos()) > env.limits.log_catch_up.as_nanos() {
            return self.end(env, true);
        }
        match row.kind() {
            log::Kind::Installations => {
                if poll.installations {
                    Action::Progress
                } else {
                    poll.installations = true;
                    Action::Installations
                }
            }
            log::Kind::Hinted | log::Kind::Repositories => {
                self.state = State::Delivery { id: row.id };
                Action::Progress
            }
            log::Kind::Other => Action::Progress,
        }
    }
    fn end(&mut self, env: &Env<Limits>, gap: bool) -> Action {
        let reread = !self.poll.as_ref().expect("active poll").installations;
        let gap = gap && self.newest.is_some();
        self.finish(env, true);
        if gap { Action::Gap { reread } } else { Action::Progress }
    }
}
