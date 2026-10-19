use super::fixtures::{self, Run, secret};
use crate::{
    item::Item,
    reconcile::{Event, Failure, Need, Reply, Stage, Why},
};
use skein_forge::forgejo::{
    document::{Access, Row},
    request::Request,
};

#[test]
fn an_owned_forge_makes_a_missing_org_for_its_person() {
    let mut run = fixtures::owned_org_read();
    assert_eq!(
        run.step(Event::Reply(Reply::Status(404))),
        fixtures::send(Request::CreateOrg { person: b"ada".as_slice().into(), name: b"work".as_slice().into() })
    );
}
#[test]
fn a_managed_forge_missing_an_org_is_refused_naming_it() {
    let mut run = fixtures::org_read();
    assert_eq!(
        run.step(Event::Reply(Reply::Status(404))),
        fixtures::named_failure(Stage::Org, b"acme", Why::OrgMissing)
    );
}
#[test]
fn a_missing_team_is_made() {
    let mut run = fixtures::teams_read();
    assert_eq!(
        run.step(fixtures::rows(Box::new([]))),
        fixtures::send(Request::CreateTeam { org: b"acme".as_slice().into(), spec: fixtures::team_spec() })
    );
}
#[test]
fn a_changed_team_is_repaired() {
    let mut run = fixtures::teams_read();
    assert_eq!(
        run.step(fixtures::rows(Box::new([fixtures::team_row(7, b"kit", Access::Read)]))),
        fixtures::send(Request::EditTeam { team: 7, spec: fixtures::team_spec() })
    );
}
#[test]
fn an_absent_bot_member_is_added() {
    let mut run = fixtures::members_read();
    assert_eq!(
        run.step(fixtures::rows(Box::new([]))),
        fixtures::send(Request::AddMember { team: 7, login: b"kit".as_slice().into() })
    );
}
#[test]
fn an_absent_hook_is_made() {
    let mut run = fixtures::hook_listing_read();
    assert_eq!(
        run.step(fixtures::rows(Box::new([]))),
        fixtures::send(Request::CreateOrgHook { org: b"acme".as_slice().into(), hook: fixtures::hook_spec() })
    );
}
#[test]
fn a_kept_hook_is_patched() {
    let mut run = fixtures::hook_listing_read();
    assert_eq!(
        run.step(fixtures::rows(Box::new([fixtures::hook_row(3)]))),
        fixtures::send(Request::EditOrgHook { org: b"acme".as_slice().into(), id: 3, hook: fixtures::hook_spec() })
    );
}
#[test]
fn a_listener_off_loopback_skips_hooks() {
    use core::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
    for listener in [
        None,
        Some(SocketAddr::from((Ipv4Addr::new(192, 0, 2, 1), 9898))),
        Some(SocketAddr::from((Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1), 9898))),
    ] {
        let mut given = fixtures::given();
        given.listener = listener;
        let mut run = Run::with_given(given);
        fixtures::to_org(&mut run);
        run.step(fixtures::org_event());
        run.step(fixtures::rows(Box::new([])));
        run.step(fixtures::team_event());
        assert_eq!(
            run.step(fixtures::rows(Box::new([Row::User(fixtures::person(b"kit", b"kit:bot"))]))),
            Need::Done,
            "{listener:?}"
        );
    }
}
#[test]
fn tokens_are_paged_until_the_named_token() {
    let mut run = fixtures::token_read(Item::ApiToken);
    run.step(Event::Read(None));
    run.step(fixtures::settings_event(fixtures::PAGE_CAP));
    assert_eq!(
        run.step(fixtures::rows(Box::new([fixtures::token_row(5, b"other"), fixtures::token_row(6, b"another")]))),
        fixtures::send(Request::Tokens { login: b"kit".as_slice().into(), page: fixtures::page(2) })
    );
    assert_eq!(
        run.step(fixtures::rows(Box::new([fixtures::token_row(7, b"kit-api")]))),
        fixtures::send(Request::DeleteToken { login: b"kit".as_slice().into(), id: 7 })
    );
}
#[test]
fn teams_are_paged_until_the_named_team() {
    let mut run = fixtures::teams_read();
    assert_eq!(
        run.step(fixtures::rows(Box::new([
            fixtures::team_row(5, b"other", Access::Read),
            fixtures::team_row(6, b"another", Access::Read)
        ]))),
        fixtures::send(Request::Teams { org: b"acme".as_slice().into(), page: fixtures::page(2) })
    );
    assert_eq!(
        run.step(fixtures::rows(Box::new([fixtures::team_row(7, b"kit", Access::Write)]))),
        fixtures::send(Request::TeamMembers { team: 7, page: fixtures::page(1) })
    );
}
#[test]
fn members_are_paged_until_the_bot() {
    let mut run = fixtures::members_read();
    assert_eq!(
        run.step(fixtures::rows(Box::new([
            Row::User(fixtures::person(b"ada", b"Ada")),
            Row::User(fixtures::person(b"bob", b"Bob"))
        ]))),
        fixtures::send(Request::TeamMembers { team: 7, page: fixtures::page(2) })
    );
    assert_eq!(run.step(fixtures::rows(Box::new([Row::User(fixtures::person(b"kit", b"kit:bot"))]))), Need::Done);
}
#[test]
fn a_hook_on_page_two_restarts_that_page_after_delete() {
    for deleted in [Reply::Empty, Reply::Status(404)] {
        let mut run = fixtures::hook_listing_read();
        let mut other = fixtures::hook_row(9);
        if let Row::Hook(hook) = &mut other {
            hook.url = b"http://elsewhere.invalid".as_slice().into();
        }
        assert_eq!(
            run.step(fixtures::rows(Box::new([fixtures::hook_row(7), other]))),
            fixtures::send(Request::OrgHooks { org: b"acme".as_slice().into(), page: fixtures::page(2) })
        );
        assert_eq!(
            run.step(fixtures::rows(Box::new([fixtures::hook_row(3)]))),
            fixtures::send(Request::DeleteOrgHook { org: b"acme".as_slice().into(), id: 7 })
        );
        assert_eq!(
            run.step(Event::Reply(deleted)),
            fixtures::send(Request::OrgHooks { org: b"acme".as_slice().into(), page: fixtures::page(2) })
        );
        assert_eq!(
            run.step(fixtures::rows(Box::new([fixtures::hook_row(3)]))),
            fixtures::send(Request::EditOrgHook { org: b"acme".as_slice().into(), id: 3, hook: fixtures::hook_spec() })
        );
    }
}
#[test]
fn a_zero_page_cap_is_unexpected() {
    let mut tokens = fixtures::token_read(Item::ApiToken);
    tokens.step(Event::Read(None));
    let mut orgs = fixtures::id_read();
    orgs.step(Event::Read(Some(secret(b"42"))));
    for (mut run, stage, subject) in
        [(tokens, Stage::Token, b"kit-api".as_slice()), (orgs, Stage::Org, b"acme".as_slice())]
    {
        assert_eq!(
            run.step(fixtures::settings_event(0)),
            fixtures::named_failure(stage, subject, Why::Unexpected),
            "{stage:?}"
        );
    }
}
#[test]
fn a_short_secret_draw_is_unexpected() {
    let mut run = fixtures::hook_secret_read();
    run.step(Event::Read(None));
    let short = [0x0a; fixtures::SHORT_DRAW_BYTES];
    let need = run.step(Event::Drawn(secret(&short)));
    let expected = Failure { forge: 1, step: Stage::Hook, subject: b"acme".as_slice().into(), why: Why::Unexpected };
    assert_eq!(need, Need::Stop(expected), "a draw has the requested length");
}
#[test]
fn an_admin_401_names_the_given_admin() {
    let mut run = Run::managed();
    run.step(Event::Begin);
    run.step(Event::Read(Some(secret(fixtures::ADMIN))));
    assert_eq!(
        run.step(Event::Reply(Reply::Status(401))),
        fixtures::named_failure(Stage::Admin, b"kit-admin", Why::Refused(401))
    );
}
#[test]
fn a_token_create_conflict_stops_without_writing() {
    let mut run = fixtures::token_read(Item::ApiToken);
    run.step(Event::Read(None));
    run.step(fixtures::settings_event(fixtures::PAGE_CAP));
    run.step(fixtures::rows(Box::new([])));
    assert_eq!(
        run.step(Event::Reply(Reply::Status(422))),
        fixtures::named_failure(Stage::Token, b"kit-api", Why::Refused(422))
    );
}

struct PhaseCase {
    name: &'static str,
    run: Box<Run>,
    prefix: Box<[Event]>,
    wrong: Box<Event>,
    stage: Stage,
    subject: &'static [u8],
}
fn phase(name: &'static str, run: Run, prefix: Box<[Event]>, stage: Stage, subject: &'static [u8]) -> PhaseCase {
    PhaseCase { name, run: Box::new(run), prefix, wrong: Box::new(Event::Begin), stage, subject }
}
fn missing_token_listing() -> Run {
    let mut run = fixtures::token_read(Item::ApiToken);
    run.step(Event::Read(None));
    run.step(fixtures::settings_event(fixtures::PAGE_CAP));
    run
}
fn token_creation() -> Run {
    let mut run = missing_token_listing();
    run.step(fixtures::rows(Box::new([])));
    run
}
fn bot_creation() -> Run {
    let mut run = fixtures::bot_lookup_run();
    run.step(Event::Reply(Reply::Status(404)));
    run
}
fn org_settings() -> Run {
    let mut run = fixtures::id_read();
    run.step(Event::Read(Some(secret(b"42"))));
    run
}
fn hook_draw() -> Run {
    let mut run = fixtures::hook_secret_read();
    run.step(Event::Read(None));
    run
}
// Every nonterminal outer state, and all token/org/hook phases: 28 rows.
const PHASE_COUNT: usize = 28;
fn phase_cases() -> [PhaseCase; PHASE_COUNT] {
    let mut begin = phase("begin", Run::managed(), Box::new([]), Stage::Admin, b"kit-admin");
    begin.wrong = Box::new(Event::Written);
    [
        begin,
        phase("admin read", Run::managed(), Box::new([Event::Begin]), Stage::Admin, b"kit-admin"),
        phase(
            "admin check",
            Run::managed(),
            Box::new([Event::Begin, Event::Read(Some(secret(fixtures::ADMIN)))]),
            Stage::Admin,
            b"kit-admin",
        ),
        phase("bot lookup", fixtures::bot_lookup_run(), Box::new([]), Stage::Bot, b"kit"),
        phase("bot draw", bot_creation(), Box::new([]), Stage::Bot, b"kit"),
        phase("bot create", bot_creation(), Box::new([fixtures::drawn()]), Stage::Bot, b"kit"),
        phase("token read", fixtures::token_read(Item::ApiToken), Box::new([]), Stage::Token, b"kit-api"),
        phase(
            "token check",
            fixtures::token_read(Item::ApiToken),
            Box::new([Event::Read(Some(secret(b"old-token")))]),
            Stage::Token,
            b"kit-api",
        ),
        phase(
            "token settings",
            fixtures::token_read(Item::ApiToken),
            Box::new([Event::Read(None)]),
            Stage::Token,
            b"kit-api",
        ),
        phase("token listing", missing_token_listing(), Box::new([]), Stage::Token, b"kit-api"),
        phase(
            "token delete",
            missing_token_listing(),
            Box::new([fixtures::rows(Box::new([fixtures::token_row(7, b"kit-api")]))]),
            Stage::Token,
            b"kit-api",
        ),
        phase("token create", token_creation(), Box::new([]), Stage::Token, b"kit-api"),
        phase("token written", token_creation(), Box::new([fixtures::token_event()]), Stage::Token, b"kit-api"),
        phase("id read", fixtures::id_read(), Box::new([]), Stage::Bot, b"kit"),
        phase("id written", fixtures::id_read(), Box::new([Event::Read(None)]), Stage::Bot, b"kit"),
        phase("org settings", org_settings(), Box::new([]), Stage::Org, b"acme"),
        phase("org lookup", fixtures::org_read(), Box::new([]), Stage::Org, b"acme"),
        phase(
            "org create",
            fixtures::owned_org_read(),
            Box::new([Event::Reply(Reply::Status(404))]),
            Stage::Org,
            b"work",
        ),
        phase("teams listing", fixtures::teams_read(), Box::new([]), Stage::Org, b"acme"),
        phase("team written", fixtures::teams_read(), Box::new([fixtures::rows(Box::new([]))]), Stage::Org, b"acme"),
        phase("members listing", fixtures::members_read(), Box::new([]), Stage::Org, b"acme"),
        phase("member add", fixtures::members_read(), Box::new([fixtures::rows(Box::new([]))]), Stage::Org, b"acme"),
        phase("hook read", fixtures::hook_secret_read(), Box::new([]), Stage::Hook, b"acme"),
        phase("hook draw", hook_draw(), Box::new([]), Stage::Hook, b"acme"),
        phase("hook written", hook_draw(), Box::new([fixtures::drawn()]), Stage::Hook, b"acme"),
        phase("hook listing", fixtures::hook_listing_read(), Box::new([]), Stage::Hook, b"acme"),
        phase(
            "hook delete",
            fixtures::hook_listing_read(),
            Box::new([fixtures::rows(Box::new([fixtures::hook_row(7), fixtures::hook_row(3)]))]),
            Stage::Hook,
            b"acme",
        ),
        phase(
            "hook saved",
            fixtures::hook_listing_read(),
            Box::new([fixtures::rows(Box::new([]))]),
            Stage::Hook,
            b"acme",
        ),
    ]
}
#[test]
fn an_out_of_order_event_stops_each_phase() {
    for PhaseCase { name, mut run, prefix, wrong, stage, subject } in phase_cases() {
        for event in prefix {
            run.step(event);
        }
        let expected =
            Need::Stop(Failure { forge: run.forge(), step: stage, subject: subject.into(), why: Why::Unexpected });
        assert_eq!(run.step(*wrong), expected, "{name}");
    }
}
fn terminal_runs() -> [Run; 2] {
    let mut done = fixtures::members_read();
    done.step(fixtures::rows(Box::new([Row::User(fixtures::person(b"kit", b"kit:bot"))])));
    let mut stop = Run::managed();
    stop.step(Event::Begin);
    stop.step(Event::Read(None));
    [done, stop]
}
#[test]
fn an_ended_machine_ignores_every_event() {
    for mut run in terminal_runs() {
        for event in [Event::Begin, Event::Read(None), Event::Reply(Reply::Down), Event::Written, fixtures::drawn()] {
            let mut out = skein_lib::Queue::with_capacity(crate::reconcile::MAX_OUT);
            crate::reconcile::step(&mut run.machine, event.clone(), &mut out);
            assert_eq!(out.len(), 0, "late {event:?}");
        }
    }
}
