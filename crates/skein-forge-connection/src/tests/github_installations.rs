//! Installation reconciliation through the component boundary and the landed page plan.
use super::fixtures::{self, FORGE, Harness};
use crate::{Account, Change, Event, Request, start::Start};
use alloc::boxed::Box;
use skein_forge::{
    api,
    github::{
        document,
        plans::{
            Body, Heard,
            start::{Installations, Step},
        },
    },
};
use skein_lib::{Duration, List, Time, Wall};

const SECOND: Duration = Duration::from_secs(1);
const LIVE: &[u8] = br#"[{"id":1,"account":{"id":22,"login":"acme","type":"Organization"},"created_at":"1970-01-01T00:00:01Z","suspended_at":null}]"#;
const SUSPENDED: &[u8] = br#"[{"id":1,"account":{"id":22,"login":"acme","type":"Organization"},"created_at":"1970-01-01T00:00:01Z","suspended_at":"1970-01-01T00:00:02Z"}]"#;
const OTHER: &[u8] = br#"[{"id":2,"account":{"id":23,"login":"other","type":"User"},"created_at":"1970-01-01T00:00:01Z","suspended_at":"1970-01-01T00:00:02Z"}]"#;
const NEXT_PAGE: &[u8] = b"Link: <https://api.github.test/app/installations?per_page=100&page=2>; rel=\"next\"\r\n";
const FULL: &[u8] = br#"{"token":"artificial-full","expires_at":"1970-01-01T01:00:00Z"}"#;
const MISSING: u64 = 9;
const ALSO_MISSING: u64 = 10;
const PAGE_CAP: u32 = skein_forge::github::pin::PAGE_CAP;
fn accepted(h: &mut Harness, ids: &[u64]) {
    h.send(Request::Installations { forge: FORGE, accepted: Box::from(ids) });
}
fn harness(cap: u32, ids: &[u64]) -> Harness {
    let mut h = Harness::with(
        crate::Limits { installations: cap, ..fixtures::limits() },
        fixtures::shared(),
        fixtures::github_settings(),
    );
    accepted(&mut h, ids);
    h.connected(FORGE);
    h
}
fn changes(h: &mut Harness) -> Box<[(u64, Option<Account>, Change)]> {
    h.run(Duration::ZERO);
    let events = h.installation_events.as_mut().expect("GitHub fixture");
    let mut out = List::with_capacity(events.len());
    for event in events.as_slice() {
        match event {
            Event::Installation { installation, account, change, .. } => {
                out.push((*installation, account.clone(), *change)).expect("measured changes");
            }
            Event::Fact { .. }
            | Event::GitToken { .. }
            | Event::Answered(_)
            | Event::Hint(_)
            | Event::Listening { .. }
            | Event::ListenerLost
            | Event::Connected { .. }
            | Event::Stopped { .. }
            | Event::Closed => unreachable!("installation collection"),
        }
    }
    events.clear();
    out.into_boxed()
}
fn account() -> Account {
    Account { id: 22, login: Box::from(b"acme".as_slice()) }
}
fn reconnect(h: &mut Harness) {
    h.send(Request::Connect {
        forge: FORGE,
        access: fixtures::app_access(skein_fake_forge_github::keys::KEYS.first().expect("public key"), 7, 9),
    });
    h.run(Duration::ZERO);
    h.connected(FORGE);
}
#[test]
fn first_read_reports_missing_accepted_id() {
    let mut h = harness(3, &[MISSING]);
    h.answer(FORGE, &fixtures::ok(b"[]"));
    assert_eq!(changes(&mut h).as_ref(), [(MISSING, None, Change::Deleted)]);
}
#[test]
fn first_read_reports_suspended_accepted_row() {
    let mut h = harness(3, &[1]);
    h.answer(FORGE, &fixtures::ok(SUSPENDED));
    assert_eq!(changes(&mut h).as_ref(), [(1, Some(account()), Change::Suspended)]);
}
#[test]
fn first_read_reports_unaccepted_row() {
    let mut h = harness(3, &[]);
    h.answer(FORGE, &fixtures::ok(LIVE));
    assert_eq!(changes(&mut h).as_ref(), [(1, Some(account()), Change::Created)]);
}
#[test]
fn first_read_keeps_live_accepted_row_silent() {
    let mut h = harness(3, &[1]);
    h.answer(FORGE, &fixtures::ok(LIVE));
    assert!(changes(&mut h).is_empty());
}
#[test]
fn failed_first_read_keeps_the_baseline() {
    let mut h = harness(3, &[1, MISSING]);
    h.answer(FORGE, &fixtures::response_with_fields(200, LIVE, NEXT_PAGE));
    h.answer(FORGE, &fixtures::status(503));
    h.run(SECOND);
    h.connected(FORGE);
    h.answer(FORGE, &fixtures::ok(SUSPENDED));
    assert_eq!(changes(&mut h).as_ref(), [(1, Some(account()), Change::Suspended), (MISSING, None, Change::Deleted)]);
}
#[test]
fn reconnect_compares_tables_instead_of_repeating_the_baseline() {
    let mut h = harness(3, &[1, MISSING]);
    h.answer(FORGE, &fixtures::ok(SUSPENDED));
    drop(changes(&mut h));
    reconnect(&mut h);
    h.answer(FORGE, &fixtures::ok(SUSPENDED));
    assert!(changes(&mut h).is_empty());
    assert_eq!(h.connected_count, 2);
}
#[test]
fn missing_accepted_ids_share_one_reread() {
    let mut h = harness(3, &[]);
    h.answer(FORGE, &fixtures::ok(b"[]"));
    drop(changes(&mut h));
    accepted(&mut h, &[1, MISSING, ALSO_MISSING]);
    h.run(Duration::ZERO);
    h.answer(FORGE, &fixtures::ok(SUSPENDED));
    assert_eq!(
        changes(&mut h).as_ref(),
        [
            (1, Some(account()), Change::Created),
            (MISSING, None, Change::Deleted),
            (ALSO_MISSING, None, Change::Deleted)
        ]
    );
    let count = h.request_lines(FORGE).len();
    accepted(&mut h, &[ALSO_MISSING, MISSING, 1]);
    h.run(Duration::from_secs(100));
    assert_eq!(count, 2);
    assert_eq!(h.connections_opened(), 1);
    assert!(changes(&mut h).is_empty());
    assert_eq!(h.connected_count, 1);
}
#[test]
fn failed_missing_read_retries_without_marking_absence() {
    let mut h = harness(3, &[]);
    h.answer(FORGE, &fixtures::ok(b"[]"));
    drop(changes(&mut h));
    accepted(&mut h, &[MISSING]);
    h.run(Duration::ZERO);
    h.answer(FORGE, &fixtures::response_with_fields(429, b"{}", b"Retry-After: 3\r\n"));
    assert_eq!(h.start(FORGE), Start::Ready { cap: PAGE_CAP });
    assert!(changes(&mut h).is_empty());
    h.run(Duration::from_secs(3));
    h.connected(FORGE);
    h.answer(FORGE, &fixtures::ok(b"[]"));
    assert_eq!(changes(&mut h).as_ref(), [(MISSING, None, Change::Deleted)]);
    assert_eq!(h.connected_count, 1);
}
#[test]
fn baseline_and_missing_id_rule_emit_one_deleted() {
    let mut h = harness(3, &[MISSING]);
    h.answer(FORGE, &fixtures::ok(b"[]"));
    drop(changes(&mut h));
    accepted(&mut h, &[MISSING]);
    h.run(Duration::ZERO);
    assert_eq!(h.request_lines(FORGE).len(), 1);
    assert!(changes(&mut h).is_empty());
}
#[test]
fn accepting_only_present_rows_schedules_no_read() {
    let mut h = harness(3, &[]);
    h.answer(FORGE, &fixtures::ok(SUSPENDED));
    drop(changes(&mut h));
    accepted(&mut h, &[1]);
    assert!(changes(&mut h).is_empty());
    assert_eq!(h.request_lines(FORGE).len(), 1);
}
#[test]
fn new_accepted_set_discards_stale_read() {
    let mut h = harness(3, &[1]);
    h.run(Duration::from_secs(2));
    h.answer(FORGE, &fixtures::response_with_fields(200, SUSPENDED, NEXT_PAGE));
    accepted(&mut h, &[2, MISSING]);
    h.answer(FORGE, &fixtures::ok(OTHER));
    assert!(changes(&mut h).is_empty());
    h.run(SECOND);
    h.answer(FORGE, &fixtures::ok(OTHER));
    assert_eq!(
        changes(&mut h).as_ref(),
        [
            (2, Some(Account { id: 23, login: Box::from(b"other".as_slice()) }), Change::Suspended),
            (MISSING, None, Change::Deleted)
        ]
    );
    assert_eq!(
        h.request_lines(FORGE).as_slice(),
        [
            Box::from(b"GET /app/installations?per_page=100&page=1 HTTP/1.1".as_slice()),
            Box::from(b"GET /app/installations?per_page=100&page=2 HTTP/1.1".as_slice()),
            Box::from(b"GET /app/installations?per_page=100&page=1 HTTP/1.1".as_slice()),
        ]
    );
    assert_eq!(h.connected_count, 1);
}
fn document_row(id: u64, login_bytes: usize) -> document::Row {
    document::Row::Installation(document::Installation {
        id,
        login: fixtures::filled(login_bytes, b'x'),
        account: 22,
        kind: document::Account::Organization,
        created: Wall::EPOCH,
        suspended: false,
    })
}
fn page_outcome(
    count: u32,
    row: Option<document::Row>,
) -> Result<skein_forge::github::plans::start::InstallationPage, api::Error> {
    let bounds = crate::limits::installation_bounds(&fixtures::limits(), &fixtures::shared()).expect("bounds");
    let mut plan = Installations::new(1);
    drop(plan.begin(bounds).expect("begin page"));
    let head = Heard::Head { status: 200, head: skein_forge::github::header::Head::new() };
    drop(plan.heard(head, bounds).expect("head"));
    for id in 0..count {
        if let Some(step) = plan.heard(Heard::Row(document_row(u64::from(id), 4)), bounds).expect("row") {
            return outcome(step);
        }
    }
    if let Some(row) = row
        && let Some(step) = plan.heard(Heard::Row(row), bounds).expect("extra row")
    {
        return outcome(step);
    }
    outcome(
        plan.heard(Heard::Done(Body::Document(document::Document::Rows)), bounds).expect("done").expect("page result"),
    )
}
fn outcome(
    step: Step<skein_forge::github::plans::start::InstallationPage>,
) -> Result<skein_forge::github::plans::start::InstallationPage, api::Error> {
    match step {
        Step::Done(result) => result,
        Step::Send(_) => unreachable!("single page"),
    }
}
#[test]
fn installation_page_limits_are_not_table_overflow() {
    let mut h = harness(1, &[1]);
    h.answer(FORGE, &fixtures::response_with_fields(200, OTHER, NEXT_PAGE));
    h.answer(FORGE, &fixtures::ok(SUSPENDED));
    assert_eq!(changes(&mut h).as_ref(), [(1, Some(account()), Change::Suspended)]);
    assert_eq!(h.facts.as_ref().expect("facts").len(), 1);
    assert_eq!(page_outcome(PAGE_CAP, None).expect("full page").rows.len(), 100);
}
#[test]
fn installation_page_over_cap_is_a_failed_read() {
    assert_eq!(page_outcome(PAGE_CAP + 1, None).expect_err("too many rows"), api::Error::TooLarge);
}
#[test]
fn installation_page_wrong_row_kind_is_a_failed_read() {
    assert_eq!(
        page_outcome(0, Some(document::Row::Branch(Box::from(b"main".as_slice())))).expect_err("wrong row kind"),
        api::Error::InvalidAnswer
    );
}
#[test]
fn installation_login_at_name_bound_succeeds() {
    name_bound(4, true);
}
#[test]
fn installation_login_past_name_bound_fails_without_unkept() {
    name_bound(5, false);
}
fn name_bound(width: usize, succeeds: bool) {
    let mut h = Harness::with(
        fixtures::limits(),
        crate::Shared { name_bytes: 4, ..fixtures::shared() },
        fixtures::github_settings(),
    );
    h.connected(FORGE);
    let head = br#"[{"id":1,"account":{"id":22,"login":""#;
    let tail = br#"","type":"Organization"},"created_at":"1970-01-01T00:00:01Z","suspended_at":null}]"#;
    let mut body = skein_lib::Writer::new(
        head.len().checked_add(width).expect("login length").checked_add(tail.len()).expect("JSON framing"),
    );
    body.put(head).expect("head");
    body.put(&fixtures::filled(width, b'x')).expect("name");
    body.put(tail).expect("tail");
    h.answer(FORGE, &fixtures::ok(&body.finish()));
    let events = changes(&mut h);
    assert_eq!(!events.is_empty(), succeeds);
    assert!(h.facts.as_ref().expect("facts").is_empty());
    assert_eq!(h.start(FORGE) == Start::Ready { cap: PAGE_CAP }, succeeds);
}
#[test]
fn failed_page_restarts_at_one_with_a_new_send_deadline() {
    let mut h = harness(3, &[]);
    h.run(SECOND);
    h.answer(FORGE, &fixtures::response_with_fields(200, LIVE, NEXT_PAGE));
    assert_eq!(h.component.next_deadline(), Some(Time::from_nanos(11_000_000_000)));
    h.answer(FORGE, &fixtures::status(503));
    h.run(SECOND);
    h.connected(FORGE);
    assert_eq!(
        h.request_lines(FORGE).as_slice(),
        [Box::from(b"GET /app/installations?per_page=100&page=1 HTTP/1.1".as_slice())]
    );
    assert_eq!(h.component.next_deadline(), Some(Time::from_nanos(12_000_000_000)));
}

#[test]
fn changed_acceptance_during_startup_mint_reads_the_latest_set() {
    let mut h = harness(3, &[1]);
    h.answer(FORGE, &fixtures::ok(LIVE));
    accepted(&mut h, &[2, MISSING]);
    h.answer(FORGE, &fixtures::response(201, FULL));
    h.answer(FORGE, &fixtures::ok(OTHER));
    assert_eq!(h.start(FORGE), Start::Ready { cap: PAGE_CAP });
    assert_eq!(
        changes(&mut h).as_ref(),
        [
            (1, Some(account()), Change::Deleted),
            (2, Some(Account { id: 23, login: Box::from(b"other".as_slice()) }), Change::Created),
            (MISSING, None, Change::Deleted),
        ]
    );
    assert!(h.git_tokens.as_ref().expect("tokens").is_empty());
    assert_eq!(h.connected_count, 1);
}
#[test]
fn ready_read_refusal_stops_once_per_connect() {
    let mut h = harness(3, &[]);
    h.answer(FORGE, &fixtures::ok(b"[]"));
    drop(changes(&mut h));
    accepted(&mut h, &[MISSING]);
    h.answer(FORGE, &fixtures::status(401));
    h.run(Duration::ZERO);
    assert_eq!(h.connected_count, 1);
    assert_eq!(h.stopped(), Some((FORGE, crate::Stopped::KeyRefused)));
    h.run(Duration::from_secs(100));
    assert!(changes(&mut h).is_empty());
}
