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

fn user_repos_harness(all_repositories: bool) -> fixtures::Harness {
    use crate::{Config, Faults, Repository};
    use skein_fake_forge_domain::{self as fake, api as a};
    use skein_lib::{Duration, List};
    let mut users = List::with_capacity(2);
    users.push((1, boxed(b"alice"))).unwrap();
    users.push((2, boxed(b"bob"))).unwrap();
    let mut tokens = List::with_capacity(2);
    tokens.push((Secret::new(boxed(b"example-app")), 1)).unwrap();
    tokens.push((Secret::new(boxed(b"bob")), 2)).unwrap();
    let mut repositories = List::with_capacity(4);
    // Deliberately not id order: paging must follow ids rather than configuration order.
    for (id, full) in
        [(30, b"alice/widgets".as_slice()), (10, b"bob/widgets"), (20, b"acme/widgets"), (40, b"bob/example-app")]
    {
        repositories.push(Repository { id, full: boxed(full), protected: None }).unwrap();
    }
    let config = Config {
        hooks: None,
        base: boxed(b"http://example.invalid"),
        tokens,
        users,
        repositories,
        max_response_items: 4,
        default_paging_num: 4,
        faults: Faults { reset: 0, drop: 0, server_error: 0 },
    };
    let mut h = fixtures::Harness::new(config);
    h.fake_env.limits.limits.repositories = 4;
    h.fake_env.limits.limits.commits = 4;
    h.domain = fake::Domain::new(&h.fake_env.limits, 3);
    for (id, full) in
        [(30, b"alice/widgets".as_slice()), (10, b"bob/widgets"), (20, b"acme/widgets"), (40, b"bob/example-app")]
    {
        fake::repository(
            &mut h.domain,
            &h.fake_env.limits,
            a::Setup {
                id,
                name: boxed(full),
                default: boxed(b"main"),
                tree: Box::from([]),
                labels: Box::from([]),
                checks: a::Checks {
                    contexts: Box::from([]),
                    latency_min: Duration::ZERO,
                    latency_max: Duration::ZERO,
                    silent: 0,
                    passes: 0,
                    reruns: 0,
                    cue: None,
                },
                protection: None,
                hooked: false,
            },
        );
    }
    fake::grant(&mut h.domain, b"bob/widgets", 1, a::Permission::Write);
    let store = h.service.setup_mut();
    assert_eq!(store.account(b"alice", false, b""), 1);
    assert_eq!(store.account(b"bob", false, b""), 2);
    store.org(b"acme", b"bob");
    let team = store.add_team(
        b"acme",
        d::TeamSpec {
            name: boxed(b"example-app"),
            permission: d::Access::Write,
            includes_all_repositories: all_repositories,
            units: d::Units {
                code: d::Access::None,
                issues: d::Access::None,
                pulls: d::Access::None,
                actions: d::Access::None,
            },
        },
    );
    store.member(team, 1);
    h
}
fn user_repos_request(page: u32, limit: u32) -> Request {
    Request::UserRepos { page: skein_forge::forgejo::request::Page { page: Some(page), limit: Some(limit) } }
}
fn ask_user_repos(h: &mut fixtures::Harness, page: u32, limit: u32) -> Box<[Row]> {
    let request = user_repos_request(page, limit);
    let encoded = skein_forge::forgejo::client::encode(&request, &h.env.limits.documents).unwrap();
    h.ask_as(b"GET", &encoded.target, b"", b"example-app");
    assert_eq!(h.status(), 200);
    h.decoded(&request).rows
}
fn reachable_row(id: u64, owner: u64, login: &[u8], private: bool, push: bool) -> Row {
    Row::Reachable(d::Reachable { repository: id, owner, login: boxed(login), name: boxed(b"widgets"), private, push })
}
#[test]
fn fake_user_repos_lists_owned_collaborating_and_team_repositories() {
    let mut h = user_repos_harness(true);
    assert_eq!(
        &*ask_user_repos(&mut h, 1, 4),
        &[
            reachable_row(10, 2, b"bob", false, true),
            reachable_row(20, 1, b"acme", false, true),
            reachable_row(30, 1, b"alice", false, true),
        ]
    );
    assert_eq!(h.service.tally().routes.get(&crate::Route::UserRepos), Some(&1));
    assert_eq!(h.domain.permission(b"acme/widgets", 1).unwrap(), skein_fake_forge_domain::api::Permission::None);
}
#[test]
fn fake_user_repos_team_without_all_repositories_reaches_nothing() {
    let mut h = user_repos_harness(false);
    assert_eq!(
        &*ask_user_repos(&mut h, 1, 4),
        &[reachable_row(10, 2, b"bob", false, true), reachable_row(30, 1, b"alice", false, true),]
    );
}
#[test]
fn fake_user_repos_includes_private_repositories() {
    let mut h = user_repos_harness(true);
    h.service.setup_mut().private(20).unwrap();
    h.service.setup_mut().private(20).unwrap();
    assert_eq!(
        &*ask_user_repos(&mut h, 1, 4),
        &[
            reachable_row(10, 2, b"bob", false, true),
            reachable_row(20, 1, b"acme", true, true),
            reachable_row(30, 1, b"alice", false, true),
        ]
    );
}
#[test]
fn fake_user_repos_pages_by_page_and_limit_in_id_order() {
    let mut h = user_repos_harness(true);
    for (page, expected) in [
        (1, Some(reachable_row(10, 2, b"bob", false, true))),
        (2, Some(reachable_row(20, 1, b"acme", false, true))),
        (3, Some(reachable_row(30, 1, b"alice", false, true))),
        (4, None),
        (u32::MAX, None),
    ] {
        let rows = ask_user_repos(&mut h, page, 1);
        match expected {
            Some(row) => assert_eq!(&*rows, &[row]),
            None => assert!(rows.is_empty()),
        }
    }
    assert_eq!(ask_user_repos(&mut h, 1, 99).len(), 3);
    h.ask_as(b"GET", b"/api/v1/user/repos?limit=1", b"", b"example-app");
    assert_eq!(h.decoded(&user_repos_request(1, 4)).rows.len(), 3);
}
#[test]
fn fake_user_repos_rows_carry_permissions_push_pull_admin() {
    use skein_fake_forge_domain::{self as fake, api::Permission};
    for (permission, expected) in [
        (Permission::Read, br#"[{"id":10,"name":"widgets","private":false,"owner":{"id":2,"login":"bob"},"permissions":{"push":false,"pull":true,"admin":false}}]"#.as_slice()),
        (Permission::Write, br#"[{"id":10,"name":"widgets","private":false,"owner":{"id":2,"login":"bob"},"permissions":{"push":true,"pull":true,"admin":false}}]"#),
        (Permission::Admin, br#"[{"id":10,"name":"widgets","private":false,"owner":{"id":2,"login":"bob"},"permissions":{"push":true,"pull":true,"admin":true}}]"#),
    ] {
        let mut h = user_repos_harness(true);
        fake::grant(&mut h.domain, b"bob/widgets", 1, permission);
        ask_user_repos(&mut h, 1, 1);
        assert_eq!(&*h.body(), expected);
    }
}
#[test]
fn fake_user_repos_read_only_token_sees_push_false() {
    let mut h = user_repos_harness(false);
    skein_fake_forge_domain::grant(&mut h.domain, b"bob/widgets", 1, skein_fake_forge_domain::api::Permission::Read);
    assert_eq!(&*ask_user_repos(&mut h, 1, 1), &[reachable_row(10, 2, b"bob", false, false)]);
}
#[test]
fn fake_user_repos_without_token_is_401() {
    let mut h = user_repos_harness(true);
    for token in [b"".as_slice(), b"unknown"] {
        h.ask_as(b"GET", b"/api/v1/user/repos", b"", token);
        assert_eq!(h.status(), 401);
    }
    assert_eq!(h.calls, 0);
}

#[test]
fn fake_user_repos_combines_team_grants_with_direct_permissions() {
    use skein_fake_forge_domain::{self as fake, api::Permission};
    for (access, direct, effective) in [
        (d::Access::None, Permission::None, None),
        (d::Access::Read, Permission::None, Some(false)),
        (d::Access::Write, Permission::Read, Some(true)),
        (d::Access::Admin, Permission::None, Some(true)),
        (d::Access::Owner, Permission::None, Some(true)),
        (d::Access::Read, Permission::Admin, Some(true)),
    ] {
        let mut h = user_repos_harness(false);
        fake::grant(&mut h.domain, b"acme/widgets", 1, direct);
        let store = h.service.setup_mut();
        let team = store.add_team(
            b"acme",
            d::TeamSpec {
                name: boxed(b"widgets"),
                permission: access,
                includes_all_repositories: true,
                units: d::Units {
                    code: d::Access::None,
                    issues: d::Access::None,
                    pulls: d::Access::None,
                    actions: d::Access::None,
                },
            },
        );
        store.member(team, 1);
        let rows = ask_user_repos(&mut h, 1, 4);
        let mut found = None;
        for row in &rows {
            if let Row::Reachable(repo) = row
                && repo.repository == 20
            {
                found = Some(row.clone());
            }
        }
        match effective {
            Some(push) => assert_eq!(found, Some(reachable_row(20, 1, b"acme", false, push))),
            None => assert_eq!(found, None),
        }
        if access == d::Access::Admin || access == d::Access::Owner || direct == Permission::Admin {
            assert!(skein_lib::bytes::find(&h.body(), b"\"admin\":true").is_some());
        }
    }
}

#[test]
fn fake_user_repos_organization_owner_has_admin_permission() {
    let mut h = user_repos_harness(false);
    h.ask_as(b"GET", b"/api/v1/user/repos?page=2&limit=1", b"", b"bob");
    assert_eq!(h.status(), 200);
    assert_eq!(&*h.body(), br#"[{"id":20,"name":"widgets","private":false,"owner":{"id":1,"login":"acme"},"permissions":{"push":true,"pull":true,"admin":true}}]"#);
}

#[test]
fn fake_user_repos_privacy_metadata_is_bounded() {
    let mut h = user_repos_harness(false);
    let cap = h.env.limits.documents.inner;
    for repository in 0..cap {
        assert_eq!(h.service.setup_mut().private(u64::from(repository)), Ok(()));
    }
    assert_eq!(h.service.setup_mut().private(u64::from(cap)), Err(crate::Error::Limits));
    assert_eq!(h.service.setup_mut().private(0), Ok(()));
}
