use super::fixtures::{Call, FakePlatform, exchange};
use crate::{
    store::{Record, Store},
    transport::{Carried, Requests, carry},
};

fn writes() -> Vec<Record> {
    vec![Record { key: b"key".as_slice().into(), value: b"record".as_slice().into() }]
}
#[test]
fn writes_go_before_exchanges() {
    let mut platform = FakePlatform::default();
    let requests = Requests { writes: writes(), exchanges: vec![exchange(1)], ..Requests::default() };
    carry(requests, &mut Store::new(b"notes-", None), &mut platform, &mut Vec::new());
    assert_eq!(platform.calls, vec![Call::Write(b"notes-a2V5".to_vec(), b"cmVjb3Jk".to_vec()), Call::Send(1),]);
}
#[test]
fn a_refused_write_fails_every_exchange_of_its_step() {
    let mut platform = FakePlatform { quota: Some(0), ..FakePlatform::default() };
    let requests = Requests { writes: writes(), exchanges: vec![exchange(1), exchange(2)], ..Requests::default() };
    let mut reports = Vec::new();
    carry(requests, &mut Store::new(b"notes-", None), &mut platform, &mut reports);
    assert_eq!(
        reports,
        vec![
            Carried::Refused { key: b"key".as_slice().into() },
            Carried::Failed { exchange: 1 },
            Carried::Failed { exchange: 2 }
        ]
    );
}
#[test]
fn a_refused_write_sends_no_exchange() {
    let mut platform = FakePlatform::default();
    let requests = Requests { writes: writes(), exchanges: vec![exchange(1), exchange(2)], ..Requests::default() };
    carry(requests, &mut Store::new(b"notes-", Some(0)), &mut platform, &mut Vec::new());
    assert_eq!(platform.calls, vec![]);
}
#[test]
fn a_refused_write_cancels_the_navigation() {
    let mut platform = FakePlatform::default();
    let requests = Requests { writes: writes(), navigate: Some(b"/away".as_slice().into()), ..Requests::default() };
    carry(requests, &mut Store::new(b"notes-", Some(0)), &mut platform, &mut Vec::new());
    assert_eq!(platform.calls, vec![]);
}
#[test]
fn the_navigation_goes_after_the_writes() {
    let mut platform = FakePlatform::default();
    let requests = Requests { writes: writes(), navigate: Some(b"/away".as_slice().into()), ..Requests::default() };
    carry(requests, &mut Store::new(b"notes-", None), &mut platform, &mut Vec::new());
    assert_eq!(
        platform.calls,
        vec![Call::Write(b"notes-a2V5".to_vec(), b"cmVjb3Jk".to_vec()), Call::Navigate(b"/away".to_vec()),]
    );
}
#[test]
fn removals_go_before_exchanges() {
    let mut platform = FakePlatform::default();
    let requests =
        Requests { removes: vec![b"key".as_slice().into()], exchanges: vec![exchange(1)], ..Requests::default() };
    carry(requests, &mut Store::new(b"notes-", None), &mut platform, &mut Vec::new());
    assert_eq!(platform.calls, vec![Call::Remove(b"notes-a2V5".to_vec()), Call::Send(1)]);
}
#[test]
fn a_written_record_is_reported() {
    let mut reports = Vec::new();
    let requests = Requests { writes: writes(), ..Requests::default() };
    carry(requests, &mut Store::new(b"notes-", None), &mut FakePlatform::default(), &mut reports);
    assert_eq!(reports, vec![Carried::Written { key: b"key".as_slice().into() }]);
}
