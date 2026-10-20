use super::{
    fixtures::{bytes, decode, limits, wall},
    server,
};
use crate::github::{
    delivery::{Delivery, Pull},
    document::{Document, Kind, Row},
    log::{self, Ask, Cursor, Logged},
    request::{self, Method, Request},
};
use skein_lib::{Overflow, Writer, bytes::copy_of};
fn row() -> log::Row {
    log::Row {
        id: 9_999_999_999_999_999_999,
        guid: [0xab; 16],
        event: copy_of(b"pull_request"),
        action: Some(copy_of(b"dequeued")),
        installation_id: Some(u64::MAX),
        repository_id: Some(7),
        status_code: 502,
        delivered_at: wall(),
        redelivery: true,
    }
}
fn payload() -> Delivery {
    Delivery::Pull {
        repository: 7,
        number: 3,
        action: Pull { action: copy_of(b"dequeued"), reason: Some(copy_of(b"CI_FAILURE")) },
    }
}
#[test]
fn github_server_delivery_log_targets_round_trip() {
    for request in [
        Request::LogPage { ask: Ask::First { validator: None }, per_page: 3 },
        Request::LogPage { ask: Ask::Next { cursor: Cursor::new(b"a/b &c").unwrap() }, per_page: 2 },
        Request::LogDelivery { id: u64::MAX, event: Box::new([]) },
    ] {
        let encoded = request::encode(&request, &limits()).unwrap();
        assert_eq!(server::route(encoded.method, &encoded.target, &limits()), Some(request));
    }
    for target in [
        b"/app/hook/deliveries?per_page=2&extra=1".as_slice(),
        b"/app/hook/deliveries?cursor=a&per_page=2",
        b"/app/hook/deliveries/01",
        b"/app/hook/deliveries?per_page=2&cursor=bad%01",
    ] {
        assert_eq!(server::route(Method::Get, target, &limits()), None);
    }
}
#[test]
fn github_server_delivery_log_documents_decode_at_every_chunk_size_and_writer_bound() {
    let row = row();
    let payload = payload();
    let mut list = Writer::new(4096);
    server::deliveries(core::slice::from_ref(&row), &mut list).unwrap();
    let list = bytes(list);
    let mut whole = Writer::new(4096);
    server::logged(&row, &payload, b"sha256=example", &mut whole).unwrap();
    let whole = bytes(whole);
    for chunk in [1, 7, 4096] {
        let (rows, result) = decode(Kind::LogRows, &list, chunk, &limits());
        assert_eq!(result, Ok(Document::Rows));
        assert_eq!(rows.as_slice(), [Row::Log(row.clone())]);
        assert_eq!(
            super::fixtures::decode_with(
                crate::github::decode::log_delivery_decoder(&row.event, &limits()),
                &whole,
                chunk,
                &limits(),
            )
            .1,
            Ok(Document::Delivery(Logged { event: row.event.clone(), delivery: payload.clone(), repository: None }))
        );
    }
    for size in [0, list.len() - 1, list.len()] {
        let mut out = Writer::new(size);
        assert_eq!(
            server::deliveries(core::slice::from_ref(&row), &mut out),
            if size < list.len() { Err(Overflow) } else { Ok(()) }
        );
        assert_eq!(out.written(), if size < list.len() { 0 } else { size });
    }
    for size in [0, whole.len() - 1, whole.len()] {
        let mut out = Writer::new(size);
        assert_eq!(
            server::logged(&row, &payload, b"sha256=example", &mut out),
            if size < whole.len() { Err(Overflow) } else { Ok(()) }
        );
        assert_eq!(out.written(), if size < whole.len() { 0 } else { size });
    }
}
