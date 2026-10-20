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
#[test]
fn a_piece_past_its_bound_comes_in_parts() {
    let mut fetch = crate::fetch::Fetch::new(1);
    let ticket = fetch.open_stream(2);
    let reports = vec![fetch.stream_piece(ticket, b"abcde".as_slice().into()), fetch.read_on(), fetch.read_on()];
    assert_eq!(
        reports,
        vec![
            Some(crate::fetch::Report::StreamPiece { bytes: b"ab".as_slice().into() }),
            Some(crate::fetch::Report::StreamPiece { bytes: b"cd".as_slice().into() }),
            Some(crate::fetch::Report::StreamPiece { bytes: b"e".as_slice().into() }),
        ]
    );
}

#[test]
fn an_old_streams_end_cannot_end_its_replacement() {
    let mut fetch = crate::fetch::Fetch::new(1);
    let old = fetch.open_stream(2);
    fetch.open_stream(2);
    assert_eq!(fetch.end_stream(old), None);
}

#[test]
fn a_stream_refused_reports_its_status_and_ends() {
    let mut fetch = crate::fetch::Fetch::new(1);
    let ticket = fetch.open_stream(2);
    assert_eq!(fetch.refuse_stream(ticket, 401), Some(crate::fetch::Report::StreamRefused { status: 401 }));
    assert_eq!(fetch.stream_ticket(), None);
    assert_eq!(fetch.end_stream(ticket), None);
}

#[test]
fn an_old_streams_refusal_cannot_end_its_replacement() {
    let mut fetch = crate::fetch::Fetch::new(1);
    let old = fetch.open_stream(2);
    let new = fetch.open_stream(2);
    assert_eq!(fetch.refuse_stream(old, 401), None);
    assert_eq!(fetch.stream_ticket(), Some(new));
}
