use super::*;
use crate::tests::fixtures;
use skein_lib::{List, bytes};

const CAP: u32 = 3;
const LIVE: u64 = 1;
const STRAY: u64 = 2;
const MISSING: u64 = 9;
const NAME: &[u8] = b"acme";
const NEW_NAME: &[u8] = b"renamed";
const NEXT: Duration = Duration::from_secs(1);
fn row(id: u64, login: &[u8], suspended: bool) -> document::Installation {
    document::Installation {
        id,
        login: bytes::copy_of(login),
        account: id.checked_add(20).expect("fixture account"),
        kind: document::Account::Organization,
        created: Wall::EPOCH,
        suspended,
    }
}
fn read(gh: &mut GitHub, rows: Box<[document::Installation]>) {
    gh.begin_read(NEXT);
    page(gh, rows);
    // This helper represents delivery of the single fact before commit.
    drop(gh.output(fixtures::FORGE));
    gh.commit();
}
fn page(gh: &mut GitHub, rows: Box<[document::Installation]>) {
    match &mut gh.read {
        Read::Reading(reading) => {
            reading.page(rows, &gh.rows, &gh.accepted, fixtures::shared().name_bytes).expect("page");
        }
        Read::Idle | Read::Changes(_) | Read::Waiting { .. } => unreachable!("reading"),
    }
}
fn changes(gh: &mut GitHub) -> List<(u64, Option<Account>, Change)> {
    let mut events = List::with_capacity(128);
    for _ in 0_u32..128 {
        match gh.output(fixtures::FORGE) {
            Some(Event::Installation { installation, account, change, .. }) => {
                events.push((installation, account, change)).expect("fixture changes");
            }
            Some(
                Event::Fact { .. }
                | Event::GitToken { .. }
                | Event::Answered(_)
                | Event::Hint(_)
                | Event::ListenerLost
                | Event::Connected { .. }
                | Event::Stopped { .. }
                | Event::Closed,
            ) => unreachable!("changes"),
            None => break,
        }
    }
    gh.read = Read::Idle;
    events
}
fn later(
    before: Box<[document::Installation]>,
    after: Box<[document::Installation]>,
) -> List<(u64, Option<Account>, Change)> {
    let mut gh = GitHub::new(CAP);
    read(&mut gh, before);
    drop(changes(&mut gh));
    read(&mut gh, after);
    changes(&mut gh)
}
fn fact(gh: &mut GitHub) -> Option<Fact> {
    match gh.output(fixtures::FORGE) {
        Some(Event::Fact { fact, .. }) => Some(fact),
        None => None,
        Some(
            Event::Installation { .. }
            | Event::GitToken { .. }
            | Event::Answered(_)
            | Event::Hint(_)
            | Event::ListenerLost
            | Event::Connected { .. }
            | Event::Stopped { .. }
            | Event::Closed,
        ) => unreachable!("fact"),
    }
}
fn expected(id: u64, name: &[u8], change: Change) -> (u64, Option<Account>, Change) {
    (id, Some(Account { id: id.checked_add(20).expect("fixture account"), login: bytes::copy_of(name) }), change)
}
#[test]
fn later_read_reports_creation() {
    assert_eq!(
        later(Box::from([]), Box::from([row(LIVE, NAME, false)])).as_slice(),
        [expected(LIVE, NAME, Change::Created)]
    );
}
#[test]
fn later_read_reports_deletion() {
    assert_eq!(
        later(Box::from([row(LIVE, NAME, false)]), Box::from([])).as_slice(),
        [expected(LIVE, NAME, Change::Deleted)]
    );
}
#[test]
fn later_read_reports_suspension() {
    assert_eq!(
        later(Box::from([row(LIVE, NAME, false)]), Box::from([row(LIVE, NAME, true)])).as_slice(),
        [expected(LIVE, NAME, Change::Suspended)]
    );
}
#[test]
fn later_read_reports_unsuspension() {
    assert_eq!(
        later(Box::from([row(LIVE, NAME, true)]), Box::from([row(LIVE, NAME, false)])).as_slice(),
        [expected(LIVE, NAME, Change::Unsuspended)]
    );
}
#[test]
fn later_read_reports_rename() {
    assert_eq!(
        later(Box::from([row(LIVE, NAME, false)]), Box::from([row(LIVE, NEW_NAME, false)])).as_slice(),
        [expected(LIVE, NEW_NAME, Change::Renamed)]
    );
}
#[test]
fn unchanged_read_is_silent() {
    assert!(later(Box::from([row(LIVE, NAME, false)]), Box::from([row(LIVE, NAME, false)])).is_empty());
}
#[test]
fn lifecycle_precedes_rename() {
    assert_eq!(
        later(Box::from([row(LIVE, NAME, false)]), Box::from([row(LIVE, NEW_NAME, true)])).as_slice(),
        [expected(LIVE, NEW_NAME, Change::Suspended), expected(LIVE, NEW_NAME, Change::Renamed)]
    );
}
#[test]
fn new_and_deleted_rows_only_report_membership() {
    assert_eq!(
        later(Box::from([row(LIVE, NAME, true)]), Box::from([row(STRAY, NEW_NAME, true)])).as_slice(),
        [expected(LIVE, NAME, Change::Deleted), expected(STRAY, NEW_NAME, Change::Created)]
    );
}
#[test]
fn known_missing_account_stays_some() {
    let mut gh = GitHub::new(CAP);
    gh.accept(&[LIVE]);
    read(&mut gh, Box::from([row(LIVE, NAME, true)]));
    drop(changes(&mut gh));
    read(&mut gh, Box::from([]));
    drop(changes(&mut gh));
    gh.accept(&[LIVE, MISSING]);
    read(&mut gh, Box::from([]));
    assert_eq!(changes(&mut gh).as_slice(), [expected(LIVE, NAME, Change::Deleted), (MISSING, None, Change::Deleted)]);
}
#[test]
fn equal_reordered_accepted_set_keeps_its_generation() {
    let mut gh = GitHub::new(CAP);
    gh.accept(&[STRAY, LIVE]);
    gh.begin_read(NEXT);
    gh.accept(&[LIVE, STRAY, LIVE, 0]);
    assert_eq!(gh.generation, 1);
    assert!(!gh.stale_read());
}
#[test]
fn later_page_keeps_accepted_row_before_stray() {
    let mut gh = GitHub::new(1);
    gh.accept(&[LIVE]);
    gh.begin_read(NEXT);
    page(&mut gh, Box::from([row(STRAY, NAME, false)]));
    page(&mut gh, Box::from([row(LIVE, NEW_NAME, true)]));
    assert_eq!(fact(&mut gh), Some(Fact::Unkept { installation: STRAY }));
    assert!(gh.output(fixtures::FORGE).is_none());
    gh.commit();
    assert_eq!(changes(&mut gh).as_slice(), [expected(LIVE, NEW_NAME, Change::Suspended)]);
    assert_eq!(gh.rows.len(), 1);
}
#[test]
fn multiple_overflows_across_pages_emit_one_fact_per_read() {
    let mut gh = GitHub::new(1);
    for _ in 0_u32..2 {
        gh.begin_read(NEXT);
        page(&mut gh, Box::from([row(LIVE, NAME, false), row(STRAY, NAME, false)]));
        assert_eq!(fact(&mut gh), Some(Fact::Unkept { installation: STRAY }));
        page(&mut gh, Box::from([row(MISSING, NAME, false)]));
        assert!(gh.output(fixtures::FORGE).is_none());
        gh.commit();
        drop(changes(&mut gh));
    }
}
#[test]
fn retained_strays_follow_encounter_order() {
    let mut gh = GitHub::new(2);
    gh.accept(&[LIVE]);
    gh.begin_read(NEXT);
    page(&mut gh, Box::from([row(MISSING, NAME, false), row(STRAY, NAME, false)]));
    page(&mut gh, Box::from([row(LIVE, NAME, false)]));
    drop(gh.output(fixtures::FORGE));
    gh.commit();
    assert!(gh.rows.contains_key(&MISSING));
    assert!(!gh.rows.contains_key(&STRAY));
}
#[test]
fn seen_but_evicted_is_not_deleted() {
    let mut gh = GitHub::new(1);
    read(&mut gh, Box::from([row(STRAY, NAME, false)]));
    drop(changes(&mut gh));
    gh.accept(&[LIVE]);
    gh.begin_read(NEXT);
    page(&mut gh, Box::from([row(STRAY, NAME, false), row(LIVE, NAME, true)]));
    drop(gh.output(fixtures::FORGE));
    gh.commit();
    assert_eq!(changes(&mut gh).as_slice(), [expected(LIVE, NAME, Change::Created)]);
}
#[test]
fn rename_preserves_routing_and_moves_no_token_into_staging() {
    let mut gh = GitHub::new(CAP);
    gh.accept(&[LIVE]);
    read(&mut gh, Box::from([row(LIVE, NAME, false)]));
    drop(changes(&mut gh));
    let token = b"artificial-api";
    let before = gh.rows.get_mut(&LIVE).expect("live row");
    before.api = Some(Secret::new(Box::from(token.as_slice())));
    before.api_expires = Wall::from_nanos(1000);
    before.git_expires = Wall::from_nanos(2000);
    gh.begin_read(NEXT);
    page(&mut gh, Box::from([row(LIVE, NEW_NAME, false)]));
    assert!(gh.rows.get(&LIVE).expect("committed").api.as_ref().expect("token").expose() == token);
    gh.commit();
    let after = gh.rows.get(&LIVE).expect("renamed");
    assert_eq!(
        (after.account.id, after.api_expires, after.git_expires),
        (21, Wall::from_nanos(1000), Wall::from_nanos(2000))
    );
    assert!(after.api.as_ref().expect("preserved").expose() == token);
}
#[test]
fn missing_id_deletion_deduplicates_old_table_deletion() {
    let mut gh = GitHub::new(CAP);
    read(&mut gh, Box::from([row(LIVE, NAME, true)]));
    drop(changes(&mut gh));
    gh.accept(&[LIVE, MISSING]);
    read(&mut gh, Box::from([]));
    assert_eq!(changes(&mut gh).as_slice(), [expected(LIVE, NAME, Change::Deleted), (MISSING, None, Change::Deleted)]);
}
#[test]
fn changed_set_rearms_missing_id_detection() {
    let mut gh = GitHub::new(CAP);
    gh.accept(&[MISSING]);
    read(&mut gh, Box::from([]));
    drop(changes(&mut gh));
    gh.accept(&[MISSING, LIVE]);
    read(&mut gh, Box::from([]));
    assert_eq!(changes(&mut gh).as_slice(), [(LIVE, None, Change::Deleted), (MISSING, None, Change::Deleted)]);
}
#[test]
fn committed_cursor_keeps_its_accepted_generation() {
    let mut gh = GitHub::new(CAP);
    gh.accept(&[LIVE]);
    read(&mut gh, Box::from([row(LIVE, NAME, true), row(STRAY, NAME, true)]));
    gh.accept(&[STRAY]);
    assert_eq!(
        changes(&mut gh).as_slice(),
        [expected(LIVE, NAME, Change::Suspended), expected(STRAY, NAME, Change::Created)]
    );
}

#[test]
fn mint_fixes_due_on_the_monotonic_clock() {
    let mut env = skein_lib::Env {
        now: Time::from_nanos(100),
        wall: Wall::from_nanos(1000),
        limits: crate::Limits { renew_before: Duration::from_nanos(200), ..fixtures::limits() },
    };
    let mut clock = Clock::minted(Wall::from_nanos(1600), &env);
    assert_eq!((clock.expiry, clock.due), (Time::from_nanos(700), Some(Time::from_nanos(500))));
    for wall in [Wall::from_nanos(9000), Wall::EPOCH] {
        env.wall = wall;
        env.now = Time::from_nanos(400);
        clock.fire(env.now);
        assert_eq!((clock.expiry, clock.due), (Time::from_nanos(700), Some(Time::from_nanos(500))));
    }
}
#[test]
fn token_times_saturate_at_the_end_of_time() {
    assert_eq!(
        token_times(Wall::from_nanos(1000), Time::from_nanos(u64::MAX - 10), Wall::EPOCH, Duration::from_nanos(20)),
        (Time::from_nanos(u64::MAX), Time::from_nanos(u64::MAX - 20))
    );
}
#[test]
fn short_life_due_saturates_at_the_clock_origin() {
    assert_eq!(
        token_times(Wall::from_nanos(1010), Time::from_nanos(100), Wall::from_nanos(1000), Duration::from_nanos(200)),
        (Time::from_nanos(110), Time::ZERO)
    );
}
#[test]
fn elapsed_wall_life_saturates_to_zero() {
    assert_eq!(
        token_times(Wall::EPOCH, Time::from_nanos(100), Wall::from_nanos(1000), Duration::from_nanos(20)),
        (Time::from_nanos(100), Time::from_nanos(80))
    );
}
#[test]
fn due_is_consumed_once_into_pending_work() {
    let mut clock = Clock { expiry: Time::from_nanos(100), due: Some(Time::from_nanos(80)), pending: false };
    clock.fire(Time::from_nanos(79));
    assert!(!clock.pending);
    clock.fire(Time::from_nanos(80));
    assert!(clock.pending);
    assert_eq!(clock.due, None);
    clock.take();
    clock.fire(Time::from_nanos(200));
    assert!(!clock.pending);
}
#[test]
fn selection_orders_installations_then_full_before_contents() {
    let mut gh = GitHub::new(CAP);
    gh.accept(&[STRAY, LIVE]);
    read(&mut gh, Box::from([row(STRAY, NAME, false), row(LIVE, NAME, false)]));
    let first = gh.rows.first().expect("id order");
    assert_eq!((*first.0, first.1.scope()), (LIVE, Some(Scope::Full)));
    let first = gh.rows.get_mut(&LIVE).expect("first");
    first.full.take();
    first.state = Tokens::Current;
    assert_eq!(first.scope(), Some(Scope::Contents));
    first.contents.take();
    assert_eq!(first.scope(), None);
}
