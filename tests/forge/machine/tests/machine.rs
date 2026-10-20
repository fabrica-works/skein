use skein_forge::api::{Answer as ResultAnswer, Error, Op, Read};
use skein_forge_connection::{StartRead, Stopped};
use skein_forge_machine_world::{
    fixtures,
    machine::{self, Answer, Connect, Machine, Response},
};
use skein_lib::Duration;
const SECOND: Duration = Duration::from_secs(1);
#[test]
fn the_start_reaches_ready_from_the_recorded_answers() {
    let mut world = Machine::ready(7, vec![]);
    world.run_for(SECOND);
    assert!(world.script_done());
    assert_eq!(world.requests().len(), 2);
    assert_eq!(world.stopped(), None);
    assert!(world.close());
}
#[test]
fn another_user_stops_the_start_once_naming_the_forge() {
    let mut world = Machine::new(
        7,
        vec![Answer {
            request: "GET /api/v1/user",
            response: Response::Bytes(fixtures::response(200, br#"{"id":8,"login":"other"}"#)),
        }],
    );
    world.run_for(Duration::from_secs(3));
    assert_eq!(world.stopped(), Some((fixtures::FORGE, Stopped::OtherUser { user: 8 })));
    assert_eq!(world.requests().len(), 1);
    assert!(world.script_done());
    assert!(world.close());
}
#[test]
fn settings_answered_404_stops_the_start_with_its_status() {
    let mut world = Machine::new(
        7,
        vec![
            Answer::case("GET /api/v1/user", "user"),
            Answer { request: "GET /api/v1/settings/api", response: Response::Bytes(fixtures::response(404, b"")) },
        ],
    );
    world.run_for(SECOND);
    assert_eq!(
        world.stopped(),
        Some((fixtures::FORGE, Stopped::Refused { read: StartRead::ApiSettings, status: 404 }))
    );
    assert!(world.script_done());
    assert!(world.close());
}
#[test]
fn an_unreachable_forge_is_checked_again_after_its_backoff() {
    let mut world = Machine::ready(7, vec![]);
    world.connects(&[Connect::Refused]);
    world.run_for(Duration::from_millis(999));
    assert_eq!(world.connections_opened(), 1);
    world.run_for(SECOND);
    assert_eq!(world.connections_opened(), 3);
    assert!(world.script_done());
    assert!(world.close());
}
#[test]
fn a_call_waiting_past_its_deadline_is_unavailable_at_no_cost() {
    let mut world = Machine::new(7, vec![Answer { request: "GET /api/v1/user", response: Response::Silent }]);
    world.call(1, fixtures::REPO, fixtures::branch());
    world.run_for(Duration::from_secs(10));
    let answered = world.answered(1).expect("waiting call ended");
    assert_eq!((answered.cost, &answered.result), (0, &Err(Error::Unavailable)));
    assert!(world.close());
}
#[test]
fn a_call_silent_past_its_deadline_is_a_timeout_and_aborts() {
    let mut world = Machine::ready(
        7,
        vec![
            Answer::case("GET /api/v1/repositories/1", "repository-by-id"),
            Answer { request: "GET /api/v1/repos/fixture/specimen/branches/main", response: Response::Silent },
        ],
    );
    world.call(1, fixtures::REPO, fixtures::branch());
    world.run_for(Duration::from_secs(10));
    let answered = world.answered(1).expect("written call ended");
    assert_eq!((answered.cost, &answered.result), (2, &Err(Error::Timeout)));
    assert!(world.aborts() > 0);
    assert!(world.close());
}
#[test]
fn a_path_dropped_on_404_is_resolved_again_by_id() {
    let mut world = Machine::ready(
        7,
        vec![
            Answer::case("GET /api/v1/repositories/1", "repository-by-id"),
            Answer::case("GET /api/v1/repos/fixture/specimen/branches/gone", "missing-issue"),
            Answer::case("GET /api/v1/repositories/1", "repository-by-id"),
            Answer::case("GET /api/v1/repos/fixture/specimen/branches/main", "branch"),
        ],
    );
    world.call(1, fixtures::REPO, Op::Read(Read::Branch { branch: Box::from(*b"gone") }));
    world.run_for(SECOND);
    world.call(2, fixtures::REPO, fixtures::branch());
    world.run_for(SECOND);
    let first = world.answered(1).expect("missing answered");
    assert_eq!((first.cost, &first.result), (2, &Err(Error::Missing)));
    let second = world.answered(2).expect("branch answered");
    assert_eq!(second.cost, 2);
    assert!(matches!(&second.result, Ok(ResultAnswer::Commit(_))), "{:?}", second.result);
    assert!(world.script_done());
    assert!(world.close());
}
#[test]
fn a_redirect_drops_the_path_and_answers_unavailable() {
    let mut world = Machine::ready(
        7,
        vec![
            Answer::case("GET /api/v1/repositories/1", "repository-by-id"),
            Answer {
                request: "GET /api/v1/repos/fixture/specimen/branches/main",
                response: Response::Bytes(fixtures::response(301, b"")),
            },
            Answer::case("GET /api/v1/repositories/1", "repository-by-id"),
            Answer::case("GET /api/v1/repos/fixture/specimen/branches/main", "branch"),
        ],
    );
    world.call(1, fixtures::REPO, fixtures::branch());
    world.run_for(SECOND);
    assert_eq!(world.answered(1).expect("redirect answered").result, Err(Error::Unavailable));
    world.call(2, fixtures::REPO, fixtures::branch());
    world.run_for(SECOND);
    assert_eq!(world.answered(2).expect("branch answered").cost, 2);
    assert!(world.script_done());
    assert!(world.close());
}
#[test]
fn a_body_past_body_bytes_is_carried_empty_without_its_key() {
    let body = format!(
        "{{\"number\":1,\"state\":\"open\",\"user\":{{\"id\":2,\"login\":\"writer\"}},\"labels\":[],\"updated_at\":\"2020-01-01T00:00:00Z\",\"title\":\"title\",\"body\":\"<!-- kit:key ab -->\\n{}\"}}",
        "x".repeat(1046)
    );
    let mut world = Machine::ready(
        7,
        vec![
            Answer::case("GET /api/v1/repositories/1", "repository-by-id"),
            Answer {
                request: "GET /api/v1/repos/fixture/specimen/issues?state=all&since=1970-01-01T00%3A00%3A00Z&sort=leastupdate&page=1&limit=2",
                response: Response::Bytes(fixtures::dated_response(200, format!("[{body}]").as_bytes())),
            },
        ],
    );
    world.call(1, fixtures::REPO, Op::Read(Read::Items { since: skein_lib::Time::ZERO, page: 1, kind: None }));
    world.run_for(SECOND);
    let result = &world.answered(1).expect("items answered").result;
    let Ok(ResultAnswer::Items { items, more, .. }) = result else {
        panic!("items: {result:?}");
    };
    assert_eq!(items.len(), 1);
    assert!(!more);
    assert_eq!(items[0].key, None);
    assert!(items[0].body.is_empty());
    assert!(world.close());
}
#[test]
fn an_idle_connection_closes_and_the_next_call_connects() {
    let mut branch = machine::case_bytes("branch");
    let at = branch
        .windows(b"Connection: close\r\n".len())
        .position(|bytes| bytes == b"Connection: close\r\n")
        .expect("close header");
    branch.drain(at..at + b"Connection: close\r\n".len());
    let mut world = Machine::ready(
        7,
        vec![
            Answer::case("GET /api/v1/repositories/1", "repository-by-id"),
            Answer { request: "GET /api/v1/repos/fixture/specimen/branches/main", response: Response::Bytes(branch) },
            Answer::case("GET /api/v1/repos/fixture/specimen/branches/main", "branch"),
        ],
    );
    world.call(1, fixtures::REPO, fixtures::branch());
    world.run_for(SECOND);
    let opened = world.connections_opened();
    world.run_for(Duration::from_secs(5));
    world.call(2, fixtures::REPO, fixtures::branch());
    world.run_for(SECOND);
    assert_eq!(world.connections_opened(), opened + 1);
    assert_eq!(world.answered(2).expect("after idle").cost, 1);
    assert!(world.script_done());
    assert!(world.close());
}
#[test]
fn a_connection_marked_close_finishes_its_call_and_the_next_connects() {
    let mut world = Machine::ready(
        7,
        vec![
            Answer::case("GET /api/v1/repositories/1", "repository-by-id"),
            Answer::case("GET /api/v1/repos/fixture/specimen/branches/main", "branch"),
            Answer::case("GET /api/v1/repos/fixture/specimen/branches/main", "branch"),
        ],
    );
    world.call(1, fixtures::REPO, fixtures::branch());
    world.run_for(SECOND);
    let opened = world.connections_opened();
    world.call(2, fixtures::REPO, fixtures::branch());
    world.run_for(SECOND);
    assert_eq!(world.connections_opened(), opened + 1);
    assert_eq!(world.answered(2).expect("after close").cost, 1);
    assert!(world.close());
}
#[test]
fn a_well_framed_malformed_document_is_invalid_answer() {
    let mut world = Machine::ready(
        7,
        vec![
            Answer::case("GET /api/v1/repositories/1", "repository-by-id"),
            Answer {
                request: "GET /api/v1/repos/fixture/specimen/branches/main",
                response: Response::Bytes(fixtures::response(200, b"{")),
            },
        ],
    );
    world.call(1, fixtures::REPO, fixtures::branch());
    world.run_for(SECOND);
    let answered = world.answered(1).expect("malformed answered");
    assert_eq!((answered.cost, &answered.result), (2, &Err(Error::InvalidAnswer)));
    assert!(world.close());
}
#[test]
fn a_reset_after_a_written_request_is_timeout() {
    let mut world = Machine::ready(
        7,
        vec![
            Answer::case("GET /api/v1/repositories/1", "repository-by-id"),
            Answer { request: "GET /api/v1/repos/fixture/specimen/branches/main", response: Response::Reset },
        ],
    );
    world.call(1, fixtures::REPO, fixtures::branch());
    world.run_for(SECOND);
    let answered = world.answered(1).expect("reset answered");
    assert_eq!((answered.cost, &answered.result), (2, &Err(Error::Timeout)));
    assert!(world.close());
}
#[test]
fn a_write_body_is_read_before_its_scripted_response() {
    let mut world = Machine::ready(7, vec![Answer::case("GET /api/v1/repositories/1", "repository-by-id"), Answer { request: "POST /api/v1/repos/fixture/specimen/issues", response: Response::Bytes(fixtures::response(201, br#"{"number":7,"state":"open","user":{"id":2,"login":"writer"},"labels":[],"updated_at":"2020-01-01T00:00:00Z","title":"title","body":"body"}"#)) }]);
    world.call(
        1,
        fixtures::REPO,
        Op::Write(skein_forge::api::Write::CreateIssue {
            key: Box::from([0xab]),
            title: Box::from(*b"title"),
            body: Box::from(*b"body"),
        }),
    );
    world.run_for(SECOND);
    let answered = world.answered(1).expect("creation answered");
    assert_eq!((answered.cost, &answered.result), (2, &Ok(ResultAnswer::Created(7))));
    assert!(
        world
            .requests()
            .last()
            .expect("creation request")
            .ends_with(br#"{"title":"title","body":"<!-- kit:key ab -->\nbody"}"#)
    );
    assert!(world.close());
}
