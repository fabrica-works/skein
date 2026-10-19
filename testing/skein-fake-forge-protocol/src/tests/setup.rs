use super::fixtures::{self, boxed};
use fixtures::{HOOK, ORGANIZATION, PAGE, PERSON, SETUP_ROWS, TEAM};
use skein_forge::forgejo::{
    document::{self as d, Document, Row},
    request::Request,
};
use skein_lib::Secret;

#[test]
fn setup_routes_keep_orgs_teams_members_and_hooks() {
    let mut h = fixtures::setup_hookable();
    let create = Request::CreateOrg { person: boxed(b"kit"), name: boxed(b"work") };
    h.send(&create);
    assert_eq!(h.decoded(&create).document, Document::Org(d::Org { id: ORGANIZATION, name: boxed(b"work") }));
    make_admin_team(&mut h);
    let teams = Request::Teams { org: boxed(b"work"), page: PAGE };
    h.send(&teams);
    assert_eq!(
        &*h.decoded(&teams).rows,
        &[Row::Team(d::Team { id: TEAM, spec: d::TeamSpec { permission: d::Access::Admin, ..fixtures::team_spec() } })]
    );
    h.send(&Request::AddMember { team: TEAM, login: boxed(b"kit") });
    let members = Request::TeamMembers { team: TEAM, page: PAGE };
    let create_hook = Request::CreateOrgHook { org: boxed(b"work"), hook: fixtures::hook_spec() };
    h.send(&create_hook);
    assert_eq!(h.decoded(&create_hook).document, Document::Hook(expected_hook(true)));
    h.push(b"work");
    h.run();
    assert_eq!(h.deliveries.len(), fixtures::NO_DELIVERIES);
    h.send(&Request::EditOrgHook {
        org: boxed(b"work"),
        id: HOOK,
        hook: skein_forge::forgejo::request::HookSpec { active: false, ..fixtures::hook_spec() },
    });
    let hooks = Request::OrgHooks { org: boxed(b"work"), page: PAGE };
    h.send(&hooks);
    assert_eq!(&*h.decoded(&hooks).rows, &[Row::Hook(expected_hook(false))]);
    h.send(&Request::DeleteOrgHook { org: boxed(b"work"), id: HOOK });
    h.send(&hooks);
    assert_eq!(&*h.decoded(&hooks).rows, &[]);
    h.send(&members);
    assert_eq!(&*h.decoded(&members).rows, &[Row::User(d::User { full_name: boxed(b""), ..setup_person() })]);
}
fn make_admin_team(h: &mut fixtures::Harness) {
    for request in [
        Request::CreateTeam { org: boxed(b"work"), spec: fixtures::team_spec() },
        Request::EditTeam { team: TEAM, spec: d::TeamSpec { permission: d::Access::Admin, ..fixtures::team_spec() } },
    ] {
        h.send(&request);
    }
}
fn setup_person() -> d::User {
    d::User { id: PERSON, login: boxed(b"kit"), is_admin: false, full_name: boxed(b"kit:bot") }
}
fn expected_hook(active: bool) -> d::Hook {
    d::Hook { id: HOOK, url: boxed(b"http://127.0.0.1:7200/hooks/1"), events: Box::new([boxed(b"push")]), active }
}
#[test]
fn user_scopes_admit_the_three_documented_spellings() {
    for scope in [b"read:user".as_slice(), b"write:user", b"all"] {
        let mut h = scoped_user(&[scope]);
        h.ask_as(b"GET", b"/api/v1/user", b"", USER_TOKEN);
        assert_eq!(h.status(), 200, "{scope:?}");
        assert_eq!(h.decoded(&Request::User).document, Document::User(setup_person()), "{scope:?}");
    }
}
const USER_TOKEN: &[u8] = b"scoped-user-token";
fn scoped_user(scopes: &[&[u8]]) -> fixtures::Harness {
    let mut h = fixtures::setup_harness();
    h.service.setup_mut().token(PERSON, b"scoped", scopes, Secret::new(boxed(USER_TOKEN)));
    h
}
#[test]
fn a_token_without_user_scope_is_refused() {
    let mut h = scoped_user(&[b"read:repository"]);
    h.ask_as(b"GET", b"/api/v1/user", b"", USER_TOKEN);
    assert_eq!(h.status(), 403);
}
#[test]
fn missing_setup_objects_answer_not_found() {
    for (method, target) in [
        (b"GET".as_slice(), b"/api/v1/users/missing".as_slice()),
        (b"GET", b"/api/v1/orgs/missing"),
        (b"GET", b"/api/v1/teams/99/members"),
        (b"DELETE", b"/api/v1/orgs/work/hooks/99"),
    ] {
        let mut h = fixtures::setup_harness();
        h.service.setup_mut().org(b"work", b"kit");
        h.ask(method, target, b"");
        assert_eq!(h.status(), 404, "{target:?}");
    }
}
#[test]
fn a_full_setup_table_refuses_one_more_row() {
    for table in full_tables() {
        let mut h = fixtures::setup_harness();
        h.service.setup_mut().org(b"work", b"kit");
        (table.fill)(h.service.setup_mut());
        assert_eq!(
            (table.count)(h.service.setup()),
            usize::try_from(SETUP_ROWS).expect("table fits"),
            "{} at bound",
            table.name
        );
        h.send(&table.request);
        assert_eq!(h.status(), 503, "{} past bound", table.name);
        assert_eq!(&*h.body(), br#"{"message":"setup store is full"}"#, "{}", table.name);
        assert_eq!(
            (table.count)(h.service.setup()),
            usize::try_from(SETUP_ROWS).expect("table fits"),
            "{} unchanged",
            table.name
        );
    }
}
// setup_harness seeds kit-admin and kit, one admin token; this table seeds work.
const SETUP_ACCOUNTS: u64 = 2;
const SETUP_TOKENS: u64 = 1;
const SETUP_ORGS: u64 = 1;
struct FullTable {
    name: &'static str,
    fill: fn(&mut crate::setup::Store),
    count: fn(&crate::setup::Store) -> usize,
    request: Request,
}
fn numbered_rows(store: &crate::setup::Store, start: u64, present: fn(&crate::setup::Store, &[u8]) -> bool) -> usize {
    let mut count = usize::from(present(store, b"overflow"));
    for number in start..u64::from(SETUP_ROWS) {
        count = count
            .checked_add(usize::from(present(store, skein_lib::Decimal::of(number).as_bytes())))
            .expect("bounded count");
    }
    count
}
fn full_tables() -> [FullTable; 5] {
    [
        FullTable {
            name: "accounts",
            fill: |store| {
                for n in SETUP_ACCOUNTS..u64::from(SETUP_ROWS) {
                    store.account(skein_lib::Decimal::of(n).as_bytes(), false, b"");
                }
            },
            count: |store| {
                let seeded = usize::from(store.find(b"kit-admin").is_some())
                    .checked_add(usize::from(store.find(b"kit").is_some()))
                    .expect("seeded accounts");
                seeded
                    .checked_add(numbered_rows(store, SETUP_ACCOUNTS, |store, name| store.find(name).is_some()))
                    .expect("account count")
            },
            request: Request::CreateUser {
                login: boxed(b"overflow"),
                email: boxed(b"kit@example.test"),
                full_name: boxed(b""),
                password: Secret::new(boxed(b"password")),
                must_change_password: false,
            },
        },
        FullTable {
            name: "tokens",
            fill: |store| {
                for n in SETUP_TOKENS..u64::from(SETUP_ROWS) {
                    store.token(PERSON, skein_lib::Decimal::of(n).as_bytes(), &[b"all"], Secret::new(boxed(b"filler")));
                }
            },
            count: |store| store.issued().len(),
            request: Request::CreateToken {
                login: boxed(b"kit"),
                name: boxed(b"overflow"),
                scopes: Box::new([boxed(b"all")]),
            },
        },
        FullTable {
            name: "organizations",
            fill: |store| {
                for n in SETUP_ORGS..u64::from(SETUP_ROWS) {
                    store.org(skein_lib::Decimal::of(n).as_bytes(), b"kit");
                }
            },
            count: |store| {
                usize::from(store.organization(b"work").is_some())
                    .checked_add(numbered_rows(store, SETUP_ORGS, |store, name| store.organization(name).is_some()))
                    .expect("org count")
            },
            request: Request::CreateOrg { person: boxed(b"kit"), name: boxed(b"overflow") },
        },
        FullTable {
            name: "teams",
            fill: |store| {
                for n in 0..SETUP_ROWS {
                    store.add_team(
                        b"work",
                        d::TeamSpec {
                            name: boxed(skein_lib::Decimal::of(u64::from(n)).as_bytes()),
                            ..fixtures::team_spec()
                        },
                    );
                }
            },
            count: |store| numbered_rows(store, 0, |store, name| store.team(b"work", name).is_some()),
            request: Request::CreateTeam {
                org: boxed(b"work"),
                spec: d::TeamSpec { name: boxed(b"overflow"), ..fixtures::team_spec() },
            },
        },
        FullTable {
            name: "hooks",
            fill: |store| {
                for _ in 0..SETUP_ROWS {
                    store.hook(b"work", b"http://127.0.0.1:7200/hooks/1", Secret::new(boxed(fixtures::FIXTURE_SECRET)));
                }
            },
            count: |store| store.subscriptions(b"work").count(),
            request: Request::CreateOrgHook { org: boxed(b"work"), hook: fixtures::hook_spec() },
        },
    ]
}
#[test]
fn a_plain_user_is_refused_the_admin_routes() {
    let mut h = fixtures::setup_harness();
    h.service.setup_mut().token(2, b"plain", &[b"read:user"], Secret::new(boxed(b"plain-user-token")));
    h.ask_as(b"GET", b"/api/v1/admin/users/kit/tokens", b"", b"plain-user-token");
    assert_eq!(h.status(), 403);
    assert_eq!(h.body().as_ref(), b"{\"message\":\"user should be the site admin\"}");
}
#[test]
fn a_made_token_authenticates_as_its_user_and_a_deleted_one_does_not() {
    let mut h = fixtures::setup_harness();
    let made = fixtures::make_setup_token(&mut h);
    let secret = made.value.expect("new token value");
    h.ask_as(b"GET", b"/api/v1/user", b"", secret.expose());
    assert_eq!(h.status(), 200);
    assert_eq!(h.body().as_ref(), br#"{"id":2,"login":"kit","is_admin":false,"full_name":"kit:bot"}"#);
    h.ask(b"DELETE", b"/api/v1/admin/users/kit/tokens/2", b"");
    assert_eq!(h.status(), 204);
    h.ask_as(b"GET", b"/api/v1/user", b"", secret.expose());
    assert_eq!(h.status(), 401);
}
#[test]
fn a_second_token_of_one_name_is_refused() {
    let mut h = fixtures::setup_harness();
    fixtures::make_setup_token(&mut h);
    h.ask(b"POST", b"/api/v1/admin/users/kit/tokens", br#"{"name":"kit-api","scopes":["read:user"]}"#);
    assert_eq!(h.status(), 400);
    assert_eq!(h.body().as_ref(), b"{\"message\":\"access token name has been used already\"}");
}
