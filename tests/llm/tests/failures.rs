//! Provider reset evidence remains distinct from an unknown reset (llm.md, 2.5).
use skein_lib::Duration;
use skein_llm::{Call, Endpoint, Failure, Report, client};
use skein_llm_world::{World, call, limits, response, run_call};

type ResetCase = (Call, &'static str, &'static str, Option<Duration>, &'static [u8]);

fn anthropic_input() -> Call {
    let mut input = call(1);
    input.endpoint = Endpoint::anthropic();
    input.credential.as_mut().expect("fixture credential").account_id = Box::new([]);
    input
}

fn reset_cases() -> [ResetCase; 4] {
    [
        (call(1), r#"{"error":{"code":"usage_limit_reached","message":"spent"}}"#, "", None, b"usage_limit_reached"),
        (
            call(1),
            r#"{"error":{"code":"usage_limit_reached","message":"spent","resets_in_seconds":90}}"#,
            "",
            Some(Duration::from_secs(90)),
            b"usage_limit_reached",
        ),
        (
            anthropic_input(),
            r#"{"error":{"type":"rate_limit_error","message":"spent"}}"#,
            "anthropic-ratelimit-unified-status: rejected\r\n",
            None,
            b"rate_limit_error",
        ),
        (
            anthropic_input(),
            r#"{"error":{"type":"rate_limit_error","message":"spent"}}"#,
            "anthropic-ratelimit-unified-status: rejected\r\nanthropic-ratelimit-unified-reset: 90\r\n",
            Some(Duration::from_secs(90)),
            b"rate_limit_error",
        ),
    ]
}

#[test]
fn exhausted_without_a_reset_carries_none_never_zero() {
    for (input, body, headers, expected, code) in reset_cases() {
        let provider = input.endpoint.provider;
        let headers = format!("Content-Type: application/json\r\n{headers}");
        let world = run_call(input, limits(), response(429, &headers, body.as_bytes(), false), 1);
        world.assert_once();
        let failure = world.seen.iter().find_map(|event| {
            if let client::Event::Failed { failure, report, .. } = event {
                Some((*failure, report.clone()))
            } else {
                None
            }
        });
        assert_eq!(
            failure,
            Some((
                Failure::Exhausted { retry_after: expected },
                Report { code: code.into(), request: Box::new([]), field: Box::new([]) }
            )),
            "{provider:?} {headers}"
        );
    }
}

const REQUEST_ID: &[u8] = b"fixture-request";
// llm.md 4.2 bounds each of the three report values at 128 bytes.
const REPORT_BOUND: usize = 128;

fn report(world: &World) -> Report {
    world
        .seen
        .iter()
        .find_map(|event| if let client::Event::Failed { report, .. } = event { Some(report.clone()) } else { None })
        .expect("one failed terminal")
}

fn failed(input: Call, headers: &str, body: &[u8]) -> World {
    let world = run_call(input, limits(), response(429, headers, body, false), 1);
    world.assert_once();
    world
}

type RouteReportCase = (Call, &'static str, u16, &'static [u8], &'static [u8], &'static [u8]);

fn route_report_cases() -> [RouteReportCase; 6] {
    [
        (
            call(1),
            "x-oai-request-id",
            429,
            br#"{"error":{"code":"fixture-code","message":"spent","param":"model"}}"#.as_slice(),
            b"fixture-code".as_slice(),
            b"model".as_slice(),
        ),
        (call(1), "x-oai-request-id", 429, b"undecodable", b"".as_slice(), b"".as_slice()),
        (
            anthropic_input(),
            "request-id",
            429,
            br#"{"error":{"type":"fixture-type","message":"spent","param":"ignored"}}"#.as_slice(),
            b"fixture-type".as_slice(),
            b"".as_slice(),
        ),
        (anthropic_input(), "request-id", 429, b"undecodable", b"".as_slice(), b"".as_slice()),
        (
            call(1),
            "x-oai-request-id",
            200,
            b"data: {\"type\":\"error\",\"code\":\"fixture-code\",\"message\":\"spent\",\"param\":\"model\"}\n\n",
            b"fixture-code",
            b"model",
        ),
        (
            anthropic_input(),
            "request-id",
            200,
            b"event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"fixture-type\",\"message\":\"spent\"}}\n\n",
            b"fixture-type",
            b"",
        ),
    ]
}

#[test]
fn each_routes_request_id_reaches_the_report() {
    for (input, name, status, body, code, field) in route_report_cases() {
        let headers = format!("{name}: fixture-request\r\nContent-Type: text/event-stream\r\n");
        let world = run_call(input, limits(), response(status, &headers, body, false), 1);
        assert_eq!(
            report(&world),
            Report { code: code.into(), request: REQUEST_ID.into(), field: field.into() },
            "{name} {body:?}"
        );
    }
}

fn report_bound_cases() -> [(usize, usize, usize, Report); 6] {
    [
        (
            REPORT_BOUND,
            0,
            0,
            Report { code: vec![b'c'; REPORT_BOUND].into(), request: Box::new([]), field: Box::new([]) },
        ),
        (REPORT_BOUND + 1, 0, 0, Report::none()),
        (
            0,
            REPORT_BOUND,
            0,
            Report { code: Box::new([]), request: vec![b'r'; REPORT_BOUND].into(), field: Box::new([]) },
        ),
        (0, REPORT_BOUND + 1, 0, Report::none()),
        (
            0,
            0,
            REPORT_BOUND,
            Report { code: Box::new([]), request: Box::new([]), field: vec![b'f'; REPORT_BOUND].into() },
        ),
        (0, 0, REPORT_BOUND + 1, Report::none()),
    ]
}

#[test]
fn a_report_value_past_its_bound_is_empty() {
    for (code, request, field, expected) in report_bound_cases() {
        let body = format!(
            r#"{{"error":{{"code":"{}","message":"spent","param":"{}"}}}}"#,
            "c".repeat(code),
            "f".repeat(field)
        );
        let headers = format!("x-oai-request-id: {}\r\n", "r".repeat(request));
        let world = failed(call(1), &headers, body.as_bytes());
        assert_eq!(report(&world), expected, "code {code}, request {request}, field {field}");
    }
}

#[test]
fn a_failure_before_any_head_has_an_empty_report() {
    let mut world = World::new(call(1), limits(), Vec::new(), 1);
    world.abort(Failure::Unavailable);
    assert_eq!(report(&world), Report::none());
}

#[test]
fn a_local_failure_after_a_head_keeps_the_request_id() {
    for failure in [
        Failure::Limit { which: skein_llm::Cap::Answer, bound: 1 },
        Failure::TimedOut { phase: skein_llm::Phase::Idle },
        Failure::Protocol,
        Failure::Cancelled,
    ] {
        let source =
            response(200, "x-oai-request-id: fixture-request\r\nContent-Type: text/event-stream\r\n", b"", false);
        let mut world = World::new(call(1), limits(), source, 1);
        world.request(client::Request::Start);
        while !world.machine.response_received() && world.tick(false) {}
        world.abort(failure);
        assert_eq!(
            report(&world),
            Report { code: Box::new([]), request: REQUEST_ID.into(), field: Box::new([]) },
            "{failure:?}"
        );
    }
}
