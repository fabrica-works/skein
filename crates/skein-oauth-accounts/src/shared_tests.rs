//! Shared writes hold the owner's lock through durable settlement.
use super::*;
use crate::Locked;
use private::Owner;
const ACCOUNT: u32 = 0;
const NEWER: u64 = 2;
const READ_GENERATION: u64 = 8;
const KEPT_GENERATION: u64 = 9;
const VALIDITY: u64 = 30;
const SHORT_VALIDITY: u64 = 5;
const ADDRESS: core::net::SocketAddr =
    core::net::SocketAddr::new(core::net::IpAddr::V4(core::net::Ipv4Addr::LOCALHOST), 31000);

#[derive(Debug, PartialEq, Eq)]
enum Seen {
    Lock,
    Unlock,
    Granted { generation: u64, valid: Duration },
    Keep(u64),
    SignedIn(u64),
    SignedOut(bool),
    Failed(Failure),
    None,
    Other,
}
fn seen(events: &mut Queue<Event>) -> Seen {
    match events.pop() {
        Some(Event::Lock { .. }) => Seen::Lock,
        Some(Event::Unlock { .. }) => Seen::Unlock,
        Some(Event::Granted { generation, valid, .. }) => Seen::Granted { generation, valid },
        Some(Event::Keep { record, .. }) => Seen::Keep(record.generation),
        Some(Event::SignedIn { generation, .. }) => Seen::SignedIn(generation),
        Some(Event::SignedOut { revoked, .. }) => Seen::SignedOut(revoked),
        Some(Event::Failed { failure, .. }) => Seen::Failed(failure),
        None => Seen::None,
        Some(
            Event::Visit { .. }
            | Event::Refused { .. }
            | Event::Expiring { .. }
            | Event::Removed { .. }
            | Event::Closed,
        ) => Seen::Other,
    }
}
fn shared_record(generation: u64) -> SavedToken {
    let mut record = tests_record(generation);
    record.refresh_token = Some(bytes::copy_of(b"refresh"));
    record
}
fn shared_component() -> Owner {
    let mut account = refresh_account(ACCOUNT, ADDRESS);
    if let Account::SignIn { shared, .. } = &mut account {
        *shared = true;
    }
    Owner::configured([account], &limits())
}
fn answer(owner: &mut Owner, record: SavedToken) {
    owner.ask(Request::Locked { account: ACCOUNT, answer: Locked::Held { record: Some(record) } });
}
#[test]
fn each_write_of_a_shared_record_locks_first() {
    struct Row {
        name: &'static str,
        request: Request,
        expected: Seen,
    }
    let rows = [
        Row { name: "refresh", request: Request::Grant { account: ACCOUNT }, expected: Seen::Lock },
        Row { name: "sign-out", request: Request::SignOut { account: ACCOUNT }, expected: Seen::Lock },
    ];
    for row in rows {
        let mut owner = shared_component();
        owner.ask(row.request);
        assert_eq!(seen(&mut owner.above), row.expected, "{}", row.name);
        assert!(owner.io.is_empty() && owner.files.is_empty(), "{} opens no write before the lock", row.name);
    }
}
#[test]
fn a_sign_ins_keep_locks_first() {
    let mut owner = signing_in();
    assert_event(owner.above.pop(), Event::Lock { account: ACCOUNT });
    assert!(owner.above.is_empty(), "no Keep before the owner lock");
}
fn signing_in() -> Owner {
    let mut account = refresh_account(ACCOUNT, ADDRESS);
    if let Account::SignIn { shared, registration, .. } = &mut account {
        *shared = true;
        registration.client_secret = Some(bytes::copy_of(b"fake secret"));
        registration.redirect_uri = bytes::copy_of(b"https://service.example/callback");
    }
    let mut owner = Owner::configured([account], &limits());
    owner.redirected(b"");
    let identity = owner.sent();
    owner.above = owner.finish_tokens(identity, private::BODY);
    owner.socket(LowerEvent::Closed { owner: identity });
    owner
}
fn lending() -> Owner {
    let mut account = refresh_account(ACCOUNT, ADDRESS);
    if let Account::SignIn { shared, keeper, .. } = &mut account {
        *shared = true;
        *keeper = Keeper::Owner { kept: Some(shared_record(NEWER)) };
    }
    Owner::configured([account], &limits())
}
#[test]
fn only_writes_lock_a_shared_record() {
    struct Row {
        name: &'static str,
        request: Request,
        expected: Seen,
    }
    let rows = [
        Row {
            name: "grant",
            request: Request::Grant { account: ACCOUNT },
            expected: Seen::Granted { generation: NEWER, valid: Duration::from_secs(VALIDITY) },
        },
        Row { name: "lend released", request: Request::Release { account: ACCOUNT }, expected: Seen::None },
    ];
    for row in rows {
        let mut owner = lending();
        owner.ask(row.request);
        assert_eq!(seen(&mut owner.above), row.expected, "{}", row.name);
        assert!(owner.io.is_empty() && owner.files.is_empty(), "{}", row.name);
    }
}
#[test]
fn a_newer_generation_of_the_same_subject_is_held_with_no_exchange() {
    let mut owner = shared_component();
    owner.ask(Request::Grant { account: ACCOUNT });
    drop(owner.above.pop());
    answer(&mut owner, shared_record(NEWER));
    assert_granted(owner.above.pop(), NEWER, VALIDITY);
    assert_event(owner.above.pop(), Event::Unlock { account: ACCOUNT });
    assert!(owner.io.is_empty() && owner.files.is_empty());
}
#[test]
fn a_newer_generation_within_the_lead_is_refreshed_under_this_lock() {
    let mut owner = shared_component();
    owner.ask(Request::Grant { account: ACCOUNT });
    drop(owner.above.pop());
    let mut record = shared_record(NEWER);
    record.expires_at = Wall::from_nanos(Duration::from_secs(SHORT_VALIDITY).as_nanos());
    answer(&mut owner, record);
    assert!(owner.above.is_empty());
    let identity = owner.sent();
    let mut events = owner.finish_tokens(identity, private::BODY);
    assert_eq!(seen(&mut events), Seen::Granted { generation: 3, valid: Duration::from_secs(VALIDITY) });
    assert_eq!(seen(&mut events), Seen::Unlock);
    assert!(events.is_empty());
}
#[test]
fn a_sign_in_keeps_above_the_generation_read() {
    let mut owner = signing_in();
    drop(owner.above.pop());
    answer(&mut owner, shared_record(READ_GENERATION));
    assert_eq!(seen(&mut owner.above), Seen::Keep(KEPT_GENERATION));
}
#[test]
fn a_busy_lock_fails_the_write_as_busy_and_keeps_the_record() {
    let mut owner = shared_component();
    owner.ask(Request::Grant { account: ACCOUNT });
    drop(owner.above.pop());
    owner.ask(Request::Locked { account: ACCOUNT, answer: Locked::Busy });
    assert_event(owner.above.pop(), Event::Failed { account: ACCOUNT, ends: Ends::Grant, failure: Failure::Busy });
    assert!(owner.above.is_empty(), "a busy answer took no lock");
    owner.ask(Request::Grant { account: ACCOUNT });
    assert_granted(owner.above.pop(), 0, SHORT_VALIDITY);
}
#[test]
fn the_lock_is_released_once_the_write_settles() {
    struct Row {
        name: &'static str,
        keeping: Keeping,
        expected: Seen,
    }
    let rows = [
        Row { name: "kept", keeping: Keeping::Kept, expected: Seen::SignedIn(KEPT_GENERATION) },
        Row { name: "failed", keeping: Keeping::NotKept, expected: Seen::Failed(Failure::NotKept) },
    ];
    for row in rows {
        let mut owner = signing_in();
        drop(owner.above.pop());
        answer(&mut owner, shared_record(READ_GENERATION));
        drop(owner.above.pop());
        owner.ask(Request::Kept { account: ACCOUNT, generation: KEPT_GENERATION, keeping: row.keeping });
        assert_eq!(seen(&mut owner.above), row.expected, "{}", row.name);
        assert_eq!(seen(&mut owner.above), Seen::Unlock, "{}", row.name);
        assert!(owner.above.is_empty(), "{}", row.name);
    }
}
#[test]
fn a_lock_wait_of_zero_is_unusable() {
    let mut profile = limits();
    profile.lock_wait = Duration::ZERO;
    let result = Component::new(List::with_capacity(0), &profile, crypto(17));
    assert_eq!(result.err(), Some(Unusable::Limits));
}

fn waiting_refresh() -> Owner {
    let mut owner = shared_component();
    owner.ask(Request::Grant { account: ACCOUNT });
    drop(owner.above.pop());
    owner
}
fn waiting_sign_out() -> Owner {
    let mut owner = shared_component();
    owner.ask(Request::SignOut { account: ACCOUNT });
    drop(owner.above.pop());
    owner
}
struct WriteRow {
    name: &'static str,
    owner: Owner,
    expected: Seen,
}
fn lock_outcome(mut row: WriteRow, record: Option<SavedToken>) {
    drop(row.owner.above.pop());
    row.owner.ask(Request::Locked { account: ACCOUNT, answer: Locked::Held { record } });
    assert_eq!(seen(&mut row.owner.above), row.expected, "{}", row.name);
}
#[test]
fn no_record_under_the_lock_ends_each_write() {
    let rows = [
        WriteRow { name: "refresh", owner: waiting_refresh(), expected: Seen::Failed(Failure::Expired) },
        WriteRow { name: "sign-out", owner: waiting_sign_out(), expected: Seen::SignedOut(false) },
        WriteRow { name: "sign-in", owner: signing_in(), expected: Seen::Keep(1) },
    ];
    for row in rows {
        lock_outcome(row, None);
    }
}
#[test]
fn another_subject_under_the_lock_ends_each_write() {
    let rows = [
        WriteRow { name: "refresh", owner: waiting_refresh(), expected: Seen::Failed(Failure::AnotherSubject) },
        WriteRow { name: "sign-out", owner: waiting_sign_out(), expected: Seen::Failed(Failure::AnotherSubject) },
        WriteRow { name: "sign-in", owner: signing_in(), expected: Seen::Keep(KEPT_GENERATION) },
    ];
    for row in rows {
        let mut record = shared_record(READ_GENERATION);
        record.subject = Some(bytes::copy_of(b"other"));
        lock_outcome(row, Some(record));
    }
}
#[test]
fn each_held_lock_is_unlocked_once() {
    struct Row {
        name: &'static str,
        requests: Box<[Request]>,
        expected: usize,
    }
    let kept = || Request::Kept { account: ACCOUNT, generation: KEPT_GENERATION, keeping: Keeping::Kept };
    let rows = [
        Row { name: "cancel", requests: Box::new([Request::Cancel { account: ACCOUNT }]), expected: 1 },
        Row { name: "removal", requests: Box::new([Request::Remove { account: ACCOUNT }, kept()]), expected: 1 },
        Row { name: "close", requests: Box::new([Request::Close, kept()]), expected: 1 },
        Row { name: "abort", requests: Box::new([Request::Abort]), expected: 1 },
    ];
    for row in rows {
        let mut owner = signing_in();
        drop(owner.above.pop());
        answer(&mut owner, shared_record(READ_GENERATION));
        drop(owner.above.pop());
        for request in row.requests {
            owner.ask(request);
        }
        for _ in 0..limits().accounts {
            owner.fire();
        }
        let mut unlocked = 0;
        while let Some(event) = owner.above.pop() {
            if event == (Event::Unlock { account: ACCOUNT }) {
                unlocked += 1;
            }
        }
        assert_eq!(unlocked, row.expected, "{}", row.name);
    }
}
#[test]
fn a_locked_for_no_pending_lock_is_unlocked_at_once() {
    let mut owner = shared_component();
    answer(&mut owner, shared_record(NEWER));
    assert_event(owner.above.pop(), Event::Unlock { account: ACCOUNT });
    assert!(owner.above.is_empty());
}
#[test]
fn a_lock_with_no_answer_within_its_wait_fails_as_busy() {
    let mut owner = waiting_refresh();
    let deadline = owner.env.now.saturating_add(owner.env.limits.lock_wait);
    assert_eq!(owner.component.next_deadline(), Some(deadline));
    owner.env.now = deadline;
    owner.fire();
    assert_event(owner.above.pop(), Event::Failed { account: ACCOUNT, ends: Ends::Grant, failure: Failure::Busy });
    assert!(owner.above.is_empty());
}
#[test]
fn a_locked_after_the_lock_wait_is_unlocked_at_once() {
    let mut owner = waiting_refresh();
    owner.env.now = owner.env.now.saturating_add(owner.env.limits.lock_wait);
    owner.fire();
    drop(owner.above.pop());
    answer(&mut owner, shared_record(NEWER));
    assert_event(owner.above.pop(), Event::Unlock { account: ACCOUNT });
    assert!(owner.above.is_empty());
}
#[test]
fn a_locked_after_closed_is_left_to_the_owner() {
    let mut owner = shared_component();
    owner.ask(Request::Close);
    drop(owner.above.pop());
    answer(&mut owner, shared_record(NEWER));
    assert!(owner.above.is_empty() && owner.io.is_empty() && owner.files.is_empty());
}
