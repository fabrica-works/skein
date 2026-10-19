//! One forge reconciled from its answers, with one need outstanding (forge-setup.md, 4.3).
mod hooks;
mod orgs;
mod tokens;
use crate::{
    given::{self, Given, Refused},
    item::Item,
};
use alloc::boxed::Box;
use core::net::SocketAddr;
pub use hooks::hook_url;
use skein_forge::forgejo::{
    document::{Document, Row},
    request::Request,
};
use skein_lib::{Decimal, Queue, Secret, Writer, bytes};
/// Room the host reserves before stepping.
pub const MAX_OUT: u32 = 1;
/// One effect for the host, or the run's terminal outcome.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Need {
    Send { request: Request, token: Secret },
    Read(Item),
    Write { item: Item, bytes: Secret },
    Draw(u32),
    Done,
    Stop(Failure),
}
/// The outcome of the outstanding need.
#[derive(Clone, PartialEq, Eq, Debug)]
#[expect(clippy::large_enum_variant, reason = "a reply's document moves whole, as the store's records do")]
pub enum Event {
    Begin,
    Read(Option<Secret>),
    Reply(Reply),
    Written,
    Drawn(Secret),
}
/// An HTTP answer decoded by the host.
#[derive(Clone, PartialEq, Eq, Debug)]
#[expect(clippy::large_enum_variant, reason = "a document moves whole, as the store's records do")]
pub enum Reply {
    Document(Document),
    Rows(Box<[Row]>),
    Empty,
    Status(u16),
    Down,
}
/// Names the stopped step, never its credentials.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Failure {
    pub forge: u16,
    pub step: Stage,
    pub subject: Box<[u8]>,
    pub why: Why,
}
/// Setup's six steps, with the bot's id included in Bot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Admin,
    Bot,
    Token,
    Org,
    Hook,
}
/// Why the forge could not be reconciled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Why {
    Missing,
    Refused(u16),
    NotAdmin,
    BotIsAdmin,
    Unmarked,
    OrgMissing,
    Down,
    Unexpected,
}
#[derive(Debug)]
enum State {
    Begin,
    AdminFile,
    Admin,
    Bot,
    Draw,
    Create,
    Tokens,
    Id(u64),
    Written,
    Orgs,
    Hooks,
    Ended,
}
/// One forge's state; credentials remain beside the machine's requests.
#[derive(Debug)]
pub struct Reconcile {
    forge: u16,
    bot_login: Box<[u8]>,
    bot_email: Box<[u8]>,
    admin_name: Box<[u8]>,
    api_name: Box<[u8]>,
    git_name: Box<[u8]>,
    marker: Box<[u8]>,
    admin: Option<Secret>,
    state: State,
    tokens: Option<tokens::Tokens>,
    orgs: Option<orgs::Orgs>,
    hooks: Option<hooks::Hooks>,
    listener: Option<SocketAddr>,
}
impl Reconcile {
    /// Starts one entry; hooks use the listener when their step is reached.
    pub fn new(given: &Given) -> Result<Self, Refused> {
        given::check(given)?;
        Ok(Self {
            forge: given.forge,
            bot_login: given.bot_login.clone(),
            bot_email: given.bot_email.clone(),
            admin_name: given.admin_name.clone(),
            api_name: given.api_name.clone(),
            git_name: given.git_name.clone(),
            marker: given::marker(&given.word),
            admin: None,
            state: State::Begin,
            tokens: None,
            orgs: Some(orgs::Orgs::new(given)),
            hooks: None,
            listener: given.listener,
        })
    }
    fn send(&self, request: Request) -> Need {
        Need::Send { request, token: self.admin.clone().expect("admin checked before bot") }
    }
    fn stop(&mut self, stage: Stage, why: Why) -> Need {
        self.state = State::Ended;
        self.admin = None;
        let subject = match stage {
            Stage::Admin => self.admin_name.clone(),
            Stage::Bot | Stage::Token | Stage::Org | Stage::Hook => self.bot_login.clone(),
        };
        Need::Stop(Failure { forge: self.forge, step: stage, subject, why })
    }
    fn id(&mut self, user: skein_forge::forgejo::document::User) -> Need {
        if !marked(&user.full_name, &self.marker) {
            return self.stop(Stage::Bot, Why::Unmarked);
        }
        if user.is_admin {
            return self.stop(Stage::Bot, Why::BotIsAdmin);
        }
        self.tokens = Some(tokens::Tokens::new(user.id, &self.api_name, &self.git_name));
        self.state = State::Tokens;
        Need::Read(Item::ApiToken)
    }
    fn advance(&mut self, state: State, event: Event) -> Need {
        match state {
            State::Begin => match event {
                Event::Begin => {
                    self.state = State::AdminFile;
                    Need::Read(Item::AdminToken)
                }
                Event::Read(_) | Event::Reply(_) | Event::Written | Event::Drawn(_) => {
                    self.stop(Stage::Admin, Why::Unexpected)
                }
            },
            State::AdminFile => match event {
                Event::Read(Some(token)) if !token.is_empty() => {
                    self.admin = Some(token);
                    self.state = State::Admin;
                    self.send(Request::User)
                }
                Event::Read(_) => self.stop(Stage::Admin, Why::Missing),
                Event::Begin | Event::Reply(_) | Event::Written | Event::Drawn(_) => {
                    self.stop(Stage::Admin, Why::Unexpected)
                }
            },
            State::Admin => self.admin(event),
            State::Bot => self.bot(event),
            State::Draw => self.draw(event),
            State::Create => match account(event) {
                Ok(user) => self.id(user),
                Err(why) => self.stop(Stage::Bot, why),
            },
            State::Tokens => {
                self.state = State::Tokens;
                tokens::advance(self, event)
            }
            State::Id(id) => match event {
                Event::Read(value) => {
                    let digits = Decimal::of(id);
                    if match value {
                        Some(value) => value.expose() == digits.as_bytes(),
                        None => false,
                    } {
                        self.start_orgs()
                    } else {
                        self.state = State::Written;
                        Need::Write { item: Item::User, bytes: Secret::new(bytes::copy_of(digits.as_bytes())) }
                    }
                }
                Event::Begin | Event::Reply(_) | Event::Written | Event::Drawn(_) => {
                    self.stop(Stage::Bot, Why::Unexpected)
                }
            },
            State::Written => match event {
                Event::Written => self.start_orgs(),
                Event::Begin | Event::Read(_) | Event::Reply(_) | Event::Drawn(_) => {
                    self.stop(Stage::Bot, Why::Unexpected)
                }
            },
            State::Orgs => orgs::advance(self, event),
            State::Hooks => hooks::advance(self, event),
            State::Ended => unreachable!("terminal machine is not advanced"),
        }
    }
    fn admin(&mut self, event: Event) -> Need {
        match account(event) {
            Ok(user) => {
                if !user.is_admin {
                    return self.stop(Stage::Admin, Why::NotAdmin);
                }
                self.state = State::Bot;
                self.send(Request::UserOf { login: self.bot_login.clone() })
            }
            Err(why) => self.stop(Stage::Admin, why),
        }
    }
    fn bot(&mut self, event: Event) -> Need {
        match account(event) {
            Ok(user) => self.id(user),
            Err(Why::Refused(404)) => {
                self.state = State::Draw;
                Need::Draw(32)
            }
            Err(why) => self.stop(Stage::Bot, why),
        }
    }
    fn draw(&mut self, event: Event) -> Need {
        match event {
            Event::Drawn(value) if value.len() == 32 => {
                self.state = State::Create;
                self.send(Request::CreateUser {
                    login: self.bot_login.clone(),
                    email: self.bot_email.clone(),
                    full_name: bytes::copy_of(&self.marker),
                    password: hex(&value),
                    must_change_password: false,
                })
            }
            Event::Begin | Event::Read(_) | Event::Reply(_) | Event::Written | Event::Drawn(_) => {
                self.stop(Stage::Bot, Why::Unexpected)
            }
        }
    }
}
fn account(event: Event) -> Result<skein_forge::forgejo::document::User, Why> {
    match event {
        Event::Reply(reply) => match reply {
            Reply::Document(document) => match document {
                Document::User(user) => Ok(user),
                Document::Token(_)
                | Document::Org(_)
                | Document::Hook(_)
                | Document::Team(_)
                | Document::ApiSettings(_)
                | Document::Repository(_)
                | Document::Search { .. }
                | Document::Listed { .. }
                | Document::Issue(_)
                | Document::Comment(_)
                | Document::Status(_)
                | Document::Review(_)
                | Document::Pull(_)
                | Document::CombinedStatus(_)
                | Document::Branch(_)
                | Document::Protection(_)
                | Document::Permission(_)
                | Document::Compare(_)
                | Document::Runs { .. }
                | Document::Run(_)
                | Document::Job(_)
                | Document::Error(_) => Err(Why::Unexpected),
            },
            Reply::Status(status) => Err(Why::Refused(status)),
            Reply::Down => Err(Why::Down),
            Reply::Rows(_) | Reply::Empty => Err(Why::Unexpected),
        },
        Event::Begin | Event::Read(_) | Event::Written | Event::Drawn(_) => Err(Why::Unexpected),
    }
}
/// Consumes one event and emits at most one need; terminal machines ignore later events.
pub fn step(machine: &mut Reconcile, event: Event, out: &mut Queue<Need>) {
    let event = match event {
        Event::Read(Some(value)) if value.is_empty() => Event::Read(None),
        other @ (Event::Begin | Event::Read(_) | Event::Reply(_) | Event::Written | Event::Drawn(_)) => other,
    };
    let state = core::mem::replace(&mut machine.state, State::Ended);
    match state {
        State::Ended => (),
        State::Begin
        | State::AdminFile
        | State::Admin
        | State::Bot
        | State::Draw
        | State::Create
        | State::Tokens
        | State::Id(_)
        | State::Written
        | State::Orgs
        | State::Hooks => {
            let need = machine.advance(state, event);
            out.push(need);
        }
    }
}

fn hex(value: &Secret) -> Secret {
    let mut out = Writer::new(64);
    for byte in value.expose() {
        for nibble in [byte >> 4_u8, byte & 15] {
            out.put(&[*b"0123456789abcdef".get(usize::from(nibble)).expect("hex digit")])
                .expect("32 bytes as 64 hex digits");
        }
    }
    Secret::new(out.finish())
}
impl Reconcile {
    fn start_orgs(&mut self) -> Need {
        self.state = State::Orgs;
        let cap = self.tokens.as_ref().expect("tokens checked").cap();
        let orgs = self.orgs.as_mut().expect("org walk");
        orgs.begin(cap);
        let name = orgs.name().into();
        if cap.is_some() { self.send(Request::Org { name }) } else { self.send(Request::ApiSettings) }
    }
    fn start_hooks(&mut self, names: Box<[Box<[u8]>]>, cap: u32) -> Need {
        let url = match self.listener {
            Some(address) => hook_url(address, self.forge),
            None => None,
        };
        if let Some(url) = url {
            self.hooks = Some(hooks::Hooks::new(names, cap, url));
            self.state = State::Hooks;
            Need::Read(Item::WebhookSecret)
        } else {
            self.admin = None;
            self.state = State::Ended;
            Need::Done
        }
    }
    fn stop_named(&mut self, stage: Stage, name: &[u8], why: Why) -> Need {
        let Need::Stop(mut failure) = self.stop(stage, why) else { unreachable!("stop") };
        failure.subject = name.into();
        Need::Stop(failure)
    }
}

fn marked(full_name: &[u8], marker: &[u8]) -> bool {
    for (at, candidate) in full_name.windows(marker.len()).enumerate() {
        if candidate != marker {
            continue;
        }
        let before = match at.checked_sub(1) {
            Some(index) => full_name.get(index),
            None => None,
        };
        let after = match at.checked_add(marker.len()) {
            Some(index) => full_name.get(index),
            None => None,
        };
        if edge(before) && edge(after) {
            return true;
        }
    }
    false
}
fn edge(byte: Option<&u8>) -> bool {
    match byte {
        None => true,
        Some(byte) => !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-'),
    }
}
