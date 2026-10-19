//! Synthetic provider errors exercise llm.md 4.6–4.7's code-first classes.
use skein_lib::{Duration, Wall};
use skein_llm::{Call, Failure, Report, client};
use skein_llm_world::{World, anthropic_call, call, limits_for, response};

fn failed(input: Call, status: u16, headers: &str, body: &[u8]) -> (Failure, Report) {
    let bounds = limits_for(input.endpoint.provider);
    let mut world = World::new(input, bounds, response(status, headers, body, false), 1);
    world.env.wall = Wall::from_nanos(100_000_000_000);
    world.request(client::Request::Start);
    world.run();
    world.assert_once();
    world
        .seen
        .into_iter()
        .find_map(|event| {
            if let client::Event::Failed { failure, report, .. } = event { Some((failure, report)) } else { None }
        })
        .expect("provider failure")
}

fn expected(failure: Failure, code: &str) -> (Failure, Report) {
    (failure, Report { code: code.as_bytes().into(), request: Box::new([]), field: Box::new([]) })
}

#[test]
fn anthropic_failures_are_classed_by_type_first() {
    let rejected = "anthropic-ratelimit-unified-status: rejected\r\nRetry-After: 9\r\n";
    let reset = "anthropic-ratelimit-unified-status: rejected\r\nanthropic-ratelimit-unified-reset: 160\r\n";
    for (kind, status, message, headers, class) in [
        ("permission_error", 403, "denied", "", Failure::Refused),
        ("billing_error", 402, "spent", "Retry-After: 9\r\n", Failure::Exhausted { retry_after: None }),
        ("rate_limit_error", 429, "spent", reset, Failure::Exhausted { retry_after: Some(Duration::from_secs(60)) }),
        ("rate_limit_error", 429, "spent", rejected, Failure::Exhausted { retry_after: None }),
        (
            "rate_limit_error",
            429,
            "slow",
            "Retry-After: 9\r\n",
            Failure::RateLimited { retry_after: Duration::from_secs(9) },
        ),
        ("overloaded_error", 529, "busy", "", Failure::Overloaded),
        ("api_error", 500, "unavailable", "", Failure::Unavailable),
        ("invalid_request_error", 400, "prompt is too long", "", Failure::ContextTooLong),
        ("request_too_large", 413, "large", "", Failure::ContextTooLong),
        ("invalid_request_error", 400, "balance is spent", "", Failure::Invalid),
        ("authentication_error", 401, "expired", "", Failure::Unauthorized),
        ("api_error", 400, "unavailable", "", Failure::Unavailable),
        ("permission_error", 429, "denied", "", Failure::Refused),
    ] {
        let body = format!(r#"{{"error":{{"type":"{kind}","message":"{message}","param":"ignored"}}}}"#);
        assert_eq!(
            failed(anthropic_call(1), status, headers, body.as_bytes()),
            expected(class, kind),
            "{kind} {status}"
        );
    }
}

#[test]
fn codex_failures_are_classed_by_code_first() {
    for (code, extra, headers, class) in [
        (
            "usage_limit_reached",
            r#", "resets_in_seconds":60, "resets_at":190"#,
            "Retry-After: 9\r\n",
            Failure::Exhausted { retry_after: Some(Duration::from_secs(60)) },
        ),
        (
            "usage_limit_reached",
            r#", "resets_at":160"#,
            "Retry-After: 9\r\n",
            Failure::Exhausted { retry_after: Some(Duration::from_secs(60)) },
        ),
        ("usage_limit_reached", "", "Retry-After: 9\r\n", Failure::Exhausted { retry_after: None }),
        ("rate_limit_exceeded", "", "Retry-After: 9\r\n", Failure::RateLimited { retry_after: Duration::from_secs(9) }),
    ] {
        let body = format!(r#"{{"error":{{"code":"{code}","message":"synthetic"{extra}}}}}"#);
        assert_eq!(failed(call(1), 429, headers, body.as_bytes()), expected(class, code), "{code} {extra}");
    }
}

#[test]
fn a_mid_stream_code_keeps_its_class_or_is_unavailable() {
    for (input, document, code, class) in [
        (
            call(1),
            r#"{"type":"response.failed","response":{"error":{"code":"usage_limit_reached","message":"synthetic"}}}"#,
            "usage_limit_reached",
            Failure::Exhausted { retry_after: None },
        ),
        (
            anthropic_call(1),
            r#"{"type":"error","error":{"type":"overloaded_error","message":"synthetic"}}"#,
            "overloaded_error",
            Failure::Overloaded,
        ),
        (
            call(1),
            r#"{"type":"response.failed","response":{"error":{"code":"synthetic_unknown","message":"synthetic"}}}"#,
            "synthetic_unknown",
            Failure::Unavailable,
        ),
        (
            anthropic_call(1),
            r#"{"type":"error","error":{"type":"synthetic_unknown","message":"synthetic"}}"#,
            "synthetic_unknown",
            Failure::Unavailable,
        ),
    ] {
        let body = format!("event: error\ndata: {document}\n\n");
        assert_eq!(
            failed(input, 200, "Content-Type: text/event-stream\r\n", body.as_bytes()),
            expected(class, code),
            "{code}"
        );
    }
}
