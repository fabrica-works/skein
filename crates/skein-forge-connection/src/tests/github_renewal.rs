//! Renewal uses the reserved connection; ordinary GitHub calls stay guarded until task 27.
use super::{
    fixtures::{self, FORGE, Harness},
    github_installation_loss::{commit, public_ready, requests, row},
    github_start::{CONTENTS, EXPIRES, FULL, LIVE, VIEWER, accepted, contains, mint, tokens},
};
use crate::{Event, MAX_OUT, Request, Stopped, installations::Tokens};
use alloc::boxed::Box;
use skein_forge::github::request::Scope;
use skein_lib::{Duration, Queue, Time, Wall};

const SECOND: Duration = Duration::from_secs(1);
// One-hour Full and seventy-minute Contents, with the fixture's sixty-second lead.
pub(crate) const FULL_DUE: u64 = 3540;
const CONTENTS_DUE: u64 = 4140;
const CONTENTS_EXPIRY: &[u8] = b"1970-01-01T01:10:00Z";
const LATER: &[u8] = b"1970-01-01T03:00:00Z";
const NEW_FULL: &[u8] = b"artificial-renewed-full";
const NEW_CONTENTS: &[u8] = b"artificial-renewed-contents";
const INITIAL_REQUESTS: u32 = 4; // listing, two scopes and Viewer
const REPEATED_FIRES: u32 = 3; // one consumed timer must withstand repeated delivery
pub(crate) fn ready() -> Harness {
    initial(CONTENTS_EXPIRY)
}
fn initial(contents_expiry: &[u8]) -> Harness {
    let mut h = Harness::new(fixtures::github_settings());
    accepted(&mut h, &[1]);
    h.connected(FORGE);
    h.answer(FORGE, &fixtures::ok(LIVE));
    h.answer(FORGE, &mint(FULL, EXPIRES));
    h.answer(FORGE, &mint(CONTENTS, contents_expiry));
    h.answer(FORGE, &fixtures::ok(VIEWER));
    h.run(Duration::ZERO);
    // Let the idle socket settle before crossing the renewal timer.
    h.run(h.env.limits.idle);
    h
}
pub(crate) fn advance(h: &mut Harness, seconds: u64) {
    h.run(Time::from_nanos(Duration::from_secs(seconds).as_nanos()).saturating_since(h.env.now));
}
pub(crate) fn renewing() -> Harness {
    let mut h = ready();
    advance(&mut h, FULL_DUE);
    h.connected(FORGE);
    h
}
pub(crate) fn contents_renewing() -> Harness {
    let mut h = renewing();
    renewed_full(&mut h);
    h.run(h.env.limits.idle);
    advance(&mut h, CONTENTS_DUE);
    h.connected(FORGE);
    h
}
fn usable(h: &Harness, value: &[u8]) {
    let row = h.component.test_github().rows.get(&1).expect("eligible row");
    assert!(row.credential(h.env.now).expect("usable Full").expose() == value);
}
pub(crate) fn renewed_full(h: &mut Harness) {
    h.answer(FORGE, &mint(NEW_FULL, LATER));
    h.run(Duration::ZERO);
}
pub(crate) fn renewed_contents(h: &mut Harness) {
    h.answer(FORGE, &mint(NEW_CONTENTS, LATER));
    h.run(Duration::ZERO);
}
fn expiry(h: &Harness, index: usize) -> Wall {
    match tokens(h).get(index).expect("Contents event") {
        Event::GitToken { expires, .. } => *expires,
        Event::Installation { .. }
        | Event::Fact { .. }
        | Event::Answered(_)
        | Event::Hint(_)
        | Event::ListenerLost
        | Event::Connected { .. }
        | Event::Stopped { .. }
        | Event::Closed => unreachable!("token history"),
    }
}
#[test]
fn each_scope_renews_at_its_own_due_time() {
    let mut h = ready();
    let row = h.component.test_github().rows.get(&1).expect("initial row");
    assert_eq!(
        (row.full.expiry, row.full.due),
        (Time::from_nanos(3_600_000_000_000), Some(Time::from_nanos(3_540_000_000_000)))
    );
    assert_eq!(
        (row.contents.expiry, row.contents.due),
        (Time::from_nanos(4_200_000_000_000), Some(Time::from_nanos(4_140_000_000_000)))
    );
    advance(&mut h, FULL_DUE - 1);
    assert_eq!(requests(&h), INITIAL_REQUESTS);
    assert_eq!(h.connections_opened(), 1);
    advance(&mut h, FULL_DUE);
    h.connected(FORGE);
    assert_eq!(h.component.test_github().mint, Some((1, Scope::Full)));
    renewed_full(&mut h);
    assert_eq!(requests(&h), INITIAL_REQUESTS + 1);
    h.run(h.env.limits.idle);
    advance(&mut h, CONTENTS_DUE - 1);
    assert_eq!(requests(&h), INITIAL_REQUESTS + 1);
    advance(&mut h, CONTENTS_DUE);
    h.connected(FORGE);
    assert_eq!(h.component.test_github().mint, Some((1, Scope::Contents)));
    renewed_contents(&mut h);
    assert_eq!(requests(&h), INITIAL_REQUESTS + 2);
    assert_eq!(tokens(&h).len(), 2);
    assert_eq!(expiry(&h, 1), Wall::from_nanos(10_800_000_000_000));
    assert_eq!(
        h.component.test_github().rows.get(&1).expect("row").contents.due,
        Some(Time::from_nanos(14_880_000_000_000))
    );
    public_ready(&h);
}
#[test]
fn both_due_selects_full_before_contents() {
    let mut h = initial(EXPIRES);
    advance(&mut h, FULL_DUE);
    h.connected(FORGE);
    assert_eq!(h.component.test_github().mint, Some((1, Scope::Full)));
    renewed_full(&mut h);
    assert_eq!(h.component.test_github().mint, Some((1, Scope::Contents)));
    renewed_contents(&mut h);
    assert_eq!(requests(&h), INITIAL_REQUESTS + 2);
    public_ready(&h);
}
#[test]
fn ready_time_mints_fix_both_scope_clocks() {
    let mut h = Harness::new(fixtures::github_settings());
    h.connected(FORGE);
    h.answer(FORGE, &fixtures::ok(LIVE));
    h.run(Duration::from_secs(100));
    accepted(&mut h, &[1]);
    h.run(Duration::ZERO);
    h.connected(FORGE);
    h.answer(FORGE, &mint(FULL, EXPIRES));
    h.answer(FORGE, &mint(CONTENTS, CONTENTS_EXPIRY));
    h.answer(FORGE, &fixtures::ok(VIEWER));
    h.run(Duration::ZERO);
    let row = h.component.test_github().rows.get(&1).expect("Ready-time row");
    assert_eq!(
        (row.full.expiry, row.full.due),
        (Time::from_nanos(3_700_000_000_000), Some(Time::from_nanos(3_640_000_000_000)))
    );
    assert_eq!(
        (row.contents.expiry, row.contents.due),
        (Time::from_nanos(4_300_000_000_000), Some(Time::from_nanos(4_240_000_000_000)))
    );
    public_ready(&h);
}
#[test]
fn later_wall_jumps_leave_due_and_expiry_fixed() {
    let mut h = ready();
    h.env.wall = Wall::from_nanos(u64::MAX);
    advance(&mut h, FULL_DUE - 1);
    usable(&h, FULL);
    assert_eq!(h.component.next_deadline(), Some(Time::from_nanos(3_540_000_000_000)));
    h.env.wall = Wall::EPOCH;
    advance(&mut h, FULL_DUE);
    h.connected(FORGE);
    usable(&h, FULL);
    assert_eq!(h.component.test_github().mint, Some((1, Scope::Full)));
}
#[test]
fn renewal_keeps_old_full_while_minting() {
    let h = renewing();
    usable(&h, FULL);
    public_ready(&h);
}
#[test]
fn renewal_keeps_old_full_during_backoff() {
    let mut h = renewing();
    h.answer(FORGE, &fixtures::status(503));
    usable(&h, FULL);
    public_ready(&h);
}
#[test]
fn expired_full_stays_eligible_without_a_credential() {
    let mut h = renewing();
    h.answer(FORGE, &fixtures::response_with_fields(429, b"{}", b"Retry-After: 120\r\n"));
    advance(&mut h, 3600);
    let row = h.component.test_github().rows.get(&1).expect("eligible expired row");
    assert!(row.accepted && !row.suspended);
    assert!(row.credential(h.env.now).is_none());
    assert!(row.api.is_none());
    assert_eq!(h.component.next_deadline(), Some(Time::from_nanos(3_660_000_000_000)));
    h.run(Duration::ZERO);
    assert_eq!(h.component.next_deadline(), Some(Time::from_nanos(3_660_000_000_000)));
    assert_eq!(requests(&h), INITIAL_REQUESTS + 1);
    public_ready(&h);
}
#[test]
fn successful_full_replaces_old_value_and_revision() {
    let mut h = renewing();
    renewed_full(&mut h);
    usable(&h, NEW_FULL);
    let row = h.component.test_github().rows.get(&1).expect("row");
    assert_eq!(row.token_revision, 2);
    assert_eq!(
        (row.full.expiry, row.full.due),
        (Time::from_nanos(14_340_000_000_000), Some(Time::from_nanos(14_280_000_000_000)))
    );
    assert_eq!(tokens(&h).len(), 1);
    public_ready(&h);
}
#[test]
fn contents_only_renewal_keeps_full_usable() {
    let mut h = contents_renewing();
    usable(&h, NEW_FULL);
    renewed_contents(&mut h);
    usable(&h, NEW_FULL);
    assert_eq!(h.component.test_github().rows.get(&1).expect("row").token_revision, 2);
}
fn waits(mut h: Harness, label: &str) {
    h.run(Duration::ZERO);
    let due = Time::from_nanos(3_541_000_000_000);
    assert_eq!(h.component.test_github().deadline(), Some(due), "{label}");
    h.run(Duration::from_millis(999));
    assert_eq!(requests(&h), INITIAL_REQUESTS + 1, "{label}");
    h.run(Duration::from_millis(1));
    h.connected(FORGE);
    for _ in 0..REPEATED_FIRES {
        h.run(Duration::ZERO);
    }
    assert_eq!(requests(&h), INITIAL_REQUESTS + 2, "{label}");
    renewed_full(&mut h);
    assert_eq!(requests(&h), INITIAL_REQUESTS + 2, "{label}");
    public_ready(&h);
}
#[test]
fn renewal_failure_waits_before_retry() {
    for (label, status, body) in [("rate", 429, b"{}".as_slice()), ("server", 503, b"{}"), ("malformed", 201, b"{")] {
        let mut h = renewing();
        h.answer(FORGE, &fixtures::response(status, body));
        waits(h, label);
    }
}
#[test]
fn renewal_transport_failure_waits_before_retry() {
    let mut h = renewing();
    h.reset(FORGE);
    waits(h, "transport");
}
#[test]
fn renewal_timeout_waits_before_retry() {
    let mut h = renewing();
    h.run(h.env.limits.deadline);
    assert_eq!(h.component.test_github().deadline(), Some(Time::from_nanos(3_551_000_000_000)));
    usable(&h, FULL);
}
#[test]
fn invalid_renewed_value_keeps_old_full_and_waits() {
    let mut h = renewing();
    h.answer(FORGE, &mint(b"", LATER));
    usable(&h, FULL);
    assert_eq!(h.component.test_github().deadline(), Some(Time::from_nanos(3_541_000_000_000)));
}
#[test]
fn renewal_backoff_doubles_to_cap() {
    let mut h = renewing();
    for (wait, until) in [(1, 3541), (2, 3543), (4, 3547), (8, 3555), (8, 3563)] {
        h.answer(FORGE, &fixtures::status(503));
        assert_eq!(h.component.test_github().deadline(), Some(Time::from_nanos(Duration::from_secs(until).as_nanos())));
        h.run(Duration::from_secs(wait));
        h.connected(FORGE);
    }
    renewed_full(&mut h);
    h.run(h.env.limits.idle);
    advance(&mut h, CONTENTS_DUE);
    h.connected(FORGE);
    h.answer(FORGE, &fixtures::status(503));
    assert_eq!(h.component.test_github().deadline(), Some(Time::from_nanos(4_141_000_000_000)));
}
#[test]
fn renewal_long_retry_after_wins() {
    let mut h = renewing();
    h.answer(FORGE, &fixtures::response_with_fields(429, b"{}", b"Retry-After: 20\r\n"));
    assert_eq!(h.component.test_github().deadline(), Some(Time::from_nanos(3_560_000_000_000)));
    advance(&mut h, 3559);
    assert_eq!(requests(&h), INITIAL_REQUESTS + 1);
    advance(&mut h, 3560);
    h.connected(FORGE);
    assert_eq!(requests(&h), INITIAL_REQUESTS + 2);
}
#[test]
fn renewal_401_stops_once_after_connected() {
    let mut h = renewing();
    h.answer(FORGE, &fixtures::status(401));
    h.run(Duration::from_secs(100));
    assert_eq!(h.stopped(), Some((FORGE, Stopped::KeyRefused)));
    assert_eq!(h.connected_count, 1);
    assert_eq!(requests(&h), INITIAL_REQUESTS + 1);
    assert_eq!(h.component.test_github().deadline(), None);
}
#[test]
fn renewal_terminal_mint_status_stops_token_refused() {
    let mut h = renewing();
    h.answer(FORGE, &fixtures::status(422));
    h.run(Duration::ZERO);
    assert_eq!(h.stopped(), Some((FORGE, Stopped::TokenRefused { installation: 1, status: 422 })));
    assert_eq!(h.connected_count, 1);
}
fn refusal_refresh(status: u16, body: &[u8]) {
    let mut h = renewing();
    h.answer(FORGE, &fixtures::status(status));
    let row = h.component.test_github().rows.get(&1).expect("refused row");
    assert!(row.api.is_none());
    assert_eq!((row.full.due, row.contents.due), (None, None));
    h.run(Duration::ZERO);
    assert_eq!(requests(&h), INITIAL_REQUESTS + 1);
    h.run(SECOND);
    h.connected(FORGE);
    h.answer(FORGE, &fixtures::ok(body));
    h.run(Duration::ZERO);
    assert_eq!(requests(&h), INITIAL_REQUESTS + 2);
    assert_eq!(tokens(&h).len(), 1);
    public_ready(&h);
}
#[test]
fn renewal_403_waits_then_rereads_suspension() {
    refusal_refresh(403, super::github_start::SUSPENDED);
}
#[test]
fn renewal_404_waits_then_rereads_deletion() {
    refusal_refresh(404, b"[]");
}
#[test]
fn another_current_row_survives_transient_renewal_failure() {
    let mut h = ready();
    commit(&mut h, Box::from([row(1, false), row(2, false)]));
    accepted(&mut h, &[1, 2]);
    h.run(Duration::ZERO);
    h.connected(FORGE);
    h.answer(FORGE, &mint(FULL, LATER));
    h.answer(FORGE, &mint(CONTENTS, LATER));
    h.run(h.env.limits.idle);
    advance(&mut h, FULL_DUE);
    h.connected(FORGE);
    h.answer(FORGE, &fixtures::status(503));
    let row = h.component.test_github().rows.get(&2).expect("other Current");
    assert_eq!(row.state, Tokens::Current);
    assert!(row.credential(h.env.now).is_some());
    public_ready(&h);
}
#[test]
fn loss_discards_a_written_renewal_result() {
    let mut h = renewing();
    accepted(&mut h, &[]);
    h.answer(FORGE, &mint(NEW_FULL, LATER));
    h.run(Duration::ZERO);
    let row = h.component.test_github().rows.get(&1).expect("Gone");
    assert_eq!(row.state, Tokens::Gone);
    assert!(row.api.is_none());
    assert_eq!(tokens(&h).len(), 1);
}
#[derive(Clone, Copy)]
enum Stage {
    Minting,
    Waiting,
    Held,
}
fn stage(stage: Stage) -> Harness {
    let mut h = renewing();
    match stage {
        Stage::Minting => {}
        Stage::Waiting => h.answer(FORGE, &fixtures::status(503)),
        Stage::Held => {
            renewed_full(&mut h);
            h.run(h.env.limits.idle);
            advance(&mut h, CONTENTS_DUE);
            h.connected(FORGE);
            let gh = h.component.test_github_mut();
            gh.held = Some(crate::installations::HeldToken {
                selected: gh.selected.expect("eligible renewal"),
                token: skein_lib::Secret::new(Box::from(NEW_CONTENTS)),
                expires: Wall::from_nanos(10_800_000_000_000),
            });
        }
    }
    h
}
fn close(value: Stage) {
    let mut h = stage(value);
    h.send(Request::Close);
    h.run(Duration::ZERO);
    assert!(h.closed);
    assert!(h.component.test_github().held.is_none());
    assert!(h.component.test_github().rows.get(&1).expect("row").api.is_none());
    assert_eq!(h.component.next_deadline(), None);
    assert_eq!(tokens(&h).len(), 1);
}
fn replace(value: Stage) {
    let mut h = stage(value);
    h.send(Request::Connect {
        forge: FORGE,
        access: fixtures::app_access(skein_fake_forge_github::keys::KEYS.first().expect("public key"), 7, 9),
    });
    h.run(Duration::ZERO);
    h.connected(FORGE);
    h.answer(FORGE, &fixtures::ok(super::github_start::SUSPENDED));
    h.run(Duration::ZERO);
    assert_eq!(h.connected_count, 2);
    assert_eq!(tokens(&h).len(), 1);
    assert!(h.component.test_github().held.is_none());
}
#[test]
fn close_while_renewing_discards_work() {
    close(Stage::Minting);
}
#[test]
fn close_during_renewal_backoff_discards_work() {
    close(Stage::Waiting);
}
#[test]
fn close_drops_held_renewed_contents() {
    close(Stage::Held);
}
#[test]
fn replacement_while_renewing_discards_work() {
    replace(Stage::Minting);
}
#[test]
fn replacement_during_renewal_backoff_discards_work() {
    replace(Stage::Waiting);
}
#[test]
fn replacement_drops_held_renewed_contents() {
    replace(Stage::Held);
}
#[test]
fn renewed_contents_shares_output_and_resumes_once() {
    let mut h = stage(Stage::Held);
    let mut renamed = row(1, false);
    renamed.login = Box::from(b"new-account".as_slice());
    commit(&mut h, Box::from([renamed, row(2, false)]));
    let mut up = Queue::with_capacity(MAX_OUT.above);
    let mut io = Queue::with_capacity(MAX_OUT.below);
    h.component.test_room(&h.env, &mut up, &mut io, 1, 0);
    assert_eq!(up.len(), 1);
    assert!(h.component.test_github().held.is_some());
    drop(up.pop());
    h.component.test_room(&h.env, &mut up, &mut io, 1, 0);
    drop(up.pop());
    h.component.test_room(&h.env, &mut up, &mut io, 1, 0);
    assert!(h.component.test_github().held.is_none());
    assert_eq!(up.len(), 1);
    match up.pop().expect("held renewal") {
        Event::GitToken { account, token, .. } => {
            assert_eq!(account.login.as_ref(), b"new-account");
            assert!(token.expose() == NEW_CONTENTS);
        }
        Event::Installation { .. }
        | Event::Fact { .. }
        | Event::Answered(_)
        | Event::Hint(_)
        | Event::ListenerLost
        | Event::Connected { .. }
        | Event::Stopped { .. }
        | Event::Closed => unreachable!("held token"),
    }
    h.component.fire(&h.env, &mut up, &mut io);
    assert!(up.is_empty());
}
#[test]
fn loss_suppresses_held_renewed_contents() {
    let mut h = stage(Stage::Held);
    accepted(&mut h, &[]);
    h.run(Duration::ZERO);
    assert_eq!(tokens(&h).len(), 1);
    assert!(h.component.test_github().held.is_none());
}
#[test]
#[expect(clippy::disallowed_macros, reason = "inspect Secret redaction during renewal and output handoff")]
fn renewal_debug_redacts_old_new_and_held_tokens() {
    for value in [Stage::Minting, Stage::Waiting, Stage::Held] {
        let h = stage(value);
        let debug = std::format!("{:?} {:?} {:?}", h.component, h.up, h.io);
        for token in [FULL, CONTENTS, NEW_FULL, NEW_CONTENTS] {
            assert!(!contains(debug.as_bytes(), token));
        }
    }
}

#[test]
fn contents_backoff_leaves_full_usable() {
    let mut h = contents_renewing();
    h.answer(FORGE, &fixtures::status(503));
    usable(&h, NEW_FULL);
    assert_eq!(h.component.test_github().deadline(), Some(Time::from_nanos(4_141_000_000_000)));
    public_ready(&h);
}
#[test]
fn loss_discards_a_written_contents_renewal() {
    let mut h = contents_renewing();
    accepted(&mut h, &[]);
    renewed_contents(&mut h);
    assert_eq!(tokens(&h).len(), 1);
    assert!(h.component.test_github().held.is_none());
    assert!(h.component.test_github().rows.get(&1).expect("Gone row").api.is_none());
}
#[test]
fn renewal_send_arms_a_fresh_request_deadline() {
    let mut h = renewing();
    h.run(Duration::from_secs(9));
    h.answer(FORGE, &fixtures::status(503));
    h.run(SECOND);
    h.connected(FORGE);
    assert_eq!(h.component.next_deadline(), Some(Time::from_nanos(3_560_000_000_000)));
}
#[test]
fn waiting_for_a_socket_consumes_the_due_alarm() {
    let mut h = ready();
    advance(&mut h, FULL_DUE);
    for _ in 0..REPEATED_FIRES {
        h.run(Duration::ZERO);
    }
    assert_eq!(h.component.next_deadline(), Some(Time::from_nanos(3_550_000_000_000)));
    assert_eq!(h.connections_opened(), 2);
    assert_eq!(requests(&h), INITIAL_REQUESTS);
}
