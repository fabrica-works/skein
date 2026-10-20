use skein_forge_connection::{Access, Event, Request, Stopped, settings::Kind, start::Start};
use skein_forge_machine_world::{
    fixtures::{self, FORGE},
    github,
    machine::{Answer, Machine, Response},
};
use skein_lib::{Duration, Secret, Time, Wall};

const SEED: u64 = 7;
const SECOND: Duration = Duration::from_secs(1);
const PAGE_ONE: &str = "GET /app/installations?per_page=100&page=1";
const PAGE_TWO: &str = "GET /app/installations?per_page=100&page=2";
const MINT: &str = "POST /app/installations/1/access_tokens";
const VIEWER: &str = "POST /graphql";
const START_REQUESTS: usize = 5; // two pages, Full, Contents and Viewer
fn world(mut renewal: Vec<Answer>) -> Machine {
    let mut settings = fixtures::settings();
    let forge = settings.forges.get_mut(0).expect("GitHub fixture");
    forge.kind = Kind::GitHub;
    forge.access = Some(Access {
        credential: skein_forge::credential::Credential::App { id: 7, key: Secret::new(Box::from(github::app_key())) },
        user: 9,
        webhook_secret: None,
    });
    let mut script: Vec<Answer> = [PAGE_ONE, PAGE_TWO, MINT, MINT, VIEWER].into_iter().map(Answer::served).collect();
    script.append(&mut renewal);
    let mut world = Machine::github(SEED, fixtures::limits(), settings, script);
    world.action(Request::Installations { forge: FORGE, accepted: Box::from([1]) });
    world.run_for(SECOND);
    world
}
fn expiry(world: &Machine, index: usize) -> Wall {
    match world.git_tokens().get(index).expect("Contents history") {
        Event::GitToken { expires, .. } => *expires,
        Event::Installation { .. }
        | Event::Fact { .. }
        | Event::Answered(_)
        | Event::Hint(_)
        | Event::Connected { .. }
        | Event::Stopped { .. }
        | Event::ListenerLost
        | Event::Closed => panic!("token history"),
    }
}
fn due(world: &Machine) -> Time {
    Time::from_nanos(
        expiry(world, 0).as_nanos() - github::WALL_BASE.as_nanos() - fixtures::limits().renew_before.as_nanos(),
    )
}
fn ready(world: &Machine) {
    assert_eq!(world.start(FORGE), Some(Start::Ready { cap: 100 }));
    assert_eq!(world.connected_forges(), &[FORGE]);
}
#[test]
fn due_installation_renews_without_reconnecting() {
    let mut world = world(vec![Answer::served(MINT), Answer::served(MINT)]);
    let due = due(&world);
    world.advance_to(Time::from_nanos(due.as_nanos() - 1));
    assert_eq!(world.requests().len(), START_REQUESTS);
    world.advance_to(due);
    ready(&world);
    assert_eq!(world.git_tokens().len(), 2);
    assert!(expiry(&world, 1) > expiry(&world, 0));
    // Exactly the two renewal requests: no further Listing or Viewer.
    assert_eq!(world.requests().len(), START_REQUESTS + 2);
    assert!(world.script_done());
    assert!(world.close());
}
#[test]
fn failed_renewal_waits_then_renews() {
    let mut world = world(vec![
        Answer { request: MINT, response: Response::Bytes(fixtures::response(503, b"{}")) },
        Answer::served(MINT),
        Answer::served(MINT),
    ]);
    let due = due(&world);
    world.advance_to(due);
    ready(&world);
    assert_eq!(world.requests().len(), START_REQUESTS + 1);
    world.advance_to(due.saturating_add(Duration::from_millis(999)));
    ready(&world);
    assert_eq!(world.requests().len(), START_REQUESTS + 1);
    world.advance_to(due.saturating_add(SECOND));
    ready(&world);
    assert_eq!(world.requests().len(), START_REQUESTS + 3);
    assert_eq!(world.git_tokens().len(), 2);
    assert!(expiry(&world, 1) > expiry(&world, 0));
    assert!(world.script_done());
    assert!(world.close());
}
#[test]
fn renewal_key_refusal_stops_a_ready_generation() {
    let mut world = world(vec![Answer { request: MINT, response: Response::Bytes(fixtures::response(401, b"{}")) }]);
    let due = due(&world);
    world.advance_to(due);
    world.advance_to(due.saturating_add(Duration::from_secs(100)));
    assert_eq!(world.connected_forges(), &[FORGE]);
    assert_eq!(world.stopped(), Some((FORGE, Stopped::KeyRefused)));
    assert_eq!(world.git_tokens().len(), 1);
    assert_eq!(world.requests().len(), START_REQUESTS + 1);
    assert!(world.script_done());
    assert!(world.close());
}
