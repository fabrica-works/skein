use super::{
    fixtures::{decode, limits, wall},
    header::{self, Head},
};
use crate::{
    api,
    github::{
        Delivery,
        decode::Malformed,
        document::{Document, Kind, Row},
        log::{Ask, Cursor, Listed, Logged},
        plans::{
            Body, Heard,
            plan::Bounds,
            start::{LogDelivery, LogPage, Step},
        },
        request::{self, Auth, Method, Request},
    },
};
use skein_lib::{Writer, bytes::copy_of};

const GUID: &[u8] = b"01234567-89ab-cdef-0123-456789abcdef";
const PING: &[u8] = br#"{"id":9999999999999999999,"guid":"01234567-89ab-cdef-0123-456789abcdef","event":"ping","action":null,"installation_id":null,"repository_id":null,"status_code":200,"delivered_at":"2026-10-20T08:00:00Z","redelivery":false}"#;
const ISSUE: &[u8] =
    br#"{"action":"opened","issue":{"number":3,"body":"hello"},"repository":{"id":7,"ignored":{"nested":[null]}}}"#;
fn bounds() -> Bounds {
    Bounds { page_size: 4, rows: 4, inner: 4, answer_bytes: 4096, body_bytes: 4096 }
}
fn list(row: &[u8], count: u32) -> Box<[u8]> {
    let mut out = Writer::new(4096);
    out.put(b"[").unwrap();
    for i in 0..count {
        if i != 0 {
            out.put(b",").unwrap();
        }
        out.put(row).unwrap();
    }
    out.put(b"]").unwrap();
    super::fixtures::bytes(out)
}
fn head(link: Option<&[u8]>) -> Head {
    let mut head = Head::new();
    if let Some(link) = link {
        header::field(&mut head, b"Link", link, &limits());
    }
    header::field(&mut head, b"ETag", b"W/\"example-app\"", &limits());
    head
}
fn page(ask: Ask, head: Head, input: &[u8]) -> Result<Listed, api::Error> {
    let mut plan = LogPage::new(ask);
    let Step::Send(_) = plan.begin(bounds()).unwrap() else { panic!("page request") };
    if let Some(Step::Done(result)) = plan.heard(Heard::Head { status: 200, head }, bounds()).unwrap() {
        return result;
    }
    let (rows, result) = decode(Kind::LogRows, input, 1, &limits());
    for row in rows.as_slice() {
        if let Some(Step::Done(result)) = plan.heard(Heard::Row(row.clone()), bounds()).unwrap() {
            return result;
        }
    }
    let body = match result {
        Ok(doc) => Body::Document(doc),
        Err(error) => Body::Malformed(error),
    };
    match plan.heard(Heard::Done(body), bounds()).unwrap().unwrap() {
        Step::Done(result) => result,
        Step::Send(_) => panic!("one page only"),
    }
}
#[test]
fn log_rows_keep_nineteen_digit_and_maximum_ids_and_a_ping_without_owners() {
    const MAX_PING: &[u8] = br#"{"id":18446744073709551615,"guid":"01234567-89ab-cdef-0123-456789abcdef","event":"ping","action":null,"installation_id":null,"repository_id":null,"status_code":200,"delivered_at":"2026-10-20T08:00:00Z","redelivery":false}"#;
    for (id, input) in [(9_999_999_999_999_999_999, PING), (u64::MAX, MAX_PING)] {
        for chunk in [1, 7, 4096] {
            let (rows, result) = decode(Kind::LogRows, &list(input, 1), chunk, &limits());
            assert_eq!(result, Ok(Document::Rows));
            let Row::Log(row) = &rows.as_slice()[0] else { panic!("log row") };
            assert_eq!(row.id, id);
            assert_eq!(row.guid, crate::github::guid::read(GUID).unwrap());
            assert_eq!(row.event.as_ref(), b"ping");
            assert_eq!(row.action, None);
            assert_eq!(row.installation_id, None);
            assert_eq!(row.repository_id, None);
            assert_eq!(row.delivered_at, wall());
            assert_eq!(row.status_code, 200);
            assert!(!row.redelivery);
        }
    }
}
#[test]
fn log_page_returns_the_next_cursor_and_first_page_validator() {
    for link in
        [None, Some(b"<https://api.example.test/app/hook/deliveries?cursor=next%2Fpage>; rel=\"next\"".as_slice())]
    {
        let Listed::Page(page) = page(Ask::First { validator: None }, head(link), &list(PING, 1)).unwrap() else {
            panic!("page")
        };
        assert_eq!(page.rows.len(), 1);
        assert_eq!(
            page.next,
            match link {
                Some(_) => Cursor::new(b"next/page"),
                None => None,
            }
        );
        assert_eq!(page.validator.unwrap().as_bytes(), b"W/\"example-app\"");
    }
    let Listed::Page(page) = page(Ask::Next { cursor: Cursor::new(b"older").unwrap() }, head(None), b"[]").unwrap()
    else {
        panic!("page")
    };
    assert_eq!(page.validator, None);
}
#[test]
fn log_cursor_accepts_128_printable_bytes_and_refuses_long_or_unprintable_values() {
    assert_eq!(Cursor::new(&[b'x'; 128]).unwrap().as_bytes(), &[b'x'; 128]);
    for value in [b"a\x1fb".as_slice(), b"a\x7fb", b"a\x80b", &[b'x'; 129]] {
        assert_eq!(Cursor::new(value), None);
    }
    for (cursor, valid) in [
        (copy_of(&[b'x'; 128]), true),
        (copy_of(&[b'x'; 129]), false),
        (copy_of(b"bad%01cursor"), false),
        (copy_of(b"bad\x01cursor"), false),
    ] {
        let mut out = Writer::new(1024);
        out.put(b"<https://api.example.test/?cursor=").unwrap();
        out.put(&cursor).unwrap();
        out.put(b">; rel=\"next\"").unwrap();
        let result = page(Ask::First { validator: None }, head(Some(&super::fixtures::bytes(out))), b"[]");
        assert_eq!(result.is_ok(), valid);
        if !valid {
            assert_eq!(result, Err(api::Error::InvalidAnswer));
        }
    }
}
#[test]
fn log_listing_past_inner_is_too_large() {
    let (_, result) = decode(Kind::LogRows, &list(PING, limits().inner + 1), 7, &limits());
    assert_eq!(result, Err(Malformed::Bound { field: b"deliveries" }));
    assert_eq!(page(Ask::First { validator: None }, head(None), &list(PING, 5)), Err(api::Error::TooLarge));
}
#[test]
fn log_requests_use_the_jwt_and_only_next_pages_have_a_cursor() {
    let validator = api::Validator::new(b"W/\"example-app\"").unwrap();
    for (ask, target, expected) in [
        (Ask::First { validator: None }, b"/app/hook/deliveries?per_page=4".as_slice(), None),
        (
            Ask::First { validator: Some(validator.clone()) },
            b"/app/hook/deliveries?per_page=4",
            Some(validator.clone()),
        ),
        (
            Ask::Next { cursor: Cursor::new(b"a/b &c").unwrap() },
            b"/app/hook/deliveries?per_page=4&cursor=a%2Fb%20%26c",
            None,
        ),
    ] {
        let mut plan = LogPage::new(ask);
        let Step::Send(exchange) = plan.begin(bounds()).unwrap() else { panic!("request") };
        let encoded = request::encode(&exchange.request, &limits()).unwrap();
        assert_eq!(encoded.method, Method::Get);
        assert_eq!(encoded.target.as_ref(), target);
        assert_eq!(encoded.body, None);
        assert_eq!(encoded.auth, Auth::App);
        assert_eq!(encoded.validator, expected);
        let fields = request::fields(&encoded);
        assert_eq!(fields.len(), if expected.is_some() { 3 } else { 2 });
        if let Some(validator) = expected {
            assert_eq!(fields[2], (b"If-None-Match".as_slice(), validator.as_bytes()));
        }
    }
    let mut plan = LogDelivery::new(u64::MAX);
    let Step::Send(exchange) = plan.begin(bounds()).unwrap() else { panic!("request") };
    assert_eq!(crate::github::decode::kind(&exchange.request), Some(Kind::LogDelivery));
    let encoded = request::encode(&exchange.request, &limits()).unwrap();
    assert_eq!(encoded.auth, Auth::App);
    assert_eq!(encoded.method, Method::Get);
    assert_eq!(encoded.target.as_ref(), b"/app/hook/deliveries/18446744073709551615");
    assert_eq!(encoded.body, None);
}
#[test]
fn log_conditional_304_is_unchanged_and_distinct_from_an_empty_page() {
    for ask in [
        Ask::First { validator: Some(api::Validator::new(b"\"example-app\"").unwrap()) },
        Ask::First { validator: None },
        Ask::Next { cursor: Cursor::new(b"next").unwrap() },
    ] {
        let conditional = match &ask {
            Ask::First { validator: Some(_) } => true,
            Ask::First { validator: None } | Ask::Next { .. } => false,
        };
        let mut plan = LogPage::new(ask);
        plan.begin(bounds()).unwrap();
        let result = plan.heard(Heard::Head { status: 304, head: Head::new() }, bounds()).unwrap();
        assert_eq!(
            result,
            Some(Step::Done(if conditional { Ok(Listed::Unchanged) } else { Err(api::Error::InvalidAnswer) }))
        );
    }
    let Listed::Page(empty) = page(Ask::First { validator: None }, head(None), b"[]").unwrap() else {
        panic!("empty page")
    };
    assert!(empty.rows.is_empty());
}
fn wrapped(payload: &[u8], payload_first: bool, event: &[u8]) -> Box<[u8]> {
    let mut out = Writer::new(4096);
    out.put(b"{").unwrap();
    if payload_first {
        out.put(b"\"request\":{\"payload\":").unwrap();
        out.put(payload).unwrap();
        out.put(b",").unwrap();
    } else {
        out.put(b"\"event\":\"issues\",\"request\":{").unwrap();
    }
    out.put(b"\"headers\":{\"X-GitHub-Event\":\"").unwrap();
    out.put(event).unwrap();
    out.put(b"\",\"X-GitHub-Delivery\":\"01234567-89ab-cdef-0123-456789abcdef\"}").unwrap();
    if !payload_first {
        out.put(b",\"payload\":").unwrap();
        out.put(payload).unwrap();
    }
    out.put(b"}").unwrap();
    if payload_first {
        out.put(b",\"event\":\"issues\"").unwrap();
    }
    out.put(b",\"response\":{\"payload\":{\"nested\":[false,{\"body\":\"ignored\"}]}}}").unwrap();
    super::fixtures::bytes(out)
}
#[test]
fn logged_delivery_reuses_the_pushed_decoder_and_skips_response_in_any_member_order() {
    let expected = Logged {
        event: copy_of(b"issues"),
        delivery: crate::github::delivery::decode(b"issues", ISSUE, &limits()).unwrap(),
    };
    for payload_first in [false, true] {
        let input = wrapped(ISSUE, payload_first, b"issues");
        for chunk in [1, 7, 4096] {
            assert_eq!(decode(Kind::LogDelivery, &input, chunk, &limits()).1, Ok(Document::Delivery(expected.clone())));
        }
        let mut plan = LogDelivery::new(1);
        assert_eq!(plan.begin(bounds()).unwrap(), Step::send(Request::LogDelivery { id: 1 }));
        assert_eq!(plan.heard(Heard::Head { status: 200, head: Head::new() }, bounds()).unwrap(), None);
        assert_eq!(
            plan.heard(Heard::Done(Body::Document(Document::Delivery(expected.clone()))), bounds()).unwrap(),
            Some(Step::Done(Ok(expected.clone())))
        );
    }
}
#[test]
fn logged_delivery_checks_event_agreement_and_payload_bound() {
    for first in [false, true] {
        let input = wrapped(ISSUE, first, b"push");
        assert_eq!(
            crate::github::plans::failure::malformed(decode(Kind::LogDelivery, &input, 1, &limits()).1.unwrap_err()),
            api::Error::InvalidAnswer
        );
    }
    let input = wrapped(ISSUE, false, b"issues");
    let mut small = limits();
    small.json.length = u32::try_from(input.len()).unwrap() - 1;
    let error = decode(Kind::LogDelivery, &input, 1, &small).1.unwrap_err();
    assert_eq!(crate::github::plans::failure::malformed(error), api::Error::TooLarge);
    let mut plan = LogDelivery::new(1);
    plan.begin(bounds()).unwrap();
    plan.heard(Heard::Head { status: 200, head: Head::new() }, bounds()).unwrap();
    assert_eq!(
        plan.heard(Heard::Done(Body::Malformed(error)), bounds()).unwrap(),
        Some(Step::Done(Err(api::Error::TooLarge)))
    );
}
#[test]
fn logged_unhinted_event_is_other() {
    let input = br#"{"event":"ping","request":{"headers":{"X-GitHub-Event":"ping","X-GitHub-Delivery":"01234567-89ab-cdef-0123-456789abcdef"},"payload":{"ignored":[null]}},"response":null}"#;
    assert_eq!(
        decode(Kind::LogDelivery, input, 1, &limits()).1,
        Ok(Document::Delivery(Logged { event: copy_of(b"ping"), delivery: Delivery::Other }))
    );
}

#[test]
fn log_row_retains_action_owners_status_and_redelivery() {
    let input = br#"[{"id":19,"guid":"01234567-89ab-cdef-0123-456789abcdef","event":"issues","action":"opened","installation_id":18446744073709551615,"repository_id":9007199254740993,"status_code":503,"delivered_at":"2026-10-20T08:00:00Z","redelivery":true}]"#;
    let (rows, result) = decode(Kind::LogRows, input, 1, &limits());
    assert_eq!(result, Ok(Document::Rows));
    let Row::Log(row) = &rows.as_slice()[0] else { panic!("log row") };
    assert_eq!(row.action.as_deref(), Some(b"opened".as_slice()));
    assert_eq!(row.installation_id, Some(u64::MAX));
    assert_eq!(row.repository_id, Some(9_007_199_254_740_993));
    assert_eq!(row.status_code, 503);
    assert!(row.redelivery);
}

#[test]
fn log_reads_keep_app_failure_mapping_and_reject_calls_after_the_terminal() {
    for (status, error) in [(401, api::Error::Forbidden), (404, api::Error::Missing), (500, api::Error::Timeout)] {
        let mut page = LogPage::new(Ask::First { validator: None });
        assert_eq!(page.heard(Heard::Head { status, head: Head::new() }, bounds()), Err(api::Error::InvalidAnswer));
        page.begin(bounds()).unwrap();
        assert_eq!(
            page.heard(Heard::Head { status, head: Head::new() }, bounds()).unwrap(),
            Some(Step::Done(Err(error)))
        );
        assert_eq!(page.heard(Heard::Done(Body::Empty), bounds()), Err(api::Error::InvalidAnswer));
        let mut delivery = LogDelivery::new(1);
        delivery.begin(bounds()).unwrap();
        assert_eq!(
            delivery.heard(Heard::Head { status, head: Head::new() }, bounds()).unwrap(),
            Some(Step::Done(Err(error)))
        );
    }
}

#[test]
fn log_row_kind_uses_the_pushed_event_table() {
    use crate::github::log::Kind;
    let (rows, _) = decode(crate::github::document::Kind::LogRows, &list(PING, 1), 1, &limits());
    let Row::Log(mut row) = rows.into_boxed().into_iter().next().unwrap() else { panic!("log row") };
    for (event, kind) in [
        (b"installation".as_slice(), Kind::Installations),
        (b"installation_repositories", Kind::Repositories),
        (b"issues", Kind::Hinted),
        (b"issue_comment", Kind::Hinted),
        (b"pull_request", Kind::Hinted),
        (b"pull_request_review", Kind::Hinted),
        (b"pull_request_review_comment", Kind::Hinted),
        (b"push", Kind::Hinted),
        (b"create", Kind::Hinted),
        (b"delete", Kind::Hinted),
        (b"check_run", Kind::Hinted),
        (b"check_suite", Kind::Hinted),
        (b"status", Kind::Hinted),
        (b"workflow_job", Kind::Hinted),
        (b"workflow_run", Kind::Hinted),
        (b"merge_group", Kind::Hinted),
        (b"ping", Kind::Other),
        (b"installation_target", Kind::Other),
        (b"unknown", Kind::Other),
    ] {
        row.event = copy_of(event);
        assert_eq!(row.kind(), kind, "{event:?}");
    }
}

#[test]
fn log_row_rejects_a_guid_that_is_not_uuid_text() {
    let input = br#"[{"id":1,"guid":"bad","event":"ping","action":null,"installation_id":null,"repository_id":null,"status_code":200,"delivered_at":"2026-10-20T08:00:00Z","redelivery":false}]"#;
    assert_eq!(page(Ask::First { validator: None }, head(None), input), Err(api::Error::InvalidAnswer));
}
