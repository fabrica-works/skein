//! Convert a manifest, prove its identity, then retain it (forge-setup.md, 5.2 and 5.3).
use super::{
    app,
    given::{self, Given, Place, Refused},
    key,
};
use crate::{
    call::Auth,
    item::{self, Item},
    reconcile::{self, Event, Need, Outcome, Reply, Stage, Why},
};
use alloc::boxed::Box;
use core::num::NonZeroU64;
use skein_forge::github::{
    document::{Conversion, Document},
    request::Request,
};
use skein_lib::{Queue, Secret, bytes};

#[derive(Debug)]
enum State {
    Begin,
    Conversion,
    App,
    Bot,
    AppId,
    AppSlug,
    AppKey,
    User,
    Ended,
}

/// A one-shot manifest conversion; state admission belongs to the host
/// (forge-setup.md, 5.2, 5.3 and 6.4). No request containing the code is retained.
#[derive(Debug)]
pub struct Convert {
    forge: u16,
    key_bytes: u32,
    name_bytes: u32,
    state: State,
    code: Option<Secret>,
    key: Option<Secret>,
    id: u64,
    slug: Box<[u8]>,
    bot: u64,
}
impl Convert {
    /// Checks the given values and a nonempty code of at most `URL_BYTES`
    /// before copying either (forge-setup.md, 5.2).
    pub fn new(given: &Given, code: &[u8]) -> Result<Self, Refused> {
        given::check(given)?;
        if code.is_empty() || code.len() > given::URL_BYTES {
            return Err(Refused { place: Place::Code, other: None });
        }
        Ok(Self {
            forge: given.forge,
            key_bytes: given.limits.key_bytes,
            name_bytes: given.limits.name_bytes,
            state: State::Begin,
            code: Some(Secret::new(bytes::copy_of(code))),
            key: None,
            id: 0,
            slug: Box::new([]),
            bot: 0,
        })
    }
    fn stop(&mut self, subject: &[u8], why: Why) -> Need {
        self.state = State::Ended;
        self.code = None;
        self.key = None;
        app::stop(self.forge, Stage::App, subject, why)
    }
    fn subject(&self) -> Box<[u8]> {
        match self.state {
            State::Begin | State::Conversion => bytes::copy_of(b"convert"),
            State::App => app::decimal(self.id),
            State::Bot | State::AppId | State::AppSlug | State::AppKey | State::User | State::Ended => {
                self.slug.clone()
            }
        }
    }
    fn converted(&mut self, conversion: Conversion) -> Need {
        let Conversion { id, slug, pem, owner, owner_id: _, webhook_secret } = conversion;
        drop(owner);
        drop(webhook_secret);
        let Some(id) = NonZeroU64::new(id) else {
            return self.stop(b"convert", Why::Unexpected);
        };
        self.id = id.get();
        let subject = app::decimal(self.id);
        if item::app_slug(&slug, self.name_bytes).is_none() {
            return self.stop(&subject, Why::Unexpected);
        }
        let key = key::pem(&pem, self.key_bytes);
        drop(pem);
        let Some(key) = key else {
            return self.stop(b"key", Why::KeyForm);
        };
        self.slug = slug;
        let auth = app::Kept::new(id, key.clone()).auth();
        self.key = Some(key);
        self.state = State::App;
        Need::Send { request: Request::App.into(), auth }
    }
    fn write(&mut self, item: Item, bytes: Secret, state: State) -> Need {
        self.state = state;
        Need::Write { item, bytes }
    }
    fn advance(&mut self, event: Event) -> Need {
        let subject = self.subject();
        match self.state {
            State::Begin => match event {
                Event::Begin => {
                    let code = self.code.take().expect("begin holds admitted code");
                    self.state = State::Conversion;
                    Need::Send {
                        request: Request::Conversion { code: bytes::copy_of(code.expose()) }.into(),
                        auth: Auth::None,
                    }
                }
                Event::Read(_) | Event::Reply(_) | Event::Written | Event::Drawn(_) => {
                    self.stop(&subject, Why::Unexpected)
                }
            },
            State::Conversion => match reconcile::reply(event) {
                Ok(Reply::GithubDocument(Document::Conversion(conversion))) => self.converted(conversion),
                Ok(
                    reply @ (Reply::Document(_)
                    | Reply::Rows(_)
                    | Reply::Empty
                    | Reply::Status(_)
                    | Reply::Down
                    | Reply::GithubDocument(_)
                    | Reply::GithubRows(_)
                    | Reply::Unsigned),
                ) => self.stop(&subject, unsigned_unexpected(reply)),
                Err(why) => self.stop(&subject, why),
            },
            State::App => match reconcile::reply(event) {
                Ok(Reply::GithubDocument(Document::App(app))) => {
                    if app.id != self.id {
                        return self.stop(&subject, Why::OtherApp);
                    }
                    if app.slug != self.slug {
                        return self.stop(&subject, Why::Unexpected);
                    }
                    self.state = State::Bot;
                    Need::Send { request: Request::BotUser { slug: self.slug.clone() }.into(), auth: Auth::None }
                }
                Ok(Reply::Unsigned) => self.stop(b"key", Why::KeyForm),
                Ok(
                    reply @ (Reply::Document(_)
                    | Reply::Rows(_)
                    | Reply::Empty
                    | Reply::Status(_)
                    | Reply::Down
                    | Reply::GithubDocument(_)
                    | Reply::GithubRows(_)),
                ) => self.stop(&subject, unsigned_unexpected(reply)),
                Err(why) => self.stop(&subject, why),
            },
            State::Bot => match app::bot(event) {
                Ok(id) => {
                    self.bot = id.get();
                    self.write(Item::AppId, Secret::new(bytes::copy_of(&self.id.to_le_bytes())), State::AppId)
                }
                Err(why) => self.stop(&subject, why),
            },
            State::AppId | State::AppSlug | State::AppKey | State::User => match event {
                Event::Written => self.written(),
                Event::Begin | Event::Read(_) | Event::Reply(_) | Event::Drawn(_) => {
                    self.stop(&subject, Why::Unexpected)
                }
            },
            State::Ended => unreachable!("ended conversion is not advanced"),
        }
    }
    fn written(&mut self) -> Need {
        match self.state {
            State::AppId => self.write(Item::AppSlug, Secret::new(self.slug.clone()), State::AppSlug),
            State::AppSlug => {
                let key = self.key.take().expect("proved key retained until its write");
                self.write(Item::AppKey, key, State::AppKey)
            }
            State::AppKey => self.write(Item::User, Secret::new(bytes::copy_of(&self.bot.to_le_bytes())), State::User),
            State::User => {
                self.state = State::Ended;
                Need::Done(Outcome::Nothing)
            }
            State::Begin | State::Conversion | State::App | State::Bot | State::Ended => {
                unreachable!("only writes await Written")
            }
        }
    }
}
fn unsigned_unexpected(reply: Reply) -> Why {
    match reply {
        Reply::Unsigned => Why::Unexpected,
        other @ (Reply::Status(_)
        | Reply::Down
        | Reply::Document(_)
        | Reply::Rows(_)
        | Reply::Empty
        | Reply::GithubDocument(_)
        | Reply::GithubRows(_)) => reconcile::unexpected(other),
    }
}
/// Consumes one event, emitting at most `reconcile::MAX_OUT` needs; ended
/// conversions ignore further events (forge-setup.md, 5.2 and 6.4).
pub fn step(machine: &mut Convert, event: Event, out: &mut Queue<Need>) {
    match machine.state {
        State::Ended => (),
        State::Begin
        | State::Conversion
        | State::App
        | State::Bot
        | State::AppId
        | State::AppSlug
        | State::AppKey
        | State::User => {
            let need = machine.advance(event);
            out.push(need);
        }
    }
}
/// Stops a live conversion at its current step, dropping private material.
#[cfg_attr(not(test), expect(dead_code, reason = "the conversion host arm calls halt when it is added"))]
pub(crate) fn halt(machine: &mut Convert, why: Why, out: &mut Queue<Need>) {
    match machine.state {
        State::Ended => (),
        State::Begin
        | State::Conversion
        | State::App
        | State::Bot
        | State::AppId
        | State::AppSlug
        | State::AppKey
        | State::User => {
            let subject = machine.subject();
            let need = machine.stop(&subject, why);
            out.push(need);
        }
    }
}
