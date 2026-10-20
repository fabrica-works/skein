//! One page of an admin's repositories for the picker (forge-setup.md, 4.8).
use crate::{
    given::{self, Given, Place, Refused},
    item::Item,
    reconcile::{self, Event, Failure, Need, Outcome, Reply, Stage, Why, reply, unexpected},
};
use alloc::boxed::Box;
use skein_forge::forgejo::{
    document::{Document, Row},
    request::{Page, Request},
};
use skein_lib::{Decimal, List, Queue, Secret, bytes};

/// Room the host reserves before stepping.
pub const MAX_OUT: u32 = 1;

/// One repository of a picker's page (forge-setup.md, 4.8, step 4).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Reached {
    pub id: u64,
    pub owner_id: u64,
    /// The owner's login.
    pub owner: Box<[u8]>,
    pub name: Box<[u8]>,
    pub private: bool,
}

#[derive(Debug)]
enum State {
    Begin,
    Read,
    Admin,
    Settings,
    Search,
    Ended,
}

/// One picker page; the admin's token is held only until the plan ends.
#[derive(Debug)]
pub struct Reachable {
    forge: u16,
    admin_name: Box<[u8]>,
    installation: Option<u64>,
    page: u32,
    limit: u32,
    state: State,
    token: Option<Secret>,
}

impl Reachable {
    /// Refuses invalid given values or a page of zero; pages start at one.
    pub fn new(given: &Given, installation: Option<u64>, page: u32) -> Result<Self, Refused> {
        given::check(given)?;
        if page == 0 {
            return Err(Refused { place: Place::Page, other: None });
        }
        Ok(Self {
            forge: given.forge,
            admin_name: given.admin_name.clone(),
            installation,
            page,
            limit: given.limits.reachable,
            state: State::Begin,
            token: None,
        })
    }

    fn send(&self, request: Request) -> Need {
        Need::Send { request, token: self.token.clone().expect("admin token read before requests") }
    }

    fn stop(&mut self, step: Stage, subject: Box<[u8]>, why: Why) -> Need {
        self.state = State::Ended;
        self.token = None;
        Need::Stop(Failure { forge: self.forge, step, subject, why })
    }

    fn advance(&mut self, state: State, event: Event) -> Need {
        match state {
            State::Begin => match event {
                Event::Begin => {
                    if let Some(id) = self.installation {
                        let subject = bytes::copy_of(Decimal::of(id).as_bytes());
                        return self.stop(Stage::Installation, subject, Why::Unexpected);
                    }
                    self.state = State::Read;
                    Need::Read(Item::AdminToken)
                }
                Event::Read(_) | Event::Reply(_) | Event::Written | Event::Drawn(_) => {
                    self.stop(Stage::Admin, self.admin_name.clone(), Why::Unexpected)
                }
            },
            State::Read => match event {
                Event::Read(Some(token)) => {
                    self.token = Some(token);
                    self.state = State::Admin;
                    self.send(Request::User)
                }
                Event::Read(None) => self.stop(Stage::Token, bytes::copy_of(b"api"), Why::Missing),
                Event::Begin | Event::Reply(_) | Event::Written | Event::Drawn(_) => {
                    self.stop(Stage::Admin, self.admin_name.clone(), Why::Unexpected)
                }
            },
            State::Admin => match reconcile::account(event) {
                Ok(user) => {
                    if !user.is_admin {
                        return self.stop(Stage::Admin, self.admin_name.clone(), Why::NotAdmin);
                    }
                    self.state = State::Settings;
                    self.send(Request::ApiSettings)
                }
                Err(why) => self.stop(Stage::Admin, self.admin_name.clone(), why),
            },
            State::Settings | State::Search => match self.repositories(state, event) {
                Ok(need) => need,
                Err(why) => self.stop(Stage::Token, self.admin_name.clone(), why),
            },
            State::Ended => unreachable!("terminal machine is not advanced"),
        }
    }

    fn repositories(&mut self, state: State, event: Event) -> Result<Need, Why> {
        match state {
            State::Settings => match reply(event)? {
                Reply::Document(Document::ApiSettings(settings)) if settings.max_response_items > 0 => {
                    self.limit = core::cmp::min(self.limit, settings.max_response_items);
                    self.state = State::Search;
                    Ok(self.send(Request::RepoSearch { page: Page { page: Some(self.page), limit: Some(self.limit) } }))
                }
                other @ (Reply::Document(_) | Reply::Rows(_) | Reply::Empty | Reply::Status(_) | Reply::Down) => {
                    Err(unexpected(other))
                }
            },
            State::Search => self.rows(reply(event)?),
            State::Begin | State::Read | State::Admin | State::Ended => unreachable!("repository phases only"),
        }
    }

    fn rows(&mut self, answer: Reply) -> Result<Need, Why> {
        let rows = match answer {
            Reply::Rows(rows) => rows,
            other @ (Reply::Document(_) | Reply::Empty | Reply::Status(_) | Reply::Down) => {
                return Err(unexpected(other));
            }
        };
        let limit = usize::try_from(self.limit).expect("u32 fits");
        if rows.len() > limit {
            return Err(Why::Unexpected);
        }
        let more = rows.len() == limit;
        let mut reached = List::with_capacity(u32::try_from(rows.len()).expect("bounded page"));
        for row in rows {
            match row {
                Row::Found(row) => {
                    reached
                        .push(Reached {
                            id: row.repository,
                            owner_id: row.owner,
                            owner: row.login,
                            name: row.name,
                            private: row.private,
                        })
                        .expect("bounded page");
                }
                Row::Reachable(_)
                | Row::Hook(_)
                | Row::Token(_)
                | Row::Team(_)
                | Row::User(_)
                | Row::Issue(_)
                | Row::Comment(_)
                | Row::Status(_)
                | Row::Review(_)
                | Row::ReviewComment(_)
                | Row::Branch(_)
                | Row::Protection(_)
                | Row::Pull(_)
                | Row::File(_)
                | Row::Commit(_)
                | Row::Run(_)
                | Row::Job(_)
                | Row::Step(_) => return Err(Why::Unexpected),
            }
        }
        self.state = State::Ended;
        self.token = None;
        Ok(Need::Done(Outcome::Reachable { rows: reached.into_boxed(), more }))
    }
}

/// Consumes one event and emits at most one need; terminal machines ignore later events.
pub fn step(machine: &mut Reachable, event: Event, out: &mut Queue<Need>) {
    let event = reconcile::normal(event);
    let state = core::mem::replace(&mut machine.state, State::Ended);
    match state {
        State::Ended => (),
        State::Begin | State::Read | State::Admin | State::Settings | State::Search => {
            let need = machine.advance(state, event);
            out.push(need);
        }
    }
}
