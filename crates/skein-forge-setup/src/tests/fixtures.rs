use alloc::boxed::Box;
use skein_lib::{Duration, bytes};
pub const ADMIN: &[u8] = b"site-admin-token";
pub fn secret(value: &[u8]) -> skein_lib::Secret {
    skein_lib::Secret::new(bytes::copy_of(value))
}
pub fn admin() -> skein_forge::forgejo::document::User {
    skein_forge::forgejo::document::User {
        id: 1,
        login: b"kit-admin".as_slice().into(),
        is_admin: true,
        full_name: Box::new([]),
    }
}
pub fn person(login: &[u8], full_name: &[u8]) -> skein_forge::forgejo::document::User {
    skein_forge::forgejo::document::User {
        id: 42,
        login: bytes::copy_of(login),
        is_admin: false,
        full_name: bytes::copy_of(full_name),
    }
}
pub struct Run {
    pub(super) machine: crate::reconcile::Reconcile,
    given: crate::given::Given,
}
impl Run {
    pub fn managed() -> Self {
        Self::new(1)
    }
    pub fn owned() -> Self {
        Self::new(2)
    }
    fn new(forge: u16) -> Self {
        let mut given = given();
        given.forge = forge;
        if forge == 2 {
            given.orgs = Box::new([b"work".as_slice().into()]);
            given.org_owner = Some(b"ada".as_slice().into());
        }
        Self::with_given(given)
    }
    pub fn with_given(given: crate::given::Given) -> Self {
        Self { machine: crate::reconcile::Reconcile::new(&given).expect("fixture admitted"), given }
    }
    pub fn step(&mut self, event: crate::reconcile::Event) -> crate::reconcile::Need {
        let mut out = skein_lib::Queue::with_capacity(crate::reconcile::MAX_OUT);
        crate::reconcile::step(&mut self.machine, event, &mut out);
        assert_eq!(out.len(), 1);
        out.pop().expect("one need")
    }
    pub fn forge(&self) -> u16 {
        self.given.forge
    }
    pub fn bot(&self) -> Box<[u8]> {
        self.given.bot_login.clone()
    }
    pub fn bot_lookup(&mut self) {
        use crate::{
            item::Item,
            reconcile::{Event, Need, Reply},
        };
        use skein_forge::forgejo::{document::Document, request::Request};
        assert_eq!(self.step(Event::Begin), Need::Read(Item::AdminToken));
        assert_eq!(
            self.step(Event::Read(Some(secret(ADMIN)))),
            Need::Send { request: Request::User, token: secret(ADMIN) }
        );
        assert_eq!(
            self.step(Event::Reply(Reply::Document(Document::User(admin())))),
            Need::Send { request: Request::UserOf { login: self.bot() }, token: secret(ADMIN) }
        );
    }
}
impl Run {
    pub fn kept_tokens(&mut self) {
        use crate::{
            item::Item,
            reconcile::{Event, Need, Reply},
        };
        use skein_forge::forgejo::{document::Document, request::Request};
        for next in [Item::GitToken, Item::User] {
            assert_eq!(
                self.step(Event::Read(Some(secret(b"bot-token")))),
                Need::Send { request: Request::User, token: secret(b"bot-token") }
            );
            assert_eq!(
                self.step(Event::Reply(Reply::Document(Document::User(person(b"kit", b"kit:bot"))))),
                Need::Read(next)
            );
        }
    }
}

pub fn limits() -> crate::limits::Limits {
    crate::limits::Limits {
        orgs: PROFILE_ORGS,
        deadline: Duration::from_secs(120),
        backoff: Duration::from_millis(100),
        backoff_max: Duration::from_secs(5),
    }
}
pub fn given() -> crate::given::Given {
    crate::given::Given {
        forge: 1,
        bot_login: b"kit".as_slice().into(),
        bot_email: b"kit@example.invalid".as_slice().into(),
        admin_name: b"kit-admin".as_slice().into(),
        api_name: b"kit-api".as_slice().into(),
        git_name: b"kit-git".as_slice().into(),
        orgs: Box::new([]),
        org_owner: None,
        listener: None,
        word: b"kit".as_slice().into(),
        limits: limits(),
    }
}

/// Explicit answer prefix leaving the bot lookup outstanding.
pub fn to_bot(run: &mut Run) {
    use crate::reconcile::{Event, Reply};
    use skein_forge::forgejo::document::Document;
    run.step(Event::Begin);
    run.step(Event::Read(Some(secret(ADMIN))));
    run.step(Event::Reply(Reply::Document(Document::User(admin()))));
}
// Shared profile and small page sizes from forge-setup.md, 4.3 and 4.4.
pub const PROFILE_ORGS: u32 = 8;
pub const PAGE_CAP: u32 = 2;
pub fn bot_event() -> crate::reconcile::Event {
    user_event(person(b"kit", b"kit:bot"))
}
pub fn user_event(user: skein_forge::forgejo::document::User) -> crate::reconcile::Event {
    crate::reconcile::Event::Reply(crate::reconcile::Reply::Document(skein_forge::forgejo::document::Document::User(
        user,
    )))
}
pub fn settings_event(cap: u32) -> crate::reconcile::Event {
    use skein_forge::forgejo::document::{ApiSettings, Document};
    crate::reconcile::Event::Reply(crate::reconcile::Reply::Document(Document::ApiSettings(ApiSettings {
        max_response_items: cap,
        default_paging_num: 30,
    })))
}
pub fn keep_token(run: &mut Run) {
    run.step(crate::reconcile::Event::Read(Some(secret(b"bot-token"))));
    run.step(bot_event());
}
pub fn token_read(item: crate::item::Item) -> Run {
    use crate::item::Item;
    let mut run = Run::managed();
    to_bot(&mut run);
    run.step(bot_event());
    match item {
        Item::ApiToken => (),
        Item::GitToken => keep_token(&mut run),
        Item::AdminToken | Item::User | Item::WebhookSecret => panic!("token fixture"),
    }
    run
}
pub fn id_read() -> Run {
    let mut run = token_read(crate::item::Item::ApiToken);
    keep_token(&mut run);
    keep_token(&mut run);
    run
}
pub fn to_org(run: &mut Run) {
    use crate::reconcile::Event;
    to_bot(run);
    run.step(bot_event());
    keep_token(run);
    keep_token(run);
    run.step(Event::Read(Some(secret(&42u64.to_le_bytes()))));
}
pub fn rows(rows: Box<[skein_forge::forgejo::document::Row]>) -> crate::reconcile::Event {
    crate::reconcile::Event::Reply(crate::reconcile::Reply::Rows(rows))
}
pub fn send(request: skein_forge::forgejo::request::Request) -> crate::reconcile::Need {
    crate::reconcile::Need::Send { request, token: secret(ADMIN) }
}
pub fn token_name(item: crate::item::Item) -> &'static [u8] {
    use crate::item::Item;
    match item {
        Item::ApiToken => b"kit-api",
        Item::GitToken => b"kit-git",
        Item::AdminToken | Item::User | Item::WebhookSecret => panic!("token name"),
    }
}
pub fn create_token(item: crate::item::Item) -> skein_forge::forgejo::request::Request {
    use crate::item::Item;
    let scopes: Box<[Box<[u8]>]> = match item {
        Item::ApiToken => Box::new([
            b"read:user".as_slice().into(),
            b"write:issue".as_slice().into(),
            b"write:repository".as_slice().into(),
        ]),
        Item::GitToken => Box::new([b"write:repository".as_slice().into(), b"read:user".as_slice().into()]),
        Item::AdminToken | Item::User | Item::WebhookSecret => panic!("token scopes"),
    };
    skein_forge::forgejo::request::Request::CreateToken {
        login: b"kit".as_slice().into(),
        name: token_name(item).into(),
        scopes,
    }
}
pub fn token_event() -> crate::reconcile::Event {
    use skein_forge::forgejo::document::{AccessToken, Document};
    crate::reconcile::Event::Reply(crate::reconcile::Reply::Document(Document::Token(AccessToken {
        id: 7,
        name: b"unused".as_slice().into(),
        scopes: Box::new([]),
        last_eight: Box::new([]),
        value: Some(secret(b"new-token")),
    })))
}
pub fn named_failure(
    step: crate::reconcile::Stage,
    subject: &[u8],
    why: crate::reconcile::Why,
) -> crate::reconcile::Need {
    crate::reconcile::Need::Stop(crate::reconcile::Failure { forge: 1, step, subject: subject.into(), why })
}

pub fn token_row(id: u64, name: &[u8]) -> skein_forge::forgejo::document::Row {
    use skein_forge::forgejo::document::{AccessToken, Row};
    Row::Token(AccessToken { id, name: name.into(), scopes: Box::new([]), last_eight: Box::new([]), value: None })
}
// The bot step requests 32 draw bytes (4.3).
pub const DRAW_BYTES: usize = 32;
pub fn drawn() -> crate::reconcile::Event {
    crate::reconcile::Event::Drawn(secret(&[0x0a; DRAW_BYTES]))
}
pub fn bot_lookup_run() -> Run {
    let mut run = Run::managed();
    to_bot(&mut run);
    run
}
pub fn owned_org_read() -> Run {
    let mut run = Run::owned();
    to_org(&mut run);
    run
}
pub fn page(number: u32) -> skein_forge::forgejo::request::Page {
    skein_forge::forgejo::request::Page { page: Some(number), limit: Some(PAGE_CAP) }
}

pub fn done() -> crate::reconcile::Outcome {
    crate::reconcile::Outcome::Nothing
}

pub struct CheckRun {
    pub(super) machine: crate::check::Check,
}
impl CheckRun {
    pub fn new() -> Self {
        Self { machine: crate::check::Check::new(1) }
    }
    pub fn step(&mut self, event: crate::reconcile::Event) -> crate::reconcile::Need {
        let mut out = skein_lib::Queue::with_capacity(crate::check::MAX_OUT);
        crate::check::step(&mut self.machine, event, &mut out);
        assert_eq!(out.len(), 1);
        out.pop().expect("one need")
    }
}

pub fn user(id: u64, login: &[u8], is_admin: bool) -> skein_forge::forgejo::document::User {
    skein_forge::forgejo::document::User { id, login: bytes::copy_of(login), is_admin, full_name: Box::new([]) }
}
