//! The owner's sign-in admission and confidential redirect boundary
//! (oauth.md, sections 6.3 and 6.5). Listener heads use the same decoder;
//! the OAuth client owns state authentication and the exchange's deadline.

use super::names::number;
use super::{Component, State};
use crate::accounts::{Source, copy_registration};
use crate::exchange::{Exchange, Stage};
use crate::keeper::Store;
use crate::listener::Listener;
use crate::redirect::{self, Redirect};
use crate::route::{self, Socket};
use crate::{Asked, Ends, Event, Failure, Limits, Lower, Refusal};
use alloc::boxed::Box;
use skein_lib::{CryptoRng, Env, Queue, Time, Writer};
use skein_oauth as oauth;

impl Component {
    pub(super) fn return_to(&mut self, account: u32, location: Box<[u8]>) {
        if location.is_empty()
            || location.len() > crate::RETURN_BYTES
            || !(location.starts_with(b"http://") || location.starts_with(b"https://"))
        {
            return;
        }
        for byte in &location {
            if !(0x21..=0x7e).contains(byte) {
                return;
            }
        }
        let id = match self.bindings.get(account) {
            Some(Some(id)) => *id,
            Some(None) | None => return,
        };
        let exchange = self.exchanges.get_mut(id).expect("bound exchange");
        if !exchange.signing_in() {
            return;
        }
        match exchange.stage {
            Stage::Finished => return,
            Stage::Loading
            | Stage::Locking
            | Stage::Reading
            | Stage::Running
            | Stage::Keeping { .. }
            | Stage::Forgetting { .. } => {}
        }
        match &mut exchange.listener {
            Some(listener) => listener.return_location = Some(location),
            None => exchange.return_location = Some(location),
        }
    }

    pub(super) fn sign_in(
        &mut self,
        env: &Env<Limits>,
        account: u32,
        subject: Option<Box<[u8]>>,
        up: &mut Queue<Event>,
        io: &mut Queue<Lower>,
    ) {
        if self.refused(account, Asked::SignIn, up) {
            return;
        }
        if let Some(subject) = &subject
            && subject.len() > usize::try_from(env.limits.client.document.subject_bytes).expect("u32 fits")
        {
            up.push(Event::Failed {
                account: number(&self.names, account),
                ends: Ends::SignIn,
                failure: Failure::Exchange(oauth::Failure::Limit),
            });
            return;
        }
        let registration = match self.sources.get(account).expect("configured source") {
            Source::SignIn { registration, .. } => copy_registration(registration),
            Source::HandedIn => {
                up.push(Event::Refused {
                    account: number(&self.names, account),
                    asked: Asked::SignIn,
                    why: Refusal::Source,
                });
                return;
            }
        };
        if self.bindings.get(account) != Some(&None) {
            up.push(Event::Refused { account: number(&self.names, account), asked: Asked::SignIn, why: Refusal::Busy });
            return;
        }
        if self.exchanges.is_full() {
            up.push(Event::Refused {
                account: number(&self.names, account),
                asked: Asked::SignIn,
                why: Refusal::Full { bound: env.limits.exchanges },
            });
            return;
        }
        let public = registration.client_secret.is_none();
        if public {
            let mut listeners = 0_u32;
            for binding in &self.bindings {
                match binding {
                    Some(id) => {
                        let exchange = self.exchanges.get(*id).expect("bound exchange");
                        let reserved = match exchange.stage {
                            Stage::Loading => match self.sources.get(exchange.account).expect("account source") {
                                Source::SignIn { registration, .. } => registration.client_secret.is_none(),
                                Source::HandedIn => false,
                            },
                            Stage::Locking
                            | Stage::Reading
                            | Stage::Running
                            | Stage::Keeping { .. }
                            | Stage::Forgetting { .. }
                            | Stage::Finished => false,
                        };
                        let listening = match &exchange.listener {
                            Some(listener) => !listener.settled(),
                            None => false,
                        };
                        if reserved || listening {
                            listeners = listeners.checked_add(1).expect("bounded listeners");
                        }
                    }
                    None => {}
                }
            }
            if listeners >= env.limits.listeners {
                up.push(Event::Refused {
                    account: number(&self.names, account),
                    asked: Asked::SignIn,
                    why: Refusal::Full { bound: env.limits.listeners },
                });
                return;
            }
        }
        let client = oauth::Client::new(env.limits.client).expect("validated client");
        let mut exchange = Exchange::new(account, client, false);
        exchange.subject = subject;
        exchange.sign_in = Some(prepare_sign_in(registration, number(&self.names, account), env, &mut self.crypto));
        exchange.stage = Stage::Loading;
        exchange.purpose = crate::exchange::Purpose::SignIn { waiting: false };
        let id = match self.exchanges.insert(exchange) {
            Ok(id) => id,
            Err(_) => unreachable!("admitted exchange slot"),
        };
        *self.bindings.get_mut(account).expect("account binding") = Some(id);
        let must_load = match self.accounts.get(account).expect("account state") {
            State::Empty => match self.keepers.get_mut(account).expect("account keeper") {
                Store::Private(keeper) => {
                    keeper.reload();
                    true
                }
                Store::Owner => false,
            },
            State::Record { .. } | State::Loaded { .. } => false,
        };
        if !must_load {
            self.begin_sign_in(env, account, io);
        }
    }

    pub(super) fn begin_sign_in(&mut self, _env: &Env<Limits>, account: u32, io: &mut Queue<Lower>) {
        let registration = match self.sources.get(account).expect("configured source") {
            Source::SignIn { registration, .. } => copy_registration(registration),
            Source::HandedIn => unreachable!("admitted sign-in source"),
        };
        let public = registration.client_secret.is_none();
        let id = self.bindings.get(account).expect("account binding").expect("admitted exchange");
        let exchange = self.exchanges.get_mut(id).expect("bound exchange");
        let event = exchange.sign_in.as_mut().expect("prepared sign-in");
        match event {
            oauth::Event::SignIn { generation, issued_client_id, id_token, subject, .. } => {
                match self.accounts.get(account).expect("account") {
                    State::Record { record, .. } | State::Loaded { record } => {
                        *generation = record.generation;
                        issued_client_id.clone_from(&record.issued_client_id);
                        id_token.clone_from(&record.id_token);
                        subject.clone_from(&record.subject);
                    }
                    State::Empty => {}
                }
            }
            oauth::Event::Redirected { .. }
            | oauth::Event::Refresh { .. }
            | oauth::Event::Revoke { .. }
            | oauth::Event::Http(_)
            | oauth::Event::Tick { .. }
            | oauth::Event::Cancel
            | oauth::Event::Reset => unreachable!("prepared sign-in"),
        }
        exchange.stage = Stage::Running;
        exchange.purpose = crate::exchange::Purpose::SignIn { waiting: true };
        if public {
            let preferred = redirect::port(&registration.redirect_uri).expect("validated public redirect");
            exchange.listener = Some(Listener::new(
                &registration.redirect_uri,
                redirect::varying(&registration),
                exchange.return_location.take(),
            ));
            io.push(Lower::Listen {
                owner: route::owner(id, Socket::Listener),
                addr: skein_io::Addr::Ip(
                    redirect::listen_address(&registration.redirect_uri, preferred).expect("validated public redirect"),
                ),
            });
        } else {
            start(exchange, None);
        }
    }

    pub(super) fn redirected(&mut self, env: &Env<Limits>, account: u32, uri: &[u8], up: &mut Queue<Event>) {
        // A redirect completes the already admitted sign-in through a close.
        let draining_reply = match self.lifecycle {
            super::Lifecycle::Closing => match self.bindings.get(account) {
                Some(Some(id)) => self.exchanges.get(*id).expect("bound exchange").waiting(),
                Some(None) | None => false,
            },
            super::Lifecycle::Live | super::Lifecycle::Aborting | super::Lifecycle::Closed => false,
        };
        if !draining_reply && self.refused(account, Asked::Redirected, up) {
            return;
        }
        if account >= self.accounts.len() {
            up.push(Event::Refused {
                account: number(&self.names, account),
                asked: Asked::Redirected,
                why: Refusal::Account,
            });
            return;
        }
        let registered = match self.sources.get(account).expect("configured source") {
            Source::SignIn { registration, .. } if registration.client_secret.is_some() => &registration.redirect_uri,
            Source::SignIn { .. } | Source::HandedIn => {
                up.push(Event::Refused {
                    account: number(&self.names, account),
                    asked: Asked::Redirected,
                    why: Refusal::Source,
                });
                return;
            }
        };
        let id = match self.bindings.get(account).expect("account binding") {
            Some(id) => *id,
            None => {
                up.push(Event::Refused {
                    account: number(&self.names, account),
                    asked: Asked::Redirected,
                    why: Refusal::NotWaiting,
                });
                return;
            }
        };
        let exchange = self.exchanges.get_mut(id).expect("bound exchange");
        if !exchange.waiting() || exchange.visit.is_some() || !exchange.above.is_empty() {
            up.push(Event::Refused {
                account: number(&self.names, account),
                asked: Asked::Redirected,
                why: Refusal::NotWaiting,
            });
            return;
        }
        match redirect::parse(uri, registered, &env.limits.client) {
            Some(redirect) => {
                exchange.received();
                deliver(&mut exchange.client, &mut exchange.above, redirect, env.now);
            }
            None => up.push(Event::Refused {
                account: number(&self.names, account),
                asked: Asked::Redirected,
                why: Refusal::Redirect,
            }),
        }
    }
}

pub(super) fn deliver(client: &mut oauth::Client, above: &mut Queue<oauth::Request>, redirect: Redirect, now: Time) {
    client.step(
        oauth::Event::Redirected {
            client_id: redirect.client_id,
            uri: redirect.uri,
            state: redirect.state,
            code: redirect.code,
            error: redirect.error,
            now,
        },
        above,
    );
}

fn prepare_sign_in(
    registration: oauth::Registration,
    key: u32,
    env: &Env<Limits>,
    crypto: &mut CryptoRng,
) -> oauth::Event {
    let state =
        draw(crypto, env.limits.client.state_bytes.min(32).checked_mul(3).expect("bounded state").div_euclid(4));
    let verifier = if registration.client_secret.is_none() || registration.pkce_for_confidential {
        Some(draw(crypto, 32))
    } else {
        None
    };
    let openid = match &registration.terms {
        Some(terms) => terms.openid.is_some(),
        None => false,
    };
    let nonce = if openid {
        Some(draw(crypto, env.limits.client.nonce_bytes.min(32).checked_mul(3).expect("bounded nonce").div_euclid(4)))
    } else {
        None
    };
    oauth::Event::SignIn {
        registration,
        key,
        generation: 0,
        state,
        verifier,
        nonce,
        port: None,
        issued_client_id: None,
        id_token: None,
        subject: None,
        now: env.now,
    }
}

pub(super) fn start(exchange: &mut Exchange, bound_port: Option<u16>) {
    let mut event = exchange.sign_in.take().expect("prepared sign-in");
    match &mut event {
        oauth::Event::SignIn { port, .. } => *port = bound_port,
        oauth::Event::Redirected { .. }
        | oauth::Event::Refresh { .. }
        | oauth::Event::Revoke { .. }
        | oauth::Event::Http(_)
        | oauth::Event::Tick { .. }
        | oauth::Event::Cancel
        | oauth::Event::Reset => unreachable!("prepared sign-in"),
    }
    exchange.client.step(event, &mut exchange.above);
}

pub(crate) fn draw(crypto: &mut CryptoRng, length: u32) -> Box<[u8]> {
    let mut bytes = [0_u8; 32];
    let bytes = bytes.get_mut(..usize::try_from(length).expect("at most 32")).expect("bounded random bytes");
    crypto.fill(bytes);
    let alphabet = skein_lib::base64::Alphabet::Url;
    let size = skein_lib::base64::encoded_len(alphabet, length).expect("at most 43 characters");
    let mut writer = Writer::new(usize::try_from(size).expect("at most 43 characters"));
    skein_lib::base64::encode(alphabet, bytes, &mut writer).expect("measured room");
    writer.finish()
}
