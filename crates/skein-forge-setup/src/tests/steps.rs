use super::fixtures::{self, Run, secret};
use crate::{
    item::Item,
    reconcile::{Event, Failure, Need, Reply, Stage, Why},
};
use skein_forge::forgejo::request::Request;

#[test]
fn an_owned_forge_makes_a_missing_org_for_its_person() {
    let mut run = fixtures::owned_org_read();
    assert_eq!(
        run.step(Event::Reply(Reply::Status(404))),
        fixtures::send(Request::CreateOrg { person: b"ada".as_slice().into(), name: b"work".as_slice().into() })
    );
}
#[test]
fn a_managed_forge_reads_no_org() {
    let mut run = fixtures::id_read();
    assert_eq!(run.step(Event::Read(Some(secret(&42u64.to_le_bytes())))), Need::Done(fixtures::done()));
    let mut out = skein_lib::Queue::with_capacity(crate::reconcile::MAX_OUT);
    crate::reconcile::step(&mut run.machine, Event::Reply(Reply::Down), &mut out);
    assert!(out.is_empty());
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
fn a_zero_page_cap_is_unexpected() {
    let mut tokens = fixtures::token_read(Item::ApiToken);
    tokens.step(Event::Read(None));
    assert_eq!(
        tokens.step(fixtures::settings_event(0)),
        fixtures::named_failure(Stage::Token, b"kit-api", Why::Unexpected)
    );
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
// Every nonterminal outer state, and all token and org phases: 20 rows.
const PHASE_COUNT: usize = 20;
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
        phase("repository", fixtures::access_run(&[9]), Box::new([]), Stage::Repository, b"9"),
        phase(
            "permission",
            fixtures::access_run(&[9]),
            Box::new([fixtures::repository(9, b"acme", b"specimen")]),
            Stage::Repository,
            b"acme/specimen",
        ),
        phase(
            "collaborator",
            fixtures::access_run(&[9]),
            Box::new([
                fixtures::repository(9, b"acme", b"specimen"),
                fixtures::permission(skein_forge::forgejo::document::Permission::None),
            ]),
            Stage::Repository,
            b"acme/specimen",
        ),
        phase("org lookup", fixtures::owned_org_read(), Box::new([]), Stage::Org, b"work"),
        phase(
            "org create",
            fixtures::owned_org_read(),
            Box::new([Event::Reply(Reply::Status(404))]),
            Stage::Org,
            b"work",
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
    let mut done = fixtures::id_read();
    assert_eq!(done.step(Event::Read(Some(secret(&42u64.to_le_bytes())))), Need::Done(fixtures::done()));
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

#[test]
fn a_halted_plan_stops_at_its_step_with_the_why_given() {
    for PhaseCase { name, mut run, prefix, stage, subject, .. } in phase_cases() {
        for event in prefix {
            run.step(event);
        }
        let mut out = skein_lib::Queue::with_capacity(crate::reconcile::MAX_OUT);
        crate::reconcile::halt(&mut run.machine, Why::Down, &mut out);
        assert_eq!(
            out.pop(),
            Some(Need::Stop(Failure { forge: run.forge(), step: stage, subject: subject.into(), why: Why::Down })),
            "{name}"
        );
    }
    for mut run in terminal_runs() {
        let mut out = skein_lib::Queue::with_capacity(crate::reconcile::MAX_OUT);
        crate::reconcile::halt(&mut run.machine, Why::Down, &mut out);
        assert!(out.is_empty());
    }
}

fn halted(mut machine: crate::host::run::Machine, stage: Stage, subject: &[u8], name: &str) {
    let mut out = skein_lib::Queue::with_capacity(crate::reconcile::MAX_OUT);
    machine.halt(Why::Down, &mut out);
    assert_eq!(
        out.pop(),
        Some(Need::Stop(Failure { forge: 1, step: stage, subject: subject.into(), why: Why::Down })),
        "{name}"
    );
    assert!(out.is_empty());
    machine.halt(Why::Down, &mut out);
    assert!(out.is_empty());
}
#[test]
fn a_halted_check_stops_at_each_phase_with_the_why_given() {
    for (name, run, _, stage, subject) in super::check::phase_cases().into_iter().skip(1) {
        halted(crate::host::run::Machine::Check(run.machine), stage, subject, name);
    }
}
#[test]
fn a_halted_take_over_stops_at_each_phase_with_the_why_given() {
    for (name, run, _, stage, subject) in super::take_over::phase_cases().into_iter().skip(1) {
        halted(crate::host::run::Machine::TakeOver(run.machine), stage, subject, name);
    }
}
#[test]
fn a_halted_adopt_stops_at_each_phase_with_the_why_given() {
    for (name, run, stage, subject) in super::adopt::phase_cases().into_iter().skip(1) {
        halted(crate::host::run::Machine::Adopt(run.machine), stage, subject, name);
    }
}
#[test]
fn a_halt_of_an_ended_plan_pushes_nothing() {
    use crate::host::run::Machine;
    let machines = super::check::terminal_runs()
        .map(|run| Machine::Check(run.machine))
        .into_iter()
        .chain(super::take_over::terminal_runs().map(|run| Machine::TakeOver(run.machine)))
        .chain(super::adopt::terminal_runs().map(|run| Machine::Adopt(run.machine)));
    for mut machine in machines {
        let mut out = skein_lib::Queue::with_capacity(crate::reconcile::MAX_OUT);
        machine.halt(Why::Down, &mut out);
        assert!(out.is_empty());
    }
}
