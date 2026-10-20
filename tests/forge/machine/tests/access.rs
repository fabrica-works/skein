use skein_forge::api::Error;
use skein_forge_connection::{Access, Request, Stopped, Value};
use skein_forge_machine_world::{
    fixtures::{self, FORGE, TOKEN, TOKEN_B, WRITER, access, branch, locate, locate_script, unconnected},
    machine::{Answer, Machine, Pause, Response},
};
use skein_lib::{Duration, Time};
const SECOND: Duration = Duration::from_secs(1);
const SEED: u64 = 7;
// The two start reads precede a call's first exchange.
const CALL_REQUEST: usize = 2;
// Replacement adds two new start reads after the held repository resolution.
const REPLACED_REQUESTS: usize = 5;
const NEW_USER_REQUEST: usize = 3;
fn connect(world: &mut Machine, access: Access) {
    world.action(Request::Connect { forge: FORGE, access });
}
fn disconnected(script: Vec<Answer>) -> Machine {
    Machine::with(SEED, fixtures::limits(), unconnected(), script)
}
fn failed(world: &Machine, call: u64, error: Error, cost: u32) {
    let answered = world.answered(call).expect("call ended");
    assert_eq!((&answered.result, answered.cost), (&Err(error), cost));
}
fn authorization(request: &[u8], token: &[u8]) {
    let value = [b"Authorization: token ".as_slice(), token].concat();
    assert!(request.windows(value.len()).any(|window| window == value));
}
fn start_script() -> Vec<Answer> {
    vec![Answer::case("GET /api/v1/user", "user"), Answer::case("GET /api/v1/settings/api", "settings-api")]
}
#[test]
fn a_forge_with_no_access_answers_not_connected_at_no_cost() {
    let mut world = disconnected(vec![]);
    world.call(1, fixtures::REPO, locate());
    world.run_for(SECOND);
    failed(&world, 1, Error::NotConnected, 0);
    assert_eq!(world.connections_opened(), 0);
    assert!(world.requests().is_empty());
    assert!(world.close());
}
#[test]
fn a_connect_starts_the_forge_and_says_connected() {
    let mut world = disconnected(locate_script());
    connect(&mut world, access(TOKEN, WRITER, None));
    assert_eq!(world.connected_forges(), &[FORGE]);
    world.call(1, fixtures::REPO, locate());
    world.run_for(SECOND);
    let answer = world.answered(1).expect("located");
    assert!(answer.result.is_ok());
    assert_eq!(answer.cost, 1);
    for request in world.requests() {
        authorization(request, TOKEN);
    }
    assert!(world.close());
}
#[test]
fn a_refused_connect_says_stopped_naming_the_value() {
    let mut world = disconnected(vec![]);
    connect(&mut world, access(b"", WRITER, None));
    assert_eq!(world.stopped(), Some((FORGE, Stopped::Invalid { value: Value::Token })));
    assert_eq!(world.connections_opened(), 0);
    world.call(1, fixtures::REPO, locate());
    world.run_for(SECOND);
    failed(&world, 1, Error::NotConnected, 0);
    assert!(world.close());
}
#[test]
fn a_connect_of_another_user_is_refused_and_drops_the_access() {
    let mut world = Machine::ready(SEED, start_script());
    world.run_for(SECOND);
    connect(&mut world, access(TOKEN, 3, None));
    assert_eq!(world.stopped(), Some((FORGE, Stopped::Invalid { value: Value::User })));
    world.call(1, fixtures::REPO, locate());
    world.run_for(SECOND);
    failed(&world, 1, Error::NotConnected, 0);
    connect(&mut world, access(TOKEN, WRITER, None));
    assert_eq!(world.connected_forges(), &[FORGE, FORGE]);
    assert!(world.close());
}
#[test]
fn a_replacement_answers_the_written_exchange_and_ends_the_call_forbidden() {
    let mut script = vec![Answer::case("GET /api/v1/repositories/1", "repository-by-id")];
    script.extend(start_script());
    let mut world = Machine::ready(SEED, script);
    world.pause_at(CALL_REQUEST, Pause::Reading(0));
    world.call(1, fixtures::REPO, branch());
    world.run_for(SECOND);
    connect(&mut world, access(TOKEN_B, WRITER, None));
    assert_eq!(world.connected_forges(), &[FORGE, FORGE]);
    world.release_response();
    world.run_for(SECOND);
    failed(&world, 1, Error::Forbidden, 1);
    authorization(world.requests().get(NEW_USER_REQUEST).expect("new user"), TOKEN_B);
    assert_eq!(world.requests().len(), REPLACED_REQUESTS);
    assert!(world.script_done());
    assert!(world.close());
}
#[test]
fn a_connect_during_a_start_ends_it_unheard_and_waiting_calls_take_the_new_one() {
    let mut script = vec![Answer::case("GET /api/v1/user", "user")];
    script.extend(start_script());
    script.extend([
        Answer::case("GET /api/v1/repositories/1", "repository-by-id"),
        Answer::case("GET /api/v1/repos/fixture/specimen/branches/main", "branch"),
    ]);
    let mut world = Machine::new(SEED, script);
    world.pause_at(0, Pause::Reading(0));
    world.call(1, fixtures::REPO, branch());
    world.run_for(SECOND);
    connect(&mut world, access(TOKEN_B, WRITER, None));
    assert_eq!(world.connected_forges(), &[FORGE]);
    assert_eq!(world.stopped(), None);
    world.run_for(SECOND);
    let answer = world.answered(1).expect("branch");
    assert!(answer.result.is_ok());
    assert_eq!(answer.cost, 2);
    for request in world.requests().iter().skip(1) {
        authorization(request, TOKEN_B);
    }
    assert!(world.script_done());
    assert!(world.close());
}
#[test]
fn a_stopped_start_ends_waiting_calls_not_connected_until_the_next_connect() {
    let mut script = vec![Answer {
        request: "GET /api/v1/user",
        response: Response::Bytes(fixtures::response(200, br#"{"id":8,"login":"other"}"#)),
    }];
    script.extend(locate_script());
    let mut world = disconnected(script);
    world.pause_at(0, Pause::Reading(0));
    connect(&mut world, access(TOKEN, WRITER, None));
    world.call(1, fixtures::REPO, locate());
    world.run_for(SECOND);
    assert!(world.answered(1).is_none());
    world.release_response();
    world.advance_to(Time::from_nanos(SECOND.as_nanos()));
    assert_eq!(world.stopped(), Some((FORGE, Stopped::OtherUser { user: 8 })));
    failed(&world, 1, Error::NotConnected, 0);
    connect(&mut world, access(TOKEN, WRITER, None));
    assert_eq!(world.connected_forges(), &[FORGE]);
    world.call(2, fixtures::REPO, locate());
    world.run_for(SECOND);
    let answer = world.answered(2).expect("located");
    assert!(answer.result.is_ok());
    assert_eq!(answer.cost, 1);
    assert!(world.close());
}
