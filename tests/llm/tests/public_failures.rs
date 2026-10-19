//! Synthetic public Responses failures, independently written from llm.md 4.8.
use skein_lib::{Duration, Wall};
use skein_llm::{Call, Failure, Provider, Report, client};
use skein_llm_world::{World, call, events, limits_for, public_call, response, run_call};

fn assert_failure(world: &World, expected: Failure, code: &str, field: &[u8]) {
    world.assert_once();
    let terminal = world.seen.iter().find_map(|event| {
        if let client::Event::Failed { failure, report, .. } = event { Some((*failure, report.clone())) } else { None }
    });
    assert_eq!(
        terminal,
        Some((expected, Report { code: code.as_bytes().into(), request: Box::new([]), field: field.into() })),
        "{code}"
    );
}

fn http_failure(input: Call, status: u16, headers: &str, code: &str) -> World {
    let body = format!(r#"{{"error":{{"code":"{code}","message":"synthetic failure"}}}}"#);
    let bounds = limits_for(input.endpoint.provider);
    run_call(input, bounds, response(status, headers, body.as_bytes(), false), 1)
}

fn code_cases() -> [(&'static str, u16, Failure); 13] {
    [
        ("subscription_sharing_usage_limit_exceeded", 429, Failure::Exhausted { retry_after: None }),
        ("insufficient_quota", 429, Failure::Exhausted { retry_after: None }),
        ("rate_limit_exceeded", 429, Failure::RateLimited { retry_after: Duration::ZERO }),
        ("subscription_sharing_user_not_eligible", 403, Failure::Refused),
        ("chatpass_v2_scope_not_authorized", 403, Failure::Refused),
        ("chatpass_v2_invalid_authorization_context", 403, Failure::Refused),
        ("subscription_sharing_usage_unavailable", 503, Failure::Unavailable),
        ("subscription_sharing_user_unavailable", 503, Failure::Unavailable),
        ("subscription_sharing_unsupported_capability", 400, Failure::Unsupported),
        ("subscription_sharing_route_not_supported", 403, Failure::Invalid),
        ("subscription_sharing_invalid_user", 401, Failure::Unauthorized),
        ("invalid_api_key", 401, Failure::Unauthorized),
        ("context_length_exceeded", 400, Failure::ContextTooLong),
    ]
}

#[test]
fn each_code_of_the_table_gives_its_class() {
    for (code, status, expected) in code_cases() {
        for input in [public_call(1), call(1)] {
            let world = http_failure(input, status, "", code);
            assert_failure(&world, expected, code, b"");
        }
    }
}

#[test]
fn a_code_the_table_names_beats_its_status() {
    for (code, status, expected) in [
        ("insufficient_quota", 429, Failure::Exhausted { retry_after: None }),
        ("subscription_sharing_user_not_eligible", 400, Failure::Refused),
        ("subscription_sharing_usage_unavailable", 503, Failure::Unavailable),
        ("subscription_sharing_route_not_supported", 403, Failure::Invalid),
    ] {
        let world = http_failure(public_call(1), status, "retry-after: 7\r\n", code);
        assert_failure(&world, expected, code, b"");
    }
}

fn stream_failure(input: Call, code: &str) -> World {
    let document = format!(
        r#"{{"type":"response.failed","response":{{"error":{{"code":"{code}","message":"synthetic failure"}}}}}}"#
    );
    let source = response(200, "Content-Type: text/event-stream\r\n", &events(&[&document]), false);
    let bounds = limits_for(input.endpoint.provider);
    run_call(input, bounds, source, 1)
}

#[test]
fn the_plans_limit_inside_response_failed_is_exhausted() {
    let code = "subscription_sharing_usage_limit_exceeded";
    for input in [public_call(1), call(1)] {
        let world = stream_failure(input, code);
        assert_failure(&world, Failure::Exhausted { retry_after: None }, code, b"");
    }
}

#[test]
fn the_plans_reset_comes_from_the_error_then_retry_after() {
    let code = "subscription_sharing_usage_limit_exceeded";
    for (fields, headers, expected) in [
        (",\"resets_in_seconds\":11,\"resets_at\":120", "retry-after: 7\r\n", Some(Duration::from_secs(11))),
        (",\"resets_at\":120", "retry-after: 7\r\n", Some(Duration::from_secs(20))),
        ("", "retry-after: 7\r\n", Some(Duration::from_secs(7))),
        ("", "", None),
    ] {
        let body = format!(r#"{{"error":{{"code":"{code}","message":"synthetic failure"{fields}}}}}"#);
        let source = response(429, headers, body.as_bytes(), false);
        let mut world = World::new(public_call(1), limits_for(Provider::OpenAi), source, 1);
        world.env.wall = Wall::from_nanos(100_000_000_000);
        world.request(client::Request::Start);
        world.run();
        assert_failure(&world, Failure::Exhausted { retry_after: expected }, code, b"");
    }
}

#[test]
fn an_unsupported_capability_names_its_field() {
    let code = "subscription_sharing_unsupported_capability";
    let body = format!(r#"{{"error":{{"code":"{code}","message":"synthetic failure","param":"temperature"}}}}"#);
    let source = response(400, "", body.as_bytes(), false);
    let world = run_call(public_call(1), limits_for(Provider::OpenAi), source, 1);
    assert_failure(&world, Failure::Unsupported, code, b"temperature");
}

#[test]
fn overloaded_is_a_529_or_a_5xx_overloaded_error() {
    for (status, code, expected) in [
        (529, "synthetic_error", Failure::Overloaded),
        (503, "overloaded_error", Failure::Overloaded),
        (503, "synthetic_error", Failure::Unavailable),
    ] {
        let world = http_failure(public_call(1), status, "", code);
        assert_failure(&world, expected, code, b"");
    }
}

#[test]
fn a_mid_stream_code_outside_the_table_is_unavailable() {
    for input in [public_call(1), call(1)] {
        let world = stream_failure(input, "synthetic_error");
        assert_failure(&world, Failure::Unavailable, "synthetic_error", b"");
    }
}
