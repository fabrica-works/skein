use alloc::boxed::Box;
use core::net::{Ipv4Addr, SocketAddr};
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
    machine: crate::reconcile::Reconcile,
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

impl Run {
    pub fn hook_listing() -> Self {
        use crate::reconcile::{Event, Reply};
        use skein_forge::forgejo::document::{ApiSettings, Document, Org, Row, Team};
        let mut run = Self::managed();
        run.given.listener = Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 9898)));
        run.machine = crate::reconcile::Reconcile::new(&run.given).expect("fixture admitted");
        run.bot_lookup();
        run.step(Event::Reply(Reply::Document(Document::User(person(b"kit", b"kit:bot")))));
        run.kept_tokens();
        run.step(Event::Read(Some(secret(b"42"))));
        run.step(Event::Reply(Reply::Document(Document::ApiSettings(ApiSettings {
            max_response_items: 2,
            default_paging_num: 30,
        }))));
        run.step(Event::Reply(Reply::Document(Document::Org(Org { id: 1, name: b"acme".as_slice().into() }))));
        run.step(Event::Reply(Reply::Rows(Box::new([]))));
        run.step(Event::Reply(Reply::Document(Document::Team(Team { id: 1, spec: team_spec() }))));
        run.step(Event::Reply(Reply::Rows(Box::new([Row::User(person(b"kit", b"kit:bot"))]))));
        run.step(Event::Read(Some(secret(b"webhook-secret"))));
        run
    }
}
pub fn team_spec() -> skein_forge::forgejo::document::TeamSpec {
    use skein_forge::forgejo::document::{Access, TeamSpec, Units};
    TeamSpec {
        name: b"kit".as_slice().into(),
        permission: Access::Write,
        includes_all_repositories: true,
        units: Units { code: Access::Write, issues: Access::Write, pulls: Access::Write, actions: Access::Write },
    }
}
pub fn hook_row(id: u64) -> skein_forge::forgejo::document::Row {
    skein_forge::forgejo::document::Row::Hook(skein_forge::forgejo::document::Hook {
        id,
        url: b"http://127.0.0.1:9898/hooks/1".as_slice().into(),
        events: Box::new([]),
        active: false,
    })
}

pub fn subscribed() -> Box<[Box<[u8]>]> {
    let mut events = skein_lib::List::with_capacity(22);
    for event in skein_forge::forgejo::delivery::SUBSCRIBED {
        events.push(event.into()).expect("events");
    }
    events.into_boxed()
}

pub fn limits() -> crate::limits::Limits {
    crate::limits::Limits {
        orgs: 8,
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
        orgs: Box::new([b"acme".as_slice().into()]),
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
