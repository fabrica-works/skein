//! GitHub start transcripts; the only credentials here are artificial fixture values.
use super::fixtures::{self, FORGE, Harness};
use crate::{Event, Request, StartRead, Stopped, start::Start};
use alloc::boxed::Box;
use skein_lib::{Duration, Time, Wall};

const SECOND: Duration = Duration::from_secs(1);
pub(super) const LIVE: &[u8] = br#"[{"id":1,"account":{"id":22,"login":"acme","type":"Organization"},"created_at":"1970-01-01T00:00:01Z","suspended_at":null}]"#;
pub(super) const SUSPENDED: &[u8] = br#"[{"id":1,"account":{"id":22,"login":"acme","type":"Organization"},"created_at":"1970-01-01T00:00:01Z","suspended_at":"1970-01-01T00:00:02Z"}]"#;
const PAGE_ONE: &[u8] = br#"[{"id":3,"account":{"id":24,"login":"other","type":"User"},"created_at":"1970-01-01T00:00:01Z","suspended_at":"1970-01-01T00:00:02Z"},{"id":1,"account":{"id":22,"login":"acme","type":"Organization"},"created_at":"1970-01-01T00:00:01Z","suspended_at":null}]"#;
const PAGE_TWO: &[u8] = br#"[{"id":2,"account":{"id":23,"login":"stray","type":"User"},"created_at":"1970-01-01T00:00:01Z","suspended_at":null}]"#;
pub(super) const FULL: &[u8] = b"artificial-full-api-secret";
pub(super) const CONTENTS: &[u8] = b"artificial-contents-git-secret";
pub(super) const EXPIRES: &[u8] = b"1970-01-01T01:00:00Z";
pub(super) const EXPIRY: Wall = Wall::from_nanos(3_600_000_000_000);
pub(super) const VIEWER: &[u8] = br#"{"data":{"viewer":{"databaseId":9,"login":"test-app[bot]"}}}"#;
pub(super) const NEXT_PAGE: &[u8] =
    b"Link: <https://api.github.test/app/installations?per_page=100&page=2>; rel=\"next\"\r\n";
// Default token_bytes is 64; its name bound is 128. Exercise them independently.
const TOKEN_BOUND: usize = 64;
// Below the 20-byte timestamp, to prove the mint decoder reserves its room independently.
const SMALL_TOKEN_BOUND: u32 = 8;
// Two listing pages, two mint scopes, then Viewer.
const START_REQUESTS: u32 = 5;

#[derive(Clone, Copy, Debug)]
enum Stage {
    Listing,
    Full,
    Contents,
    Identity,
}
pub(super) fn accepted(h: &mut Harness, ids: &[u64]) {
    h.send(Request::Installations { forge: FORGE, accepted: Box::from(ids) });
}
fn harness() -> Harness {
    let mut h = Harness::new(fixtures::github_settings());
    accepted(&mut h, &[1]);
    h.connected(FORGE);
    h
}
pub(super) fn mint(token: &[u8], expires: &[u8]) -> Box<[u8]> {
    let mut body = skein_lib::Writer::new(
        token.len().checked_add(expires.len()).expect("fixture fields").checked_add(28).expect("JSON framing"),
    );
    body.put(b"{\"token\":\"").expect("measured token");
    body.put(token).expect("measured token");
    body.put(b"\",\"expires_at\":\"").expect("measured expiry");
    body.put(expires).expect("measured expiry");
    body.put(b"\"}").expect("measured mint");
    fixtures::response(201, &body.finish())
}
fn listed(h: &mut Harness) {
    h.answer(FORGE, &fixtures::ok(LIVE));
}
pub(super) fn full(h: &mut Harness) {
    h.answer(FORGE, &mint(FULL, EXPIRES));
}
pub(super) fn contents(h: &mut Harness) {
    h.answer(FORGE, &mint(CONTENTS, EXPIRES));
}
fn at(stage: Stage) -> Harness {
    let mut h = harness();
    match stage {
        Stage::Listing => {}
        Stage::Full => listed(&mut h),
        Stage::Contents => {
            listed(&mut h);
            full(&mut h);
        }
        Stage::Identity => {
            listed(&mut h);
            full(&mut h);
            contents(&mut h);
        }
    }
    h
}
pub(super) fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    for piece in bytes.windows(needle.len()) {
        if piece == needle {
            return true;
        }
    }
    false
}
pub(super) fn tokens(h: &Harness) -> &[Event] {
    h.git_tokens.as_ref().expect("GitHub harness").as_slice()
}
fn waiting(h: &Harness, until: u64, next: u64) {
    assert_eq!(
        h.start(FORGE),
        Start::Waiting {
            until: Time::ZERO.saturating_add(Duration::from_secs(until)),
            next: Duration::from_secs(next)
        }
    );
}

fn request_content_length(head: &[u8]) -> usize {
    const PREFIX: &[u8] = b"\r\nContent-Length: ";
    let Some(at) = skein_lib::bytes::find(head, PREFIX) else {
        return 0;
    };
    let rest = head.get(at.checked_add(PREFIX.len()).expect("header start")..).expect("header value");
    let end = skein_lib::bytes::find(rest, b"\r\n").expect("header end");
    let mut length = 0_usize;
    for byte in rest.get(..end).expect("length digits") {
        length = length
            .checked_mul(10)
            .expect("fixture length")
            .checked_add(usize::from(byte.checked_sub(b'0').expect("decimal digit")))
            .expect("fixture length");
    }
    length
}

#[test]
fn github_starts_list_mint_both_scopes_then_check_identity() {
    let mut h = harness();
    accepted(&mut h, &[1, 3]);
    h.answer(FORGE, &fixtures::ok(b"[]")); // Finish the prior generation's written page.
    h.answer(FORGE, &fixtures::response_with_fields(200, PAGE_ONE, NEXT_PAGE));
    h.answer(FORGE, &fixtures::ok(PAGE_TWO));
    full(&mut h);
    contents(&mut h);
    h.answer(FORGE, &fixtures::ok(VIEWER));
    let wire = h.request_bytes(FORGE);
    let head_end =
        skein_lib::bytes::find(&wire, b"\r\n\r\n").expect("discarded read head").checked_add(4).expect("head framing");
    let mut rest = wire.get(head_end..).expect("new generation's requests");
    let mut requests = skein_lib::List::with_capacity(START_REQUESTS);
    let mut lines = skein_lib::List::with_capacity(START_REQUESTS);
    for _ in 0..START_REQUESTS {
        let head_end = skein_lib::bytes::find(rest, b"\r\n\r\n")
            .expect("complete request head")
            .checked_add(4)
            .expect("head framing");
        let head = rest.get(..head_end).expect("head");
        let line_end = skein_lib::bytes::find(head, b"\r\n").expect("request line");
        let length = request_content_length(head);
        let body_end = head_end.checked_add(length).expect("request length");
        let body = rest.get(head_end..body_end).expect("complete request body");
        let line = head.get(..line_end).expect("request line");
        lines.push(line).expect("five request lines");
        requests.push((line, head, body)).expect("five requests");
        rest = rest.get(body_end..).expect("next request");
    }
    assert!(rest.is_empty(), "no further request");
    assert_eq!(
        lines.as_slice(),
        [
            b"GET /app/installations?per_page=100&page=1 HTTP/1.1".as_slice(),
            b"GET /app/installations?per_page=100&page=2 HTTP/1.1",
            b"POST /app/installations/1/access_tokens HTTP/1.1",
            b"POST /app/installations/1/access_tokens HTTP/1.1",
            b"POST /graphql HTTP/1.1"
        ]
    );
    let (_, full_head, full_body) = requests.get(2).expect("Full");
    assert!(full_body.is_empty(), "Full has no body");
    assert!(!contains(full_head, b"Content-Type:"));
    let (_, _, contents_body) = requests.get(3).expect("Contents");
    assert_eq!(*contents_body, b"{\"permissions\":{\"contents\":\"write\"}}");
    let (_, viewer_head, _) = requests.get(4).expect("Viewer");
    let authorization =
        skein_forge::github::credential::bearer(&skein_lib::Secret::new(Box::from(FULL))).expect("Full bearer");
    assert!(fixtures::header(viewer_head, b"Authorization: ") == authorization.expose(), "Viewer uses Full");
    h.run(Duration::ZERO);
    assert_eq!(h.start(FORGE), Start::Ready { cap: 100 });
    assert_eq!(h.connected_count, 1);
    assert_eq!(tokens(&h).len(), 1);
    match tokens(&h).first().expect("git token") {
        Event::GitToken { forge, installation, account, token, expires } => {
            assert_eq!(
                (*forge, *installation, account.id, account.login.as_ref(), *expires),
                (FORGE, 1, 22, b"acme".as_slice(), EXPIRY)
            );
            assert!(token.expose() == CONTENTS, "only Contents goes up");
        }
        Event::Installation { .. }
        | Event::Fact { .. }
        | Event::Answered(_)
        | Event::Hint(_)
        | Event::Listening { .. }
        | Event::ListenerLost
        | Event::Connected { .. }
        | Event::Stopped { .. }
        | Event::Closed => unreachable!("token collection"),
    }
}

#[test]
fn github_with_no_accepted_installation_is_ready_without_identity() {
    let mut h = harness();
    accepted(&mut h, &[]);
    h.answer(FORGE, &fixtures::ok(b"[]")); // The changed set discards the written page.
    listed(&mut h);
    h.run(Duration::ZERO);
    assert_eq!(h.start(FORGE), Start::Ready { cap: 100 });
    assert_eq!(h.connected_count, 1);
    assert!(tokens(&h).is_empty());
    assert!(!contains(&h.request_bytes(FORGE), b"POST "));
}
#[test]
fn github_listing_401_stops_with_key_refused() {
    let mut h = harness();
    h.answer(FORGE, &fixtures::status(401));
    h.run(Duration::ZERO);
    assert_eq!(h.stopped(), Some((FORGE, Stopped::KeyRefused)));
    h.run(Duration::from_secs(20));
    assert_eq!(h.connections_opened(), 1);
    assert_eq!(h.connected_count, 0);
}
#[test]
fn github_either_mint_401_stops_with_key_refused() {
    for stage in [Stage::Full, Stage::Contents] {
        let mut h = at(stage);
        h.answer(FORGE, &fixtures::status(401));
        h.run(Duration::ZERO);
        assert_eq!(h.stopped(), Some((FORGE, Stopped::KeyRefused)), "{stage:?}");
        assert!(tokens(&h).is_empty());
    }
}
#[test]
fn github_mint_403_or_404_reads_installations_again() {
    mint_reread(403);
}
#[test]
fn github_mint_404_reads_installations_again() {
    mint_reread(404);
}
fn mint_reread(status: u16) {
    let mut h = at(Stage::Full);
    h.answer(FORGE, &fixtures::status(status));
    waiting(&h, 1, 2);
    h.run(Duration::from_millis(999));
    assert_eq!(h.connections_opened(), 1);
    h.run(Duration::from_millis(1));
    assert_eq!(h.start(FORGE), Start::Listing { next: Duration::from_secs(2) });
    h.connected(FORGE);
    listed(&mut h);
    assert!(contains(&h.request_bytes(FORGE), b"POST /app/installations/1/access_tokens"));
    assert_eq!(h.start(FORGE), Start::Minting { next: Duration::from_secs(2) });
}
#[test]
fn github_mint_refusal_fresh_table_removes_or_suspends_the_row() {
    fresh_table(404, b"[]");
}
#[test]
fn github_mint_refusal_fresh_table_suspends_the_row() {
    fresh_table(403, SUSPENDED);
}
fn fresh_table(status: u16, table: &[u8]) {
    let mut h = at(Stage::Contents);
    h.answer(FORGE, &fixtures::status(status));
    h.run(SECOND);
    h.connected(FORGE);
    h.answer(FORGE, &fixtures::ok(table));
    h.run(Duration::ZERO);
    assert_eq!(h.start(FORGE), Start::Ready { cap: 100 });
    assert!(!contains(&h.request_bytes(FORGE), b"POST "));
    assert!(tokens(&h).is_empty());
}
#[test]
fn github_accepted_ids_past_the_bound_are_ignored() {
    let limits = crate::Limits { installations: 2, ..fixtures::limits() };
    let mut h = Harness::with(limits, fixtures::shared(), fixtures::github_settings());
    accepted(&mut h, &[0, 1, 1, 2, 3]);
    h.connected(FORGE);
    // Row 1 is absent: 2 remains accepted, and 3 was past the accepted-set bound.
    h.answer(FORGE, &fixtures::response_with_fields(200, PAGE_TWO, NEXT_PAGE));
    let live_third = br#"[{"id":3,"account":{"id":24,"login":"other","type":"User"},"created_at":"1970-01-01T00:00:01Z","suspended_at":null}]"#;
    h.answer(FORGE, &fixtures::ok(live_third));
    full(&mut h);
    contents(&mut h);
    h.answer(FORGE, &fixtures::ok(VIEWER));
    assert!(!contains(&h.request_bytes(FORGE), b"/app/installations/3/access_tokens"));
    h.run(Duration::ZERO);
    assert_eq!(tokens(&h).len(), 1);
    assert_eq!(h.start(FORGE), Start::Ready { cap: 100 });
}
#[test]
fn github_nontransient_start_refusals_name_the_read_or_installation() {
    refused_read(Stage::Listing, StartRead::Installations, &[403, 422]);
}
#[test]
fn github_nontransient_viewer_refusals_name_the_read() {
    refused_read(Stage::Identity, StartRead::Viewer, &[401, 422]);
}
fn refused_read(stage: Stage, read: StartRead, statuses: &[u16]) {
    for status in statuses {
        let mut h = at(stage);
        h.answer(FORGE, &fixtures::status(*status));
        h.run(Duration::ZERO);
        assert_eq!(h.stopped(), Some((FORGE, Stopped::Refused { read, status: *status })), "{stage:?}/{status}");
    }
}

#[test]
fn github_nontransient_mint_refusals_name_the_installation() {
    for (stage, status) in [(Stage::Full, 400), (Stage::Contents, 422)] {
        let mut h = at(stage);
        h.answer(FORGE, &fixtures::status(status));
        h.run(Duration::ZERO);
        assert_eq!(h.stopped(), Some((FORGE, Stopped::TokenRefused { installation: 1, status })), "{stage:?}");
    }
}
#[test]
fn github_other_bot_user_stops_the_forge() {
    let mut h = at(Stage::Identity);
    h.answer(FORGE, &fixtures::ok(br#"{"data":{"viewer":{"databaseId":8,"login":"other"}}}"#));
    h.run(Duration::ZERO);
    assert_eq!(h.stopped(), Some((FORGE, Stopped::OtherUser { user: 8 })));
    assert_eq!(h.connected_count, 0);
}
const TRANSPORT_OR_RATE: &[(&str, u16, &[u8])] = &[("transport", 0, b""), ("429", 429, b"{}")];
const SERVER_OR_MALFORMED: &[(&str, u16, &[u8])] = &[("5xx", 503, b"{}"), ("malformed", 200, b"{")];
const MINT_SERVER_OR_MALFORMED: &[(&str, u16, &[u8])] = &[("5xx", 503, b"{}"), ("malformed", 201, b"{")];
fn transient(stage: Stage, cases: &[(&str, u16, &[u8])]) {
    for &(label, status, body) in cases {
        let mut h = at(stage);
        if status == 0 {
            h.reset(FORGE);
        } else {
            h.answer(FORGE, &fixtures::response(status, body));
        }
        waiting(&h, 1, 2);
        assert_eq!(h.stopped(), None, "{stage:?}/{label}");
        h.run(SECOND);
        assert_eq!(h.start(FORGE), Start::Listing { next: Duration::from_secs(2) }, "{stage:?}/{label}");
    }
}
#[test]
fn github_transient_start_failures_back_off_to_listing() {
    transient(Stage::Listing, TRANSPORT_OR_RATE);
}
#[test]
fn github_start_server_or_malformed_failures_back_off_to_listing() {
    transient(Stage::Listing, SERVER_OR_MALFORMED);
}
#[test]
fn github_transient_full_mint_failures_back_off_to_listing() {
    transient(Stage::Full, TRANSPORT_OR_RATE);
}
#[test]
fn github_full_mint_server_or_malformed_failures_back_off_to_listing() {
    transient(Stage::Full, MINT_SERVER_OR_MALFORMED);
}
#[test]
fn github_transient_contents_mint_failures_back_off_to_listing() {
    transient(Stage::Contents, TRANSPORT_OR_RATE);
}
#[test]
fn github_contents_mint_server_or_malformed_failures_back_off_to_listing() {
    transient(Stage::Contents, MINT_SERVER_OR_MALFORMED);
}
#[test]
fn github_transient_identity_failures_back_off_to_listing() {
    transient(Stage::Identity, TRANSPORT_OR_RATE);
}
#[test]
fn github_identity_server_or_malformed_failures_back_off_to_listing() {
    transient(Stage::Identity, SERVER_OR_MALFORMED);
}
#[test]
fn github_wait_doubles_without_reset_at_a_successful_page() {
    let mut h = harness();
    for (until, next, wait) in [(1, 2, 1), (3, 4, 2), (7, 8, 4), (15, 8, 8)] {
        h.answer(FORGE, &fixtures::response_with_fields(200, LIVE, NEXT_PAGE));
        h.answer(FORGE, &fixtures::status(503));
        waiting(&h, until, next);
        h.run(Duration::from_secs(wait));
        h.connected(FORGE);
    }
}
#[test]
fn github_retry_after_exceeds_the_backoff_max_and_expires_waiting_calls() {
    let mut h = harness();
    h.answer(FORGE, &fixtures::response_with_fields(429, b"{}", b"Retry-After: 20\r\n"));
    waiting(&h, 20, 2);
    h.call(1, fixtures::REPO, skein_forge::api::Op::Read(skein_forge::api::Read::Settings));
    h.run(Duration::from_secs(10));
    assert_eq!(h.answered(1).expect("call deadline").result, Err(skein_forge::api::Error::Unavailable));
    assert_eq!(h.answered(1).expect("call deadline").cost, 0);
    assert_eq!(h.connections_opened(), 1);
    h.run(Duration::from_secs(10));
    assert_eq!(h.connections_opened(), 2);
}
#[test]
fn github_each_send_has_a_fresh_deadline() {
    let mut h = harness();
    for (response, deadline) in [
        (fixtures::response_with_fields(200, LIVE, NEXT_PAGE), 19),
        (fixtures::ok(b"[]"), 28),
        (mint(FULL, EXPIRES), 37),
        (mint(CONTENTS, EXPIRES), 46),
    ] {
        h.run(Duration::from_secs(9));
        h.answer(FORGE, &response);
        assert_eq!(h.component.next_deadline(), Some(Time::ZERO.saturating_add(Duration::from_secs(deadline))));
    }
    h.run(Duration::from_secs(10));
    waiting(&h, 47, 2);
}
#[test]
fn github_mint_tokens_must_be_safe_nonempty_bounded_and_future() {
    for (label, token, expires) in
        [("empty", Box::from(b"".as_slice()), EXPIRES), ("unsafe", Box::from(b"bad\\nvalue".as_slice()), EXPIRES)]
    {
        let mut h = at(Stage::Full);
        h.answer(FORGE, &mint(&token, expires));
        waiting(&h, 1, 2);
        h.run(Duration::ZERO);
        assert!(tokens(&h).is_empty(), "{label}");
    }
}
#[test]
fn github_expired_mint_token_is_malformed() {
    let mut h = at(Stage::Full);
    h.answer(FORGE, &mint(FULL, b"1970-01-01T00:00:00Z"));
    waiting(&h, 1, 2);
    h.run(Duration::ZERO);
    assert!(tokens(&h).is_empty());
}
#[test]
fn github_mint_token_at_the_bound_is_accepted() {
    let mut h = at(Stage::Full);
    h.answer(FORGE, &mint(&fixtures::filled(TOKEN_BOUND, b'x'), EXPIRES));
    assert_eq!(h.start(FORGE), Start::Minting { next: SECOND });
    assert!(contains(&h.request_bytes(FORGE), b"{\"permissions\":{\"contents\":\"write\"}}"));
}
#[test]
fn github_mint_small_token_at_the_bound_is_kept() {
    let mut h = Harness::with(
        crate::Limits { token_bytes: SMALL_TOKEN_BOUND, ..fixtures::limits() },
        fixtures::shared(),
        fixtures::github_settings(),
    );
    accepted(&mut h, &[1]);
    h.connected(FORGE);
    listed(&mut h);
    let token = fixtures::filled(usize::try_from(SMALL_TOKEN_BOUND).expect("bound"), b'x');
    let response = mint(&token, EXPIRES);
    h.answer(FORGE, &response);
    h.answer(FORGE, &response);
    h.answer(FORGE, &fixtures::ok(VIEWER));
    h.run(Duration::ZERO);
    assert_eq!(h.start(FORGE), Start::Ready { cap: 100 });
    assert_eq!(tokens(&h).len(), 1);
    assert!(contains(&h.request_bytes(FORGE), b"Authorization: Bearer xxxxxxxx\r\n"), "Full is kept for Viewer");
}
#[test]
fn github_mint_one_past_a_small_token_bound_is_too_large() {
    use crate::connection::Exchange;
    use skein_forge::github::{
        plans::{
            plan::Bounds,
            start::{Minting, Step},
        },
        request::Scope,
    };
    let mut h = fixtures::ConnectionHarness::github();
    h.env.limits.token_bytes = SMALL_TOKEN_BOUND;
    let bounds = Bounds { inner: 8, page_size: 100, rows: 1, answer_bytes: 1024, body_bytes: fixtures::BODY_ROOM };
    let mut plan = Minting::new(1, Scope::Full);
    let request = match plan.begin(bounds).expect("mint begins") {
        Step::Send(exchange) => exchange.request,
        Step::Done(_) => unreachable!("mint sends"),
    };
    h.connection
        .start(
            &h.env,
            &h.forge,
            fixtures::USER_AGENT,
            Exchange::GitHub {
                exchange: skein_forge::github::plans::plan::Exchange {
                    request,
                    retention: None,
                    comparison: skein_forge::github::plans::plan::Comparison::Full,
                },
                authorization: Some(skein_lib::Secret::new(Box::from(b"Bearer artificial-jwt".as_slice()))),
            },
        )
        .expect("mint request");
    let token = fixtures::filled(usize::try_from(SMALL_TOKEN_BOUND + 1).expect("bound"), b'x');
    let heard = h.github_answer(&mint(&token, EXPIRES));
    let mut outcome = None;
    for event in heard.into_boxed() {
        if let Some(step) = plan.heard(event, bounds).expect("ordered response") {
            match step {
                Step::Done(result) => outcome = Some(result),
                Step::Send(_) => unreachable!("one mint exchange"),
            }
        }
    }
    match outcome {
        Some(Err(error)) => assert_eq!(error, skein_forge::api::Error::TooLarge),
        Some(Ok(_)) | None => unreachable!("oversized token is refused"),
    }
}
#[test]
fn github_name_bound_is_independent_of_shared_rows() {
    let mut h =
        Harness::with(fixtures::limits(), crate::Shared { rows: 1, ..fixtures::shared() }, fixtures::github_settings());
    accepted(&mut h, &[]);
    h.connected(FORGE);
    h.answer(FORGE, &fixtures::response_with_fields(200, PAGE_ONE, NEXT_PAGE));
    h.answer(FORGE, &fixtures::ok(PAGE_TWO));
    assert_eq!(h.start(FORGE), Start::Ready { cap: 100 });
}
#[test]
fn github_accepted_set_changes_discard_an_inflight_mint() {
    for stage in [Stage::Full, Stage::Contents] {
        let mut h = at(stage);
        accepted(&mut h, &[]);
        h.answer(FORGE, &mint(CONTENTS, EXPIRES));
        h.run(Duration::ZERO);
        assert!(tokens(&h).is_empty(), "{stage:?}");
        assert_eq!(h.start(FORGE), Start::Ready { cap: 100 });
        assert_eq!(h.connected_count, 1);
    }
}
#[test]
fn github_accepted_set_changes_during_listing_select_the_new_set() {
    let mut h = harness();
    h.answer(FORGE, &fixtures::response_with_fields(200, LIVE, NEXT_PAGE));
    accepted(&mut h, &[2]);
    h.answer(FORGE, &fixtures::ok(PAGE_TWO));
    h.answer(FORGE, &fixtures::ok(PAGE_TWO));
    assert!(contains(&h.request_bytes(FORGE), b"POST /app/installations/2/access_tokens"));
    assert!(!contains(&h.request_bytes(FORGE), b"POST /app/installations/1/access_tokens"));
}
#[test]
fn github_removed_inflight_mint_refusal_is_discarded() {
    let mut h = at(Stage::Full);
    accepted(&mut h, &[]);
    h.answer(FORGE, &fixtures::status(422));
    h.run(Duration::ZERO);
    assert_eq!(h.start(FORGE), Start::Ready { cap: 100 });
    assert_eq!(h.stopped(), None);
    assert!(tokens(&h).is_empty());
}

#[test]
fn github_removed_pending_mint_is_never_written() {
    let mut h = harness();
    h.answer(FORGE, &fixtures::response_with_fields(200, LIVE, b"Connection: close\r\n"));
    // The mint is pending on its replacement connection, before any request is written.
    accepted(&mut h, &[]);
    h.connected(FORGE);
    h.run(Duration::ZERO);
    assert_eq!(h.start(FORGE), Start::Ready { cap: 100 });
    assert!(!contains(&h.request_bytes(FORGE), b"POST "));
    assert!(tokens(&h).is_empty());
}

#[test]
fn github_own_connection_is_reserved_while_forgejo_calls_run() {
    let mut settings = fixtures::two_forges();
    let github = settings.forges.get_mut(1).expect("second forge");
    github.kind = crate::settings::Kind::GitHub;
    github.access = fixtures::github_settings().forges.into_boxed().into_iter().next().expect("GitHub forge").access;
    let mut h = Harness::new(settings);
    h.connected(FORGE);
    h.answer(FORGE, &fixtures::user_ok());
    h.answer(FORGE, &fixtures::settings_ok());
    h.connected(fixtures::OTHER_FORGE);
    h.answer(fixtures::OTHER_FORGE, &fixtures::ok(b"[]"));
    h.call(1, fixtures::REPO, skein_forge::api::Op::Read(skein_forge::api::Read::Settings));
    h.call(2, fixtures::REPO, skein_forge::api::Op::Read(skein_forge::api::Read::Settings));
    h.connected(FORGE);
    assert_eq!(h.connections_for(FORGE), 2);
    assert_eq!(h.connections_for(fixtures::OTHER_FORGE), 1);
    assert!(h.answered(1).is_none());
    assert!(h.answered(2).is_none());
    assert!(!contains(&h.request_bytes(fixtures::OTHER_FORGE), b"/api/v1"));
}

#[test]
fn github_ready_calls_without_an_accepted_installation_are_not_installed() {
    let mut h = harness();
    accepted(&mut h, &[]);
    h.answer(FORGE, &fixtures::ok(b"[]")); // The changed set discards the written page.
    listed(&mut h);
    h.call(1, fixtures::REPO, skein_forge::api::Op::Read(skein_forge::api::Read::Settings));
    h.run(Duration::ZERO);
    let answer = h.answered(1).expect("call terminal");
    assert_eq!(answer.result, Err(skein_forge::api::Error::NotInstalled));
    assert_eq!(answer.cost, 0);
    assert!(!contains(&h.request_bytes(FORGE), b"/api/v1"));
}
#[test]
fn github_replacement_discards_the_old_start_and_tokens() {
    replacement(Stage::Listing);
}
#[test]
fn github_replacement_discards_the_old_full_mint() {
    replacement(Stage::Full);
}
#[test]
fn github_replacement_discards_the_old_contents_mint() {
    replacement(Stage::Contents);
}
#[test]
fn github_replacement_discards_the_old_identity() {
    replacement(Stage::Identity);
}
fn replacement(stage: Stage) {
    let mut h = at(stage);
    let owner = *h.peers.first().expect("old peer").0;
    h.send(Request::Connect {
        forge: FORGE,
        access: fixtures::app_access(skein_fake_forge_github::keys::KEYS.first().expect("public key"), 8, 9),
    });
    h.component.up(
        &h.env,
        skein_io::Event::Stream { owner, up: skein_lib::stream::Up::Bytes(fixtures::ok(VIEWER)) },
        &mut h.up,
        &mut h.io,
    );
    h.run(Duration::ZERO);
    h.connected(FORGE);
    accepted(&mut h, &[]);
    h.answer(FORGE, &fixtures::ok(b"[]")); // The changed set discards the written page.
    listed(&mut h);
    h.run(Duration::ZERO);
    assert_eq!(h.connected_count, 1, "{stage:?}");
    let expected_tokens = if let Stage::Identity = stage { 1 } else { 0 };
    assert_eq!(tokens(&h).len(), expected_tokens, "{stage:?}: no old mint published");
}
#[test]
fn github_close_ends_each_start_phase() {
    close_phase(Stage::Listing);
}
#[test]
fn github_close_ends_the_full_mint_phase() {
    close_phase(Stage::Full);
}
#[test]
fn github_close_ends_the_contents_mint_phase() {
    close_phase(Stage::Contents);
}
#[test]
fn github_close_ends_the_identity_phase() {
    close_phase(Stage::Identity);
}
fn close_phase(stage: Stage) {
    let mut h = at(stage);
    h.send(Request::Close);
    h.run(Duration::ZERO);
    assert!(h.closed, "{stage:?}");
    let count = tokens(&h).len();
    let connected = h.connected_count;
    h.send(Request::Installations { forge: FORGE, accepted: Box::from([1]) });
    h.send(Request::Connect {
        forge: FORGE,
        access: fixtures::app_access(skein_fake_forge_github::keys::KEYS.first().expect("public key"), 7, 9),
    });
    h.run(Duration::from_secs(20));
    assert_eq!((tokens(&h).len(), h.connected_count), (count, connected));
    assert!(h.up.is_empty());
}
#[test]
#[expect(clippy::disallowed_macros, reason = "the test inspects Secret redaction in Debug")]
fn installation_secrets_leave_only_as_git_tokens() {
    let mut h = at(Stage::Identity);
    let debug = std::format!("{:?} {:?} {:?}", h.component, h.up, h.io);
    assert!(!contains(debug.as_bytes(), FULL));
    assert!(!contains(debug.as_bytes(), CONTENTS));
    h.answer(FORGE, &fixtures::ok(VIEWER));
    h.run(Duration::ZERO);
    for line in h.request_lines(FORGE).into_boxed() {
        assert!(!contains(&line, FULL));
        assert!(!contains(&line, CONTENTS));
    }
    let debug = std::format!("{:?} {:?} {:?}", h.component, h.up, h.git_tokens);
    assert!(!contains(debug.as_bytes(), FULL));
    assert!(!contains(debug.as_bytes(), CONTENTS));
    assert_eq!(h.component.diagnostics(), crate::Diagnostics { malformed: 0, long_bodies: 0 });
    assert!(h.answers.is_empty());
    assert_eq!(tokens(&h).len(), 1);
}

#[test]
fn github_full_mint_replaces_levels_and_contents_mint_preserves_them() {
    use skein_forge::github::document::{Level, Levels};
    let mut h = at(Stage::Full);
    let full = br#"{"token":"artificial-full","expires_at":"1970-01-01T01:00:00Z","permissions":{"contents":"write","pull_requests":"write"}}"#;
    h.answer(FORGE, &fixtures::response(201, full));
    let expected = Levels { contents: Level::Write, pull_requests: Level::Write };
    assert_eq!(h.component.test_github().rows.get(&1).expect("installation").permissions, expected);
    let narrowed =
        br#"{"token":"artificial-git","expires_at":"1970-01-01T01:00:00Z","permissions":{"contents":"write"}}"#;
    h.answer(FORGE, &fixtures::response(201, narrowed));
    assert_eq!(h.component.test_github().rows.get(&1).expect("installation").permissions, expected);
    h.answer(FORGE, &fixtures::ok(VIEWER));
    h.run(Duration::ZERO);
    assert!(h.component.test_github().identity_checked);
    h.component.test_github_mut().rows.get_mut(&1).expect("installation").full.pending = true;
    h.run(Duration::ZERO);
    // An absent permissions object in the next full mint revokes both kept levels.
    h.answer(FORGE, &mint(FULL, EXPIRES));
    assert_eq!(h.component.test_github().rows.get(&1).expect("installation").permissions, Levels::NONE);
}
