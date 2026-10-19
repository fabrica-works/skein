use super::fixtures::{self, Harness, boxed};
use crate::Route;
use skein_fake_forge_domain::api::{self as a, Op, Read, Write};
use skein_lib::{Time, bytes};
#[test]
fn an_unknown_token_is_answered_401_and_counted() {
    let mut h = Harness::new(fixtures::config());
    h.request(b"GET /api/v1/user HTTP/1.1\r\nHost: 127.0.0.1:3000\r\nAuthorization: token 0000000000000000000000000000000000000000\r\n\r\n");
    h.run();
    assert_eq!(h.status(), 401);
    assert_eq!(h.service().tally().unauthorized, 1);
    assert_eq!(h.service().tally().total, 1);
    assert_eq!(h.domain_calls(), 0);
}
#[test]
fn the_issue_listing_pages_leastupdate_capped_at_max_response_items() {
    let mut h = Harness::new(fixtures::config());
    for title in [b"one".as_slice(), b"two", b"three"] {
        h.issue(title);
    }
    h.ask(b"GET", b"/api/v1/repos/org/repo/issues?sort=leastupdate&page=1&limit=99", b"");
    assert_eq!(h.status(), 200);
    let body = h.body();
    assert!(bytes::find(&body, b"\"title\":\"one\"") < bytes::find(&body, b"\"title\":\"two\""));
    assert_eq!(bytes::find(&body, b"\"title\":\"three\""), None);
    assert_eq!(h.domain_calls(), 1);
}
#[test]
fn a_limit_without_a_page_takes_the_default_paging_num() {
    let mut h = Harness::new(fixtures::config());
    for title in [b"one".as_slice(), b"two", b"three"] {
        h.issue(title);
    }
    h.ask(b"GET", b"/api/v1/repos/org/repo/issues?limit=1", b"");
    assert_eq!(h.status(), 200);
    let body = h.body();
    assert!(bytes::find(&body, b"\"title\":\"three\"").is_some());
    assert!(bytes::find(&body, b"\"title\":\"two\"").is_some());
    assert_eq!(bytes::find(&body, b"\"title\":\"one\""), None);
}
#[test]
fn a_merge_answers_200_with_no_body() {
    let mut h = Harness::new(fixtures::config());
    let commit = h.push(b"work");
    let a::Answer::Created(number) = h
        .call(
            2,
            Op::Write(Write::OpenPull {
                title: boxed(b"pull"),
                body: boxed(b"body"),
                head: boxed(b"work"),
                base: boxed(b"main"),
                opens_empty: false,
            }),
        )
        .expect("pull")
    else {
        panic!("created")
    };
    assert_eq!(number, 1);
    assert_eq!(commit, 2);
    h.ask(
        b"POST",
        b"/api/v1/repos/org/repo/pulls/1/merge",
        b"{\"Do\":\"squash\",\"head_commit_id\":\"0000000000000002000000000000000000000000\"}",
    );
    assert_eq!(h.status(), 200);
    assert!(h.body().is_empty());
    assert!(bytes::find(&h.head(), b"Content-Length: 0").is_some());
    assert_eq!(h.domain_calls(), 1);
}
#[test]
fn requests_are_counted_by_route_user_and_total() {
    let mut h = Harness::new(fixtures::config());
    h.ask(b"GET", b"/api/v1/user", b"");
    h.ask(b"GET", b"/api/v1/repos/org/repo/issues", b"");
    h.ask(b"POST", b"/api/v1/repos/org/repo/issues", b"{\"title\":\"created\",\"body\":\"body\"}");
    assert_eq!(h.status(), 201);
    let tally = h.service().tally();
    assert_eq!(tally.total, 3);
    assert_eq!(tally.users.get(&1), Some(&3));
    for (route, count) in [(Route::User, 1), (Route::Issues, 1), (Route::CreateIssue, 1)] {
        assert_eq!(tally.routes.get(&route), Some(&count), "{route:?}");
    }
    assert_eq!(h.domain_calls(), 3, "listing, write and readback");
    assert_eq!(tally.unrouted, 0);
    assert!(bytes::find(&h.body(), b"\"title\":\"created\"").is_some());
}
#[test]
fn a_reset_closes_before_reading_and_a_timeout_drops_or_answers_500() {
    let mut config = fixtures::config();
    config.faults.reset = 1000;
    let mut h = Harness::new(config);
    h.request(b"GET /api/v1/user HTTP/1.1\r\nHost: localhost\r\n\r\n");
    h.run();
    assert!(h.closed);
    assert_eq!(h.service().tally().total, 0);
    assert_eq!(h.domain_calls(), 0);
    for (drop, server_error, closed, status) in [(1000, 0, true, None), (0, 1000, false, Some(500))] {
        let mut config = fixtures::config();
        config.faults.drop = drop;
        config.faults.server_error = server_error;
        let mut h = Harness::new(config);
        fault_write(&mut h, closed, status, "drawn post-write fault");
    }
    for (seed, closed, status) in [(0, true, None), (2, false, Some(500))] {
        let mut h = Harness::new(fixtures::config());
        h.service.rng = skein_lib::Rng::new(seed);
        h.fake_env.limits.timeouts = 1000;
        fault_write(&mut h, closed, status, "domain timeout");
    }
}
fn fault_write(h: &mut Harness, closed: bool, status: Option<u16>, row: &str) {
    h.ask(b"POST", b"/api/v1/repos/org/repo/issues", b"{\"title\":\"created\",\"body\":\"body\"}");
    assert_eq!(h.closed, closed, "{row}");
    assert_eq!(h.domain_calls(), 1, "{row}: no read after fault");
    assert_eq!(h.service().tally().total, 1, "{row}: HTTP entrance once");
    if let Some(status) = status {
        assert_eq!(h.status(), status, "{row}");
    }
    h.fake_env.limits.timeouts = 0;
    let Ok(a::Answer::Item { item, .. }) = h.call(2, Op::Read(Read::Item { number: 1, after: 0 })) else {
        panic!("{row}: write happened")
    };
    assert_eq!(item.number, 1, "{row}");
}
#[test]
fn deleting_a_missing_branch_answers_500_with_an_empty_body() {
    let mut h = Harness::new(fixtures::config());
    h.ask(b"DELETE", b"/api/v1/repos/org/repo/branches/missing", b"");
    assert_eq!(h.status(), 500);
    assert!(h.body().is_empty());
}
#[test]
fn a_pull_with_nothing_to_merge_is_opened_201() {
    let mut h = Harness::new(fixtures::config());
    h.branch(b"same", 1);
    h.ask(
        b"POST",
        b"/api/v1/repos/org/repo/pulls",
        b"{\"title\":\"empty\",\"body\":\"body\",\"head\":\"same\",\"base\":\"main\"}",
    );
    assert_eq!(h.status(), 201);
    assert!(bytes::find(&h.body(), b"\"number\":1").is_some());
    assert_eq!(h.domain_calls(), 2, "write then readback");
    let Ok(a::Answer::Pull(pull)) = h.call(2, Op::Read(Read::Pull { number: 1 })) else { panic!("pull readback") };
    assert_eq!(pull.number, 1);
}
#[test]
fn a_ci_status_has_no_creator_and_a_link_relative_to_the_origin() {
    let mut h = Harness::new(fixtures::config());
    assert_eq!(h.push(b"work"), 2);
    h.ask(b"GET", b"/api/v1/repos/org/repo/commits/0000000000000002000000000000000000000000/status", b"");
    assert_eq!(h.status(), 200);
    let body = h.body();
    assert!(bytes::find(&body, b"\"creator\":null").is_some());
    assert!(bytes::find(&body, b"\"target_url\":\"/org/repo/actions/runs/1/jobs/0\"").is_some());
    assert!(bytes::find(&body, b"\"status\":\"failure\"").is_some());
    assert_eq!(h.domain_calls(), 1);
    assert!(h.fake_env.now > Time::ZERO);
}
#[test]
fn a_malformed_write_is_422_without_a_domain_call() {
    let mut h = Harness::new(fixtures::config());
    h.ask(b"POST", b"/api/v1/repos/org/repo/issues", b"{\"title\":3}");
    assert_eq!(h.status(), 422);
    assert_eq!(h.domain_calls(), 0);
    assert_eq!(h.service().tally().unrouted, 1);
}
#[test]
fn a_rate_refusal_uses_the_forges_clock_for_retry_after() {
    let mut h = Harness::new(fixtures::config());
    h.fake_env.limits.rate_limit = 1;
    h.ask(b"GET", b"/api/v1/repos/org/repo/issues", b"");
    h.ask(b"GET", b"/api/v1/repos/org/repo/issues", b"");
    assert_eq!(h.status(), 429);
    assert!(bytes::find(&h.head(), b"retry-after: 10\r\n").is_some());
}
#[test]
fn ci_statuses_keep_a_null_creator_before_and_after_a_success() {
    let request = skein_forge::forgejo::request::Request::CombinedStatus {
        at: fixtures::at(),
        commit: crate::routes::object(2),
        page: skein_forge::forgejo::request::Page { page: None, limit: None },
    };
    for (state, expected) in
        [(a::Check::Pending, b"\"status\":\"pending\"".as_slice()), (a::Check::Passed, b"\"status\":\"success\"")]
    {
        let answer = a::Answer::Checks(Box::new([a::CheckSummary {
            status: a::Status { context: boxed(b"build"), state, author: 3, at: Time::ZERO },
            description: boxed(b"CI"),
            link: boxed(b"/org/repo/actions/runs/1/jobs/0"),
            job: None,
        }]));
        let reply = crate::response::answer(
            &fixtures::config(),
            &fixtures::limits(),
            &request,
            &crate::response::Select::Document,
            &answer,
        )
        .expect("status");
        assert!(bytes::find(&reply.body, b"\"creator\":null").is_some(), "{state:?}");
        assert!(bytes::find(&reply.body, expected).is_some(), "{state:?}");
    }
}
#[test]
fn an_unpaged_thread_past_the_projection_budget_is_refused() {
    let answer = a::Answer::Comments(Box::new([a::Comment {
        id: 1,
        author: 2,
        body: Box::new([b'x'; 2048]),
        created: Time::ZERO,
        edited: None,
    }]));
    let request = skein_forge::forgejo::request::Request::Comments { at: fixtures::at(), number: 1 };
    let mut limits = fixtures::limits();
    limits.documents.writer.length = 1024;
    assert_eq!(
        crate::response::answer(&fixtures::config(), &limits, &request, &crate::response::Select::Document, &answer),
        Err(a::Error::TooLarge)
    );
}

#[test]
fn a_failed_jobs_steps_and_failed_steps_log_are_served_over_http() {
    let mut h = Harness::new(fixtures::config());
    assert_eq!(h.push(b"work"), 2);
    h.ask(b"GET", b"/api/v1/repos/org/repo/actions/jobs/1", b"");
    assert_eq!(h.status(), 200);
    let job = h.body();
    for step in [
        b"{\"number\":0,\"name\":\"Set up job\",\"status\":\"success\"}".as_slice(),
        b"{\"number\":1,\"name\":\"Run build\",\"status\":\"failure\"}",
        b"{\"number\":2,\"name\":\"Complete job\",\"status\":\"failure\"}",
    ] {
        assert!(bytes::find(&job, step).is_some(), "{step:?}");
    }
    h.ask(b"GET", b"/api/v1/repos/org/repo/actions/jobs/1/logs?attempt=1&step=1", b"");
    assert_eq!(h.status(), 200);
    assert_eq!(
        &*h.body(),
        b"failing check build 0\nfailing check build 1\nfailing check build 2\nfailing check build 3\n"
    );
    assert_eq!(h.domain_calls(), 2);
}

#[test]
fn a_steps_log_end_is_served_by_a_suffix_range() {
    let mut h = Harness::new(fixtures::config());
    assert_eq!(h.push(b"work"), 2);
    h.request(b"GET /api/v1/repos/org/repo/actions/jobs/1/logs?attempt=1&step=1 HTTP/1.1\r\nHost: 127.0.0.1:3000\r\nAuthorization: token 1111111111111111111111111111111111111111\r\nRange: bytes=-10\r\n\r\n");
    h.run();
    assert_eq!(h.status(), 206);
    assert!(bytes::find(&h.head(), b"content-range: bytes 78-87/88\r\n").is_some());
    assert_eq!(&*h.body(), b"k build 3\n");
}
