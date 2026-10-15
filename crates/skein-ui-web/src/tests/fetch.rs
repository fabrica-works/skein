use crate::fetch::{Fetch, Report};
#[test]
fn an_answer_past_its_bound_fails_the_exchange() {
    let mut fetch = Fetch::new(1);
    let ticket = fetch.begin(7, 3).expect("room");
    assert_eq!(fetch.piece(ticket, b"four"), Some(Report::Failed { exchange: 7 }));
}
#[test]
fn an_abandoned_exchange_reports_nothing() {
    let mut fetch = Fetch::new(1);
    let ticket = fetch.begin(7, 3).expect("room");
    fetch.abandon(7);
    assert_eq!(fetch.finish(ticket, 200), None);
}
#[test]
fn an_answer_at_its_bound_is_reported_whole() {
    let mut fetch = Fetch::new(1);
    let ticket = fetch.begin(7, 3).expect("room");
    fetch.piece(ticket, b"one");
    assert_eq!(
        fetch.finish(ticket, 200),
        Some(Report::Answered { exchange: 7, status: 200, body: b"one".as_slice().into() })
    );
}
#[test]
fn an_abandoned_tickets_answer_cannot_finish_a_reused_id() {
    let mut fetch = Fetch::new(1);
    let old = fetch.begin(7, 3).expect("room");
    fetch.abandon(7);
    fetch.begin(7, 3).expect("room");
    assert_eq!(fetch.finish(old, 200), None);
}
