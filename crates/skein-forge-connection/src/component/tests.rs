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
    c.open_start(0, &env, &mut io);
}
#[test]
fn the_last_api_token_is_still_issued() {
    let (mut c, env, mut io) = token_fixture();
    c.next_token = LAST_API_OWNER;
    assert_eq!(c.open_start(0, &env, &mut io).raw(), LAST_API_OWNER);
}
