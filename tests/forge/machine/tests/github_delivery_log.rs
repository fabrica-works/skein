use skein_forge::{api, forgejo::document::Body, github::delivery::Delivery};
use skein_forge_connection::{Event, Fact, Limits, Request, Stopped};
use skein_forge_machine_world::{
    fixtures::{self, FORGE},
    machine::{Answer, Machine},
};
use skein_lib::Duration;

const SEED: u64 = 7;
const SECOND: Duration = Duration::from_secs(1);
// Above the fake's 2.2-second visibility delay, below token renewal.
const LOG_POLL: Duration = Duration::from_secs(10);
const CATCH_UP: Duration = Duration::from_secs(60);
// Two rows per page make three new deliveries cross a cursor before the kept guid.
const PAGE_ROWS: u32 = 2;
const FIRST: &str = "GET /app/hook/deliveries?per_page=2";
const INSTALLATIONS: [&str; 2] =
    ["GET /app/installations?per_page=100&page=1", "GET /app/installations?per_page=100&page=2"];
const START: [&str; 5] = [
    INSTALLATIONS[0],
    INSTALLATIONS[1],
    "POST /app/installations/1/access_tokens",
    "POST /app/installations/1/access_tokens",
    "POST /graphql",
];
const REPO: api::Repository = api::Repository { forge: FORGE, repository: 2 };

fn world(pages: u32, catch_up: Duration) -> Machine {
    let limits =
        Limits { inner: PAGE_ROWS, log_poll: LOG_POLL, log_pages: pages, log_catch_up: catch_up, ..fixtures::limits() };
    let mut world =
        Machine::github(SEED, limits, fixtures::github_settings(), START.into_iter().map(Answer::served).collect());
    world.action(Request::Installations { forge: FORGE, accepted: Box::from([1]) });
    world.action(Request::Connect { forge: FORGE, access: fixtures::github_access() });
    world.run_for(SECOND);
    world
}
fn release(world: &mut Machine, number: u64) -> u64 {
    let wall = world.wall();
    world.github_fake_mut().release(
        Delivery::Issue { repository: u64::from(REPO.repository), number, body: Body::Kept(Box::from([])) },
        wall,
    )
}
fn delivery(id: u64) -> String {
    format!("GET /app/hook/deliveries/{id}")
}
fn next(id: u64) -> String {
    format!("GET /app/hook/deliveries?per_page=2&cursor=d-{id}")
}
fn poll(world: &mut Machine, requests: Vec<String>) {
    world.log_requests(requests);
    world.run_for(LOG_POLL);
}
fn baseline(world: &mut Machine) -> u64 {
    let id = release(world, 1);
    poll(world, vec![FIRST.into()]);
    id
}
fn hints(world: &Machine) -> Vec<api::Hint> {
    world.hints().iter().map(|hinted| hinted.hint.clone()).collect()
}
fn issue(number: u64) -> api::Hint {
    api::Hint { repository: REPO, change: api::Change::Item(number), key: None }
}
fn finish(world: &mut Machine) {
    assert!(world.script_done(), "every expected read made");
    assert!(world.close());
    assert_eq!(world.closed_events(), 1);
}

#[test]
fn github_first_log_poll_keeps_newest_without_older_hints() {
    let mut world = world(2, CATCH_UP);
    for number in [1, 2, 3] {
        release(&mut world, number);
    }
    poll(&mut world, vec![FIRST.into()]);
    assert!(hints(&world).is_empty());
    assert!(world.facts().is_empty());
    // The next poll stops at the newest guid, leaving the older rows unread.
    let id = release(&mut world, 4);
    poll(&mut world, vec![FIRST.into(), delivery(id)]);
    assert_eq!(hints(&world), vec![issue(4)]);
    finish(&mut world);
}

#[test]
fn github_log_pages_to_kept_guid_hinting_newest_first_once() {
    let mut world = world(2, CATCH_UP);
    baseline(&mut world);
    let ids: Vec<_> = [2, 3, 4].into_iter().map(|number| release(&mut world, number)).collect();
    poll(&mut world, vec![FIRST.into(), delivery(ids[2]), delivery(ids[1]), next(ids[1]), delivery(ids[0])]);
    poll(&mut world, vec![FIRST.into()]);
    assert_eq!(hints(&world), vec![issue(4), issue(3), issue(2)]);
    assert!(world.facts().is_empty());
    finish(&mut world);
}

#[test]
fn github_log_page_bound_reports_one_gap_and_rereads_installations() {
    let mut world = world(1, CATCH_UP);
    baseline(&mut world);
    let ids: Vec<_> = [2, 3, 4].into_iter().map(|number| release(&mut world, number)).collect();
    let mut requests = vec![FIRST.into(), delivery(ids[2]), delivery(ids[1])];
    requests.extend(INSTALLATIONS.map(String::from));
    poll(&mut world, requests);
    poll(&mut world, vec![FIRST.into()]);
    assert_eq!(hints(&world), vec![issue(4), issue(3)]);
    assert_eq!(world.facts().len(), 1);
    assert!(matches!(world.facts()[0], Event::Fact { forge: FORGE, fact: Fact::Gap }));
    finish(&mut world);
}

#[test]
fn github_log_age_bound_reports_one_gap_and_rereads_installations() {
    // The delivery is visible by 2.2 seconds and older than this bound at the poll.
    const SHORT_CATCH_UP: Duration = Duration::from_secs(3);
    let mut world = world(2, SHORT_CATCH_UP);
    baseline(&mut world);
    release(&mut world, 2);
    let mut requests = vec![FIRST.into()];
    requests.extend(INSTALLATIONS.map(String::from));
    poll(&mut world, requests);
    poll(&mut world, vec![FIRST.into()]);
    assert!(hints(&world).is_empty());
    assert_eq!(world.facts().len(), 1);
    assert!(matches!(world.facts()[0], Event::Fact { forge: FORGE, fact: Fact::Gap }));
    finish(&mut world);
}

fn validator(request: &[u8]) -> Option<&[u8]> {
    request.split(|b| *b == b'\n').find_map(|line| line.strip_prefix(b"If-None-Match: ").map(<[u8]>::trim_ascii))
}

#[test]
fn github_log_304_preserves_validator_and_kept_guid() {
    // Startup's five requests precede the first page and its two conditional polls.
    const CONDITIONAL: usize = START.len() + 1;
    let mut world = world(2, CATCH_UP);
    baseline(&mut world);
    poll(&mut world, vec![FIRST.into()]);
    poll(&mut world, vec![FIRST.into()]);
    assert_eq!(&world.github_fake_mut().statuses()[START.len()..], &[200, 304, 304]);
    let validators: Vec<_> = world.requests()[CONDITIONAL..].iter().map(|r| validator(r)).collect();
    assert_eq!(validators.len(), 2);
    assert!(validators[0].is_some());
    assert_eq!(validators[0], validators[1]);
    let id = release(&mut world, 2);
    poll(&mut world, vec![FIRST.into(), delivery(id)]);
    assert_eq!(hints(&world), vec![issue(2)]);
    assert!(world.facts().is_empty());
    finish(&mut world);
}

#[test]
fn github_redelivery_is_skipped_and_never_kept_as_newest() {
    let mut world = world(2, CATCH_UP);
    let original = baseline(&mut world);
    let newer = release(&mut world, 2);
    let wall = world.wall();
    let redelivery = world.github_fake_mut().redeliver(original, wall);
    poll(&mut world, vec![FIRST.into(), delivery(newer), next(newer)]);
    let latest = release(&mut world, 3);
    poll(&mut world, vec![FIRST.into(), delivery(latest), next(redelivery)]);
    assert_eq!(hints(&world), vec![issue(2), issue(3)]);
    assert!(world.facts().is_empty());
    finish(&mut world);
}

#[test]
fn github_log_401_stops_the_forge_with_key_refused() {
    let mut world = world(2, CATCH_UP);
    baseline(&mut world);
    world.github_fake_mut().revoke_key();
    poll(&mut world, vec![FIRST.into()]);
    world.run_for(LOG_POLL);
    assert_eq!(world.stopped(), Some((FORGE, Stopped::KeyRefused)));
    assert!(hints(&world).is_empty());
    finish(&mut world);
}
