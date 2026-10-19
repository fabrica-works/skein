//! Each admission's number and refusal's reason, and keys given by the asker (lib.md, 12).

use super::{FIRST_OWNER, Owner, SECOND_OWNER, limits, live, owner, sent};
use crate::{
    RequestAdmission, RequestAnswered, RequestEnvelope, RequestKey, RequestLimits, RequestNumber, RequestOut,
    RequestRecord, RequestRefused, RequestTable, Time, Wall,
};

/// The scope `live` confirms.
const SCOPE: u64 = 7;
/// One declared byte, of the ten `limits` admits.
const SIZE: u64 = 1;
/// A record's key, as the asker's store hands it back at a restart.
const RESTORED: RequestKey = [42; 16];
/// The asker's stable name for its request.
const NAMED: RequestKey = [0xE1; 16];
/// One request holds the only slot for the refusal checks.
const ONE_REQUEST: u32 = 1;

fn record() -> RequestRecord<u64> {
    RequestRecord { key: RESTORED, scope: SCOPE, first_sent: None, request: 11 }
}

fn one_held() -> RequestTable<Owner, u64> {
    let mut table = RequestTable::new(&RequestLimits { requests: ONE_REQUEST, ..limits() }, 1);
    table.restore(Owner::Restored(RESTORED), SIZE, record()).expect("fits");
    table
}

fn retired_given() -> (RequestTable<Owner, u64>, RequestAdmission) {
    let mut table = live();
    let admission = table.ask_given(owner(FIRST_OWNER), SCOPE, SIZE, NAMED, 10).expect("fits");
    let attempt = sent(&mut table, Time::ZERO, Wall::EPOCH).expect("send");
    assert_eq!(table.answered(Time::ZERO, attempt, RequestEnvelope::Final), RequestAnswered::Final(owner(FIRST_OWNER)));
    (table, admission)
}

fn reclaim_given(table: &mut RequestTable<Owner, u64>) {
    {
        let mut visit = table.take(Time::ZERO, Wall::EPOCH);
        match visit.next_out().expect("retired record") {
            RequestOut::Erase(key) => assert_eq!(key, NAMED),
            RequestOut::Save(_) | RequestOut::Send { .. } => unreachable!("retired request"),
        }
        assert!(visit.next_out().is_none());
    }
    table.reclaim();
}

#[test]
fn each_admission_by_ask_or_restore_is_numbered_one_above_the_last() {
    let mut table = RequestTable::<Owner, u64>::new(&limits(), 1);
    let first = table.ask(owner(FIRST_OWNER), SCOPE, SIZE, 10).expect("fits");
    let restored = table.restore(Owner::Restored(RESTORED), SIZE, record()).expect("fits");
    let second = table.ask(owner(SECOND_OWNER), SCOPE, SIZE, 12).expect("fits");
    assert_eq!(
        [first.number, restored.number, second.number],
        [RequestNumber::new(1), RequestNumber::new(2), RequestNumber::new(3)],
        "admissions are numbered from 1, by ask and by restore alike"
    );
}

#[test]
fn a_restore_of_a_held_key_is_refused_held_whatever_the_room() {
    let mut table = one_held();
    assert_eq!(table.restore(Owner::Restored(RESTORED), SIZE, record()), Err(RequestRefused::Held(record())));
    assert_eq!(table.len(), ONE_REQUEST);
}

#[test]
fn a_restore_past_the_count_is_refused_full_with_its_record() {
    let mut table = one_held();
    let other = RequestRecord { key: [43; 16], ..record() };
    assert_eq!(table.restore(owner(SECOND_OWNER), SIZE, other.clone()), Err(RequestRefused::Full(other)));
}

#[test]
fn not_kept_under_another_admission_s_number_hands_back_nothing() {
    let mut table = RequestTable::<Owner, u64>::new(&limits(), 1);
    let first = table.ask(owner(FIRST_OWNER), SCOPE, SIZE, 10).expect("fits");
    let second = table.ask(owner(SECOND_OWNER), SCOPE, SIZE, 12).expect("fits");
    assert_eq!(table.not_kept(first.key, second.number), None);
    assert_eq!(table.not_kept(second.key, first.number), None);
    let mut visit = table.take(Time::ZERO, Wall::EPOCH);
    for key in [first.key, second.key] {
        match visit.next_out().expect("save remains") {
            RequestOut::Save(record) => assert_eq!(record.key, key),
            RequestOut::Erase(_) | RequestOut::Send { .. } => unreachable!("link starts down"),
        }
    }
    assert!(visit.next_out().is_none());
}

#[test]
fn a_given_key_is_admitted_and_saved_under_that_key() {
    let mut table = RequestTable::<Owner, u64>::new(&limits(), 1);
    assert_eq!(
        table.ask_given(owner(FIRST_OWNER), SCOPE, SIZE, NAMED, 10),
        Ok(RequestAdmission { key: NAMED, number: RequestNumber::new(1) })
    );
    let mut visit = table.take(Time::ZERO, Wall::EPOCH);
    match visit.next_out().expect("given record") {
        RequestOut::Save(record) => {
            assert_eq!(record, &RequestRecord { key: NAMED, scope: SCOPE, first_sent: None, request: 10 });
        }
        RequestOut::Erase(_) | RequestOut::Send { .. } => unreachable!("link starts down"),
    }
    assert!(visit.next_out().is_none());
}

#[test]
fn a_given_key_is_refused_held_while_its_entry_is_retired() {
    let (mut table, _) = retired_given();
    assert_eq!(table.ask_given(owner(SECOND_OWNER), SCOPE, SIZE, NAMED, 20), Err(RequestRefused::Held(20)));
}

#[test]
fn a_given_key_is_admitted_again_once_reclaimed_under_a_later_number() {
    let (mut table, first) = retired_given();
    reclaim_given(&mut table);
    let second = table.ask_given(owner(SECOND_OWNER), SCOPE, SIZE, NAMED, 10).expect("reclaimed");
    assert_eq!(second.key, NAMED);
    assert!(second.number > first.number);
}

#[test]
fn a_stale_not_kept_never_retires_a_later_holder_of_its_key() {
    let mut table = RequestTable::<Owner, u64>::new(&limits(), 1);
    let first = table.ask_given(owner(FIRST_OWNER), SCOPE, SIZE, NAMED, 10).expect("fits");
    assert_eq!(table.not_kept(NAMED, first.number), Some(owner(FIRST_OWNER)));
    reclaim_given(&mut table);
    table.ask_given(owner(SECOND_OWNER), SCOPE, SIZE, NAMED, 10).expect("reclaimed");
    assert_eq!(table.not_kept(NAMED, first.number), None);
    assert_eq!(table.owner(NAMED), Some(owner(SECOND_OWNER)));
}

#[test]
fn a_held_given_key_is_refused_held_before_full() {
    let mut table = RequestTable::<Owner, u64>::new(&RequestLimits { requests: ONE_REQUEST, ..limits() }, 1);
    table.ask_given(owner(FIRST_OWNER), SCOPE, SIZE, NAMED, 10).expect("fits");
    assert_eq!(table.ask_given(owner(SECOND_OWNER), SCOPE, SIZE, NAMED, 20), Err(RequestRefused::Held(20)));
    assert_eq!(table.len(), ONE_REQUEST);
}

#[test]
fn new_keys_skip_a_held_given_key() {
    let mut original = RequestTable::<Owner, u64>::new(&limits(), 8);
    let key = original.ask(owner(FIRST_OWNER), SCOPE, SIZE, 10).expect("fits").key;
    let mut table = RequestTable::<Owner, u64>::new(&limits(), 8);
    table.ask_given(owner(FIRST_OWNER), SCOPE, SIZE, key, 10).expect("fits");
    let fresh = table.ask(owner(SECOND_OWNER), SCOPE, SIZE, 11).expect("fits").key;
    assert_ne!(fresh, key);
}
