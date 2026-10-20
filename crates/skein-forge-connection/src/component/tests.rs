use super::*;
use crate::tests::fixtures;

// forge-connection.md 6.1 reserves the listener's owner prefix.
const LISTENER_OWNER: u64 = 1_u64 << 59_u32;
const LAST_API_OWNER: u64 = LISTENER_OWNER - 1;
fn token_fixture() -> (Component, Env<Limits>, Queue<skein_io::Request>) {
    let limits = fixtures::limits();
    let c = Component::new(fixtures::settings(), &limits, &fixtures::shared()).expect("valid");
    let env = Env { now: Time::ZERO, wall: skein_lib::Wall::EPOCH, limits };
    (c, env, Queue::with_capacity(MAX_OUT.below))
}
#[test]
#[should_panic(expected = "connection token prefix")]
fn api_tokens_stop_before_the_listener_range() {
    let (mut c, env, mut io) = token_fixture();
    c.next_token = LISTENER_OWNER;
    c.open_own(0, &env, &mut io);
}
#[test]
fn the_last_api_token_is_still_issued() {
    let (mut c, env, mut io) = token_fixture();
    c.next_token = LAST_API_OWNER;
    assert_eq!(c.open_own(0, &env, &mut io).raw(), LAST_API_OWNER);
}

#[test]
fn forgejo_starts_leave_the_github_reserved_slot_free() {
    // Three Forgejo starts contend for the two call slots; GitHub has its own third slot.
    const FORGEJO_FORGES: u32 = 3;
    const GITHUB_AT: u32 = FORGEJO_FORGES;
    const FORGES: u32 = FORGEJO_FORGES + 1;
    let limits = Limits { forges: FORGES, ..fixtures::limits() };
    let mut settings = fixtures::settings();
    let mut forges = List::with_capacity(FORGES);
    for number in 1..=FORGEJO_FORGES {
        let mut forge = fixtures::settings().forges.into_boxed().into_iter().next().expect("fixture forge");
        forge.number = u16::try_from(number).expect("forge number");
        forges.push(forge).expect("forge room");
    }
    let mut github = fixtures::github_settings().forges.into_boxed().into_iter().next().expect("GitHub forge");
    github.number = u16::try_from(FORGES).expect("forge number");
    forges.push(github).expect("forge room");
    settings.forges = forges;
    let env = Env { now: Time::ZERO, wall: skein_lib::Wall::EPOCH, limits };
    let mut c = Component::new(settings, &limits, &fixtures::shared()).expect("fixture limits");
    let mut up = Queue::with_capacity(MAX_OUT.above);
    let mut io = Queue::with_capacity(MAX_OUT.below);
    c.begin(&env, &mut up, &mut io);
    assert_eq!(c.call_connections(), limits.connections);
    assert!(c.own_tokens.get(GITHUB_AT).expect("GitHub slot").is_some());
    assert_eq!(c.connections.len(), 3);
}
