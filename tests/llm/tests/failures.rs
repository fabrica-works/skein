//! Provider reset evidence remains distinct from an unknown reset (llm.md, 2.5).
use skein_lib::Duration;
use skein_llm::{Call, Endpoint, Failure, Report, client};
use skein_llm_world::{World, call, limits, response};

type ResetCase = (Call, &'static str, &'static str, Option<Duration>);

fn anthropic_input() -> Call {
    let mut input = call(1);
    input.endpoint = Endpoint::anthropic();
    input.credential.account_id = Box::new([]);
    input
}

fn reset_cases() -> [ResetCase; 4] {
    [
        (call(1), r#"{"error":{"code":"usage_limit_reached","message":"spent"}}"#, "", None),
        (
            call(1),
            r#"{"error":{"code":"usage_limit_reached","message":"spent","resets_in_seconds":90}}"#,
            "",
            Some(Duration::from_secs(90)),
        ),
        (
            anthropic_input(),
            r#"{"error":{"type":"rate_limit_error","message":"spent"}}"#,
            "anthropic-ratelimit-unified-status: rejected\r\n",
            None,
        ),
        (
            anthropic_input(),
            r#"{"error":{"type":"rate_limit_error","message":"spent"}}"#,
            "anthropic-ratelimit-unified-status: rejected\r\nanthropic-ratelimit-unified-reset: 90\r\n",
            Some(Duration::from_secs(90)),
        ),
    ]
}

#[test]
fn exhausted_without_a_reset_carries_none_never_zero() {
    for (input, body, headers, expected) in reset_cases() {
        let provider = input.endpoint.provider;
        let headers = format!("Content-Type: application/json\r\n{headers}");
        let mut world = World::new(input, limits(), response(429, &headers, body.as_bytes(), false), 1);
        world.request(client::Request::Start);
        world.run();
        world.assert_once();
        let failure = world.seen.iter().find_map(|event| {
            if let client::Event::Failed { failure, report, .. } = event { Some((*failure, report)) } else { None }
        });
        assert_eq!(
            failure,
            Some((Failure::Exhausted { retry_after: expected }, &Report::none())),
            "{provider:?} {headers}"
        );
    }
}
