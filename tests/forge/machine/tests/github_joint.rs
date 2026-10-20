use skein_fake_forge_github::keys::KEYS;
use skein_forge::{
    api::{self, Answer, Op, Read},
    credential::Credential,
};
use skein_forge_connection::{Access, Change, Component, Event, Request, Stopped, start::Start};
use skein_forge_machine_world::{
    fixtures::{self, FORGE},
    github::WALL_BASE,
    joint::{self, Joint, REPO},
};
use skein_lib::{Duration, Secret, Time, Wall};

const SEED: u64 = 7;
const SECOND: Duration = Duration::from_secs(1);
const ACCEPTED: u64 = 1;
const CALL: u64 = 1;
const READY: Start = Start::Ready { cap: 100 };
// The accepted unsuspended installation is silent; example and suspended other emit changes.
const INSTALLATION_EVENTS: usize = 2;
// Listing has two pages; startup also mints API and git tokens and reads the viewer.
const START_REQUESTS: u64 = 5;
// A fresh branch call resolves the repository and reads its branch.
const CALL_REQUESTS: u64 = 2;

fn world(seed: u64, cut_chance: u32) -> Joint {
    Joint::github(
        seed,
        cut_chance,
        fixtures::limits(),
        fixtures::github_settings(),
        fixtures::shared(),
        joint::checks(1000),
    )
}
fn connect(world: &mut Joint, access: Access) {
    world.action(Request::Installations { forge: FORGE, accepted: Box::from([ACCEPTED]) });
    world.action(Request::Connect { forge: FORGE, access });
}
fn branch(world: &mut Joint) {
    world.call(CALL, REPO, fixtures::branch());
    world.run_for(SECOND);
}

#[test]
fn records_then_connect_make_github_ready_for_repository_calls() {
    let mut world = world(SEED, 0);
    world.run_for(SECOND);
    assert_eq!(world.connected, 0);
    assert_eq!(world.tally().total, 0);
    assert_eq!(world.component.start(FORGE), Some(Start::Unconnected));
    connect(&mut world, fixtures::github_access());
    world.run_for(SECOND);
    assert_eq!(world.connected, 1);
    assert_eq!(world.component.start(FORGE), Some(READY));
    assert_eq!(world.installations.len(), INSTALLATION_EVENTS);
    assert!(world.installations.iter().any(|event| matches!(event,
        Event::Installation { forge: FORGE, installation: 2, account: Some(account), change: Change::Created }
        if account.id == 23 && account.login.as_ref() == b"example"
    )));
    assert_eq!(world.git_tokens.len(), 1);
    assert!(matches!(world.git_tokens.first(), Some(Event::GitToken {
        forge: FORGE, installation: ACCEPTED, account, token, ..
    }) if account.id == 22 && account.login.as_ref() == b"acme" && !token.is_empty()));
    branch(&mut world);
    assert!(matches!(world.answered(CALL).expect("branch answer").result, Ok(Answer::Commit(_))));
    assert_eq!(world.answers.len(), 1);
    assert_eq!(world.tally().total, START_REQUESTS + CALL_REQUESTS);
    assert_eq!(world.start_counts().served, START_REQUESTS);
    assert_eq!(world.request_counts().written, CALL_REQUESTS);
    assert_eq!(world.tally().users.get(&9), Some(&(START_REQUESTS + CALL_REQUESTS)));
    assert_eq!(world.tally().routes.get(&skein_fake_forge_protocol::Route::Branch), Some(&1));
}

#[test]
fn cutting_github_pipes_connect_and_answer_each_call_once() {
    let mut cut_heads = 0;
    // Fixed seeds cover resets during startup and calls at a 30% connection cut chance.
    for seed in [3, 7, 19] {
        let mut world = world(seed, 300);
        connect(&mut world, fixtures::github_access());
        world.run_for(Duration::from_secs(30));
        assert_eq!(world.connected, 1, "seed {seed}");
        assert_eq!(world.component.start(FORGE), Some(READY), "seed {seed}");
        branch(&mut world);
        assert_eq!(world.answers.len(), 1, "seed {seed}");
        assert!(world.answered(CALL).expect("one branch terminal").result.is_ok(), "seed {seed}");
        cut_heads += world.start_counts().cut_in_head + world.request_counts().cut_in_head;
    }
    assert!(cut_heads > 0, "fixed seeds cut request heads");
}

#[test]
fn a_refused_github_app_key_is_kept_as_stopped() {
    let mut world = world(SEED, 0);
    let mut access = fixtures::github_access();
    access.credential =
        Credential::App { id: 7, key: Secret::new(Box::from(*KEYS.get(1).expect("other public test key"))) };
    connect(&mut world, access);
    world.run_for(SECOND);
    assert_eq!(world.stopped, [(FORGE, Stopped::KeyRefused)]);
    assert_eq!(world.connected, 0);
    assert!(world.git_tokens.is_empty());
    assert_eq!(world.tally().unauthorized, 1);
}

#[test]
fn github_time_follows_the_component_and_fake_wall() {
    let mut world = world(SEED, 0);
    world.time(Time::from_nanos(SECOND.as_nanos()));
    let wall = Wall::from_nanos(WALL_BASE.as_nanos() + SECOND.as_nanos());
    assert_eq!(world.env.wall, wall);
    assert_eq!(world.fake_env.wall, wall);
    assert_eq!(world.fake_env.now.as_nanos(), wall.as_nanos());
    assert_eq!(world.now(), Time::from_nanos(SECOND.as_nanos()));
}

#[test]
fn github_replacement_preserves_the_domain_and_configured_access() {
    let mut world = world(SEED, 0);
    connect(&mut world, fixtures::github_access());
    world.run_for(SECOND);
    world.issue(b"from the public domain", b"body");
    let mut settings = fixtures::github_settings();
    settings.forges.get_mut(0).expect("forge").access = Some(fixtures::github_access());
    let component = Component::new(settings, &world.env.limits, &fixtures::shared()).expect("component");
    world.replace(component);
    world.action(Request::Installations { forge: FORGE, accepted: Box::from([ACCEPTED]) });
    world.run_for(SECOND);
    world.call(CALL, REPO, Op::Read(Read::Items { since: Time::ZERO, page: 1, kind: Some(api::Kind::Issue) }));
    world.run_for(SECOND);
    let Ok(Answer::Items { items, .. }) = &world.answered(CALL).expect("items").result else { panic!("items answer") };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title.as_ref(), b"from the public domain");
    assert_eq!(world.observations.len(), 1);
    assert!(!world.busy());
    assert_eq!(world.connected, 2);
}

#[test]
fn github_capacity_facts_are_kept_for_the_owner() {
    let mut limits = fixtures::limits();
    limits.installations = 1;
    let mut world =
        Joint::github(SEED, 0, limits, fixtures::github_settings(), fixtures::shared(), joint::checks(1000));
    connect(&mut world, fixtures::github_access());
    world.run_for(SECOND);
    assert_eq!(world.connected, 1);
    assert!(matches!(
        world.facts.as_slice(),
        [Event::Fact { forge: FORGE, fact: skein_forge_connection::Fact::Unkept { installation: 2 } }]
    ));
}
