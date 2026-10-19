//! Shared writes read once under the owner's lock (oauth.md, section 6.4).

use super::{Component, Lifecycle, State, grant, held, names::number, recorded};
use crate::accounts::{Source, copy_registration};
use crate::exchange::{Exchange, OwnerLock, Stage};
use crate::keeper::{Operation, Store};
use crate::{Event, Failure, Limits, Locked, Lower};
use skein_lib::{Env, Id, Queue};
use skein_oauth as oauth;

impl Component {
    pub(super) fn lock_write(&mut self, env: &Env<Limits>, id: Id<Exchange>, up: &mut Queue<Event>) -> bool {
        let exchange = self.exchanges.get_mut(id).expect("bound exchange");
        let shared = match self.sources.get(exchange.account).expect("account source") {
            Source::SignIn { shared, .. } => *shared,
            Source::HandedIn => false,
        };
        if !shared || exchange.lock == OwnerLock::Held {
            return false;
        }
        exchange.lock = OwnerLock::Waiting { until: env.now.saturating_add(env.limits.lock_wait) };
        exchange.stage = Stage::Locking;
        up.push(Event::Lock { account: number(&self.names, exchange.account) });
        true
    }

    pub(super) fn locked(
        &mut self,
        env: &Env<Limits>,
        account: u32,
        answer: Locked,
        up: &mut Queue<Event>,
        io: &mut Queue<Lower>,
    ) {
        if self.lifecycle == Lifecycle::Closed {
            return;
        }
        let binding = *self.bindings.get(account).expect("account binding");
        let pending = match binding {
            Some(id) => match self.exchanges.get(id).expect("bound exchange").lock {
                OwnerLock::Waiting { .. } => Some(id),
                OwnerLock::Released | OwnerLock::Held => None,
            },
            None => None,
        };
        let Some(id) = pending else {
            if let Locked::Held { .. } = answer {
                up.push(Event::Unlock { account: number(&self.names, account) });
            }
            return;
        };
        match answer {
            Locked::Busy => {
                self.exchanges.get_mut(id).expect("bound exchange").lock = OwnerLock::Released;
                self.fail_exchange(env, id, Failure::Busy, up, io);
                self.collect(id, up);
            }
            Locked::Held { record } => {
                let exchange = self.exchanges.get_mut(id).expect("bound exchange");
                exchange.lock = OwnerLock::Held;
                if let Stage::Finished = exchange.stage {
                    self.collect(id, up);
                    return;
                }
                match self.keepers.get_mut(account).expect("account keeper") {
                    Store::Owner => self.read_locked(env, id, record, up, io),
                    Store::Private(keeper) => {
                        exchange.stage = Stage::Reading;
                        exchange.file_active = true;
                        keeper.reload();
                    }
                }
            }
        }
    }

    pub(super) fn lock_loaded(
        &mut self,
        env: &Env<Limits>,
        account: u32,
        record: &mut Option<oauth::SavedToken>,
        failure: Option<Failure>,
        up: &mut Queue<Event>,
        io: &mut Queue<Lower>,
    ) -> bool {
        if let Some(id) = *self.bindings.get(account).expect("account binding") {
            let exchange = self.exchanges.get_mut(id).expect("bound exchange");
            if exchange.lock == OwnerLock::Held && exchange.file_active {
                exchange.file_active = false;
                match exchange.stage {
                    Stage::Reading => {
                        if let Some(failure) = failure {
                            self.fail_exchange(env, id, failure, up, io);
                            self.collect(id, up);
                        } else {
                            self.read_locked(env, id, record.take(), up, io);
                        }
                        return true;
                    }
                    Stage::Finished => {
                        self.collect(id, up);
                        return true;
                    }
                    Stage::Loading
                    | Stage::Locking
                    | Stage::Running
                    | Stage::Keeping { .. }
                    | Stage::Forgetting { .. } => unreachable!("shared file read"),
                }
            }
        }
        false
    }

    pub(super) fn read_locked(
        &mut self,
        env: &Env<Limits>,
        id: Id<Exchange>,
        record: Option<oauth::SavedToken>,
        up: &mut Queue<Event>,
        io: &mut Queue<Lower>,
    ) {
        let exchange = self.exchanges.get_mut(id).expect("bound exchange");
        exchange.file_active = false;
        let account = exchange.account;
        let different = match self.accounts.get(account).expect("account state") {
            State::Record { record: prior, .. } => match &record {
                Some(record) => record.subject != prior.subject,
                None => false,
            },
            State::Empty | State::Loaded { .. } => false,
        };
        if different && !exchange.signing_in() {
            self.fail_exchange(env, id, Failure::AnotherSubject, up, io);
            *self.accounts.get_mut(account).expect("account state") = State::Empty;
            self.collect(id, up);
            return;
        }
        if let Some((candidate, _)) = &mut exchange.lock_candidate {
            if record.is_none() {
                *self.accounts.get_mut(account).expect("account state") = State::Empty;
            }
            if let Some(record) = record {
                let Some(next) = record.generation.checked_add(1) else {
                    self.fail_exchange(env, id, Failure::Exchange(oauth::Failure::Limit), up, io);
                    self.collect(id, up);
                    return;
                };
                candidate.generation = candidate.generation.max(next);
            }
            self.keep_candidate(id, up);
            return;
        }
        let slot = self.accounts.get_mut(account).expect("account state");
        match record {
            Some(record) => {
                let newer = match slot {
                    State::Record { record: prior, .. } => {
                        record.generation > prior.generation && record.subject == prior.subject
                    }
                    State::Empty | State::Loaded { .. } => false,
                };
                if newer {
                    let holding = held(slot);
                    *slot = recorded(record, env, holding);
                    let ready = match slot {
                        State::Record { lead, .. } => env.now < *lead,
                        State::Empty | State::Loaded { .. } => unreachable!("record just read"),
                    };
                    if ready && !exchange.signing_out() {
                        if holding || exchange.pending_grant {
                            grant(
                                slot,
                                self.sources.get(account).expect("account source"),
                                env.now,
                                number(&self.names, account),
                                up,
                            );
                        }
                        exchange.stage = Stage::Finished;
                        self.collect(id, up);
                        return;
                    }
                }
            }
            None => {
                *slot = State::Empty;
                if exchange.signing_out() {
                    self.finish_sign_out(env, id, false, true, up, io);
                } else {
                    self.fail_exchange(env, id, Failure::Expired, up, io);
                }
                self.collect(id, up);
                return;
            }
        }
        if exchange.signing_out() {
            self.begin_sign_out(env, account, up, io);
        } else {
            self.begin_refresh(env, id);
        }
    }

    pub(super) fn keep_candidate(&mut self, id: Id<Exchange>, up: &mut Queue<Event>) {
        let exchange = self.exchanges.get_mut(id).expect("bound exchange");
        let (record, missing) = exchange.lock_candidate.take().expect("candidate waiting to keep");
        let generation = record.generation;
        match self.keepers.get_mut(exchange.account).expect("account keeper") {
            Store::Owner => {
                up.push(Event::Keep { account: number(&self.names, exchange.account), record: record.clone() });
            }
            Store::Private(keeper) => {
                keeper.next = Some(Operation::Store { generation });
                exchange.file_active = true;
            }
        }
        exchange.stage = Stage::Keeping { candidate: record, missing };
    }

    pub(super) fn start_refresh(
        &mut self,
        env: &Env<Limits>,
        account: u32,
        pending_grant: bool,
        up: &mut Queue<Event>,
    ) {
        if let State::Record { due, attempted, .. } = self.accounts.get_mut(account).expect("account state") {
            *due = false;
            *attempted = true;
        }
        let client = oauth::Client::new(env.limits.client).expect("validated client limits");
        let exchange = Exchange::new(account, client, pending_grant);
        let id = match self.exchanges.insert(exchange) {
            Ok(id) => id,
            Err(_) => unreachable!("exchange slot admitted"),
        };
        *self.bindings.get_mut(account).expect("account binding") = Some(id);
        if !self.lock_write(env, id, up) {
            self.begin_refresh(env, id);
        }
    }

    fn begin_refresh(&mut self, env: &Env<Limits>, id: Id<Exchange>) {
        let exchange = self.exchanges.get_mut(id).expect("bound exchange");
        let account = exchange.account;
        let prior = match self.accounts.get_mut(account).expect("account") {
            State::Record { record, due, attempted, .. } => {
                *due = false;
                *attempted = true;
                record.refresh_state().expect("refreshable account")
            }
            State::Empty | State::Loaded { .. } => unreachable!("refreshable account"),
        };
        let registration = match self.sources.get(account).expect("account source") {
            Source::SignIn { registration, .. } => copy_registration(registration),
            Source::HandedIn => unreachable!("sign-in source"),
        };
        exchange.stage = Stage::Running;
        exchange.client.step(oauth::Event::Refresh { registration, prior, now: env.now }, &mut exchange.above);
    }
}
