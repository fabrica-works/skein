//! Call envelopes gather response facts before the dialect's plan hears them.
use super::fixtures::{self as f, FORGE, Harness, REPO};
use crate::Answered;
use alloc::boxed::Box;
use skein_forge::api::{Answer, Error, Op, Rate, Read};
use skein_lib::{Duration, Wall};

const CALL: u64 = 1;
// A known branch uses one request; discovery adds one repository lookup.
const ONE_REQUEST: u32 = 1;
const TWO_REQUESTS: u32 = 2;
const FIRST: Rate = Rate { limit: 5000, remaining: 4999, reset_at: Wall::from_nanos(1_800_000_000_000_000_000) };
const SECOND: Rate = Rate { limit: 5000, remaining: 4998, reset_at: Wall::from_nanos(1_800_000_001_000_000_000) };
const FIRST_FIELDS: &[u8] =
    b"X-RateLimit-Limit: 5000\r\nX-RateLimit-Remaining: 4999\r\nX-RateLimit-Reset: 1800000000\r\n";
const SECOND_FIELDS: &[u8] =
    b"X-RateLimit-Limit: 5000\r\nX-RateLimit-Remaining: 4998\r\nX-RateLimit-Reset: 1800000001\r\n";

fn branch() -> Op {
    Op::Read(Read::Branch { branch: Box::from(f::BRANCH) })
}
fn github_known() -> Harness {
    let mut h = f::github_calls();
    f::github_repository(&mut h, REPO, 22);
    h
}
fn begin(h: &mut Harness) {
    h.call(CALL, REPO, branch());
    h.run(Duration::ZERO);
    h.connected(FORGE);
}
fn answer(h: &mut Harness, status: u16, body: &[u8], fields: &[u8]) {
    h.answer(FORGE, &f::response_with_fields(status, body, fields));
    h.run(Duration::ZERO);
}
fn terminal(h: &Harness) -> &Answered {
    h.answered(CALL).expect("call terminal")
}
fn resolved(second_fields: &[u8]) -> Harness {
    let mut h = f::github_calls();
    begin(&mut h);
    answer(&mut h, 200, f::GITHUB_REPOSITORY, FIRST_FIELDS);
    answer(&mut h, 200, f::GITHUB_BRANCH, second_fields);
    h
}

#[test]
fn github_terminal_carries_its_answer_window() {
    let mut h = github_known();
    begin(&mut h);
    answer(&mut h, 200, f::GITHUB_BRANCH, FIRST_FIELDS);
    let got = terminal(&h);
    assert_eq!(
        (got.cost, got.free, got.rate, &got.result),
        (ONE_REQUEST, 0, Some(FIRST), &Ok(Answer::Commit([0; 32])))
    );
}

#[test]
fn headerless_answer_keeps_the_repository_lookup_window() {
    let h = resolved(b"");
    let got = terminal(&h);
    assert_eq!((got.cost, got.rate, &got.result), (TWO_REQUESTS, Some(FIRST), &Ok(Answer::Commit([0; 32]))));
}

#[test]
fn latest_answer_replaces_the_repository_lookup_window() {
    let h = resolved(SECOND_FIELDS);
    let got = terminal(&h);
    assert_eq!((got.cost, got.rate, &got.result), (TWO_REQUESTS, Some(SECOND), &Ok(Answer::Commit([0; 32]))));
}

#[test]
fn partial_rate_headers_keep_the_last_complete_window() {
    let h = resolved(b"X-RateLimit-Remaining: 4998\r\n");
    assert_eq!(terminal(&h).rate, Some(FIRST));
}

#[test]
fn github_without_rate_headers_reports_no_window() {
    let mut h = github_known();
    begin(&mut h);
    answer(&mut h, 200, f::GITHUB_BRANCH, b"");
    assert_eq!((terminal(&h).free, terminal(&h).rate), (0, None));
}

#[test]
fn forgejo_ignores_github_rate_headers() {
    let mut h = f::known();
    begin(&mut h);
    answer(&mut h, 200, f::BRANCH_BODY, FIRST_FIELDS);
    let got = terminal(&h);
    assert_eq!(
        (got.cost, got.free, got.rate, &got.result),
        (ONE_REQUEST, 0, None, &Ok(Answer::Commit(f::BRANCH_HEAD)))
    );
}

#[test]
fn a_304_counts_free_and_cost_before_plan_refusal() {
    for mut h in [github_known(), f::known(), f::github_calls()] {
        begin(&mut h);
        answer(&mut h, 304, b"", b"");
        let got = terminal(&h);
        assert_eq!((got.cost, got.free), (ONE_REQUEST, ONE_REQUEST));
    }
}

#[test]
fn error_terminals_carry_their_response_window() {
    for (status, expected) in [(401, Error::Forbidden), (403, Error::Forbidden), (500, Error::Timeout)] {
        let mut h = github_known();
        begin(&mut h);
        answer(&mut h, status, b"", FIRST_FIELDS);
        let got = terminal(&h);
        assert_eq!((got.cost, got.free, got.rate, &got.result), (ONE_REQUEST, 0, Some(FIRST), &Err(expected)));
    }
}

#[test]
fn rate_limited_terminal_carries_its_exhausted_window() {
    let mut h = github_known();
    begin(&mut h);
    answer(
        &mut h,
        429,
        b"",
        b"Retry-After: 2\r\nX-RateLimit-Limit: 5000\r\nX-RateLimit-Remaining: 0\r\nX-RateLimit-Reset: 1800000000\r\n",
    );
    let got = terminal(&h);
    assert_eq!((got.cost, got.free, got.rate), (ONE_REQUEST, 0, Some(Rate { remaining: 0, ..FIRST })));
    assert_eq!(got.result, Err(Error::RateLimited { after: Duration::from_secs(2) }));
}

#[test]
fn refused_304_keeps_its_free_count_and_window() {
    let mut h = github_known();
    begin(&mut h);
    answer(&mut h, 304, b"", FIRST_FIELDS);
    let got = terminal(&h);
    assert_eq!((got.cost, got.free, got.rate), (ONE_REQUEST, ONE_REQUEST, Some(FIRST)));
    assert_eq!(got.result, Err(Error::InvalidAnswer));
}

#[test]
fn headerless_error_keeps_the_repository_lookup_window() {
    let mut h = f::github_calls();
    begin(&mut h);
    answer(&mut h, 200, f::GITHUB_REPOSITORY, FIRST_FIELDS);
    answer(&mut h, 500, b"", b"");
    let got = terminal(&h);
    assert_eq!((got.cost, got.rate, &got.result), (TWO_REQUESTS, Some(FIRST), &Err(Error::Timeout)));
}

#[test]
fn deadline_terminal_keeps_the_repository_lookup_window() {
    let mut h = f::github_calls();
    begin(&mut h);
    answer(&mut h, 200, f::GITHUB_REPOSITORY, FIRST_FIELDS);
    h.run(h.env.limits.deadline);
    let got = terminal(&h);
    assert_eq!((got.cost, got.free, got.rate, &got.result), (TWO_REQUESTS, 0, Some(FIRST), &Err(Error::Timeout)));
}

#[test]
fn unwritten_call_has_no_response_facts() {
    let mut h = github_known();
    h.component.test_github_mut().rows.get_mut(&1).expect("fixture row").accepted = false;
    h.call(CALL, REPO, branch());
    h.run(Duration::ZERO);
    let got = terminal(&h);
    assert_eq!((got.cost, got.free, got.rate, &got.result), (0, 0, None, &Err(Error::NotInstalled)));
}

#[test]
fn login_lookup_window_survives_a_headerless_permission_answer() {
    let mut h = github_known();
    h.call(CALL, REPO, Op::Read(Read::Permission { user: f::WRITER }));
    h.run(Duration::ZERO);
    h.connected(FORGE);
    answer(&mut h, 200, br#"{"id":2,"login":"writer"}"#, FIRST_FIELDS);
    answer(&mut h, 200, br#"{"role_name":"write"}"#, b"");
    let got = terminal(&h);
    assert_eq!(
        (got.cost, got.rate, &got.result),
        (TWO_REQUESTS, Some(FIRST), &Ok(Answer::Permission(skein_forge::api::Permission::Write)))
    );
}
