//! Step checks of route policy, caching and fallback paths.
use super::{State, bundle::Bundle, routes};
use skein_http::server::{Body, Call};
use skein_http::{Header, Method, Version};
use std::collections::BTreeMap;
fn bundle() -> Bundle {
    let mut files = BTreeMap::new();
    files.insert("boot.js".into(), Box::from(&b"export {};"[..]));
    Bundle::new(Box::from(&b"<html></html>"[..]), "0123456789abcdef".into(), files).expect("fixture bundle fits")
}
fn get(path: &[u8]) -> Call {
    Call {
        method: Method::Get,
        target: Box::from(path),
        version: Version::Http11,
        headers: Box::from([]),
        body: Body::None,
    }
}
const POLICY: &[u8] = b"default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self'; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";
#[test]
fn each_answer_carries_the_policy_and_nosniff() {
    let bundle = bundle();
    let mut state = State::new();
    for (method, path) in [
        (Method::Get, b"/".as_slice()),
        (Method::Get, b"/api/notes"),
        (Method::Get, b"/bundle/0123456789abcdef/boot.js"),
        (Method::Get, b"/bundle/wrong/file.js"),
        (Method::Post, b"/api/notes"),
    ] {
        let mut call = get(path);
        call.method = method;
        let answer = routes::route(&mut state, &bundle, &call, &[]);
        for (name, expected) in
            [(b"Content-Security-Policy".as_slice(), POLICY), (b"X-Content-Type-Options", b"nosniff")]
        {
            let header = answer
                .head
                .headers
                .iter()
                .find(|header| header.name.eq_ignore_ascii_case(name))
                .expect("required header");
            assert_eq!(header.value.as_ref(), expected, "path {path:?}, header {name:?}");
        }
    }
}
#[test]
fn another_digest_is_not_found() {
    let bundle = bundle();
    for path in [
        b"/bundle/ffffffffffffffff/boot.js".as_slice(),
        b"/bundle/0123456789abcdef/no.js",
        b"/bundle/0123456789abcdef/../index.html",
    ] {
        assert_eq!(routes::route(&mut State::new(), &bundle, &get(path), &[]).head.status, 404, "path {path:?}");
    }
}
#[test]
fn the_page_answers_any_other_path() {
    let bundle = bundle();
    for path in [b"/".as_slice(), b"/notes/7", b"/unknown?x=1"] {
        let answer = routes::route(&mut State::new(), &bundle, &get(path), &[]);
        assert_eq!(answer.head.status, 200, "path {path:?}");
        assert_eq!(answer.body.as_ref(), b"<html></html>", "path {path:?}");
        let cache = answer
            .head
            .headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(b"Cache-Control"))
            .expect("page cache header");
        assert_eq!(cache.value.as_ref(), b"no-cache");
    }
}
#[test]
fn a_matching_etag_is_not_modified() {
    let bundle = bundle();
    let mut call = get(b"/notes/7");
    call.headers =
        Box::from([Header { name: Box::from(&b"If-None-Match"[..]), value: Box::from(bundle.etag.as_bytes()) }]);
    let answer = routes::route(&mut State::new(), &bundle, &call, &[]);
    assert_eq!(answer.head.status, 304);
    assert_eq!(answer.head.body, Body::None);
    assert!(answer.body.is_empty());
    for (name, value) in [
        (b"Content-Security-Policy".as_slice(), POLICY),
        (b"X-Content-Type-Options", b"nosniff"),
        (b"Cache-Control", b"no-cache"),
    ] {
        let header =
            answer.head.headers.iter().find(|header| header.name.eq_ignore_ascii_case(name)).expect("header on 304");
        assert_eq!(header.value.as_ref(), value);
    }
}

#[test]
fn the_page_etag_is_its_fnv_digest() {
    assert_eq!(bundle().etag, "\"52ea7bb227177076\"");
}

#[test]
fn a_second_add_with_one_key_makes_no_second_note() {
    use skein_lib::Reader;
    use skein_ui_example_view::records::{CEILINGS, Notes};
    let mut state = State::new();
    let bundle = bundle();
    let mut call = get(b"/api/notes");
    call.method = Method::Post;
    let add = b"\0\0\0\0\0\0\0\x01\0\0\0\x01A\0\0\0\0\0\0\0\0\0\0\0\0\0";
    let _answer = routes::route(&mut state, &bundle, &call, add);
    let _answer = routes::route(&mut state, &bundle, &call, add);
    let notes = Notes::decode(&CEILINGS, &mut Reader::new(&state.list())).expect("notes decode");
    assert_eq!(notes.notes().len(), 1);
}

#[test]
fn a_second_add_with_one_key_answers_the_first_note() {
    let mut state = State::new();
    let bundle = bundle();
    let mut call = get(b"/api/notes");
    call.method = Method::Post;
    let _answer = routes::route(&mut state, &bundle, &call, b"\0\0\0\0\0\0\0\x01\0\0\0\x01A\0\0\0\0\0\0\0\0\0\0\0\0\0");
    let answer = routes::route(&mut state, &bundle, &call, b"\0\0\0\0\0\0\0\x01\0\0\0\x01B\0\0\0\0\0\0\0\0\0\0\0\0\0");
    assert_eq!(answer.body.as_ref(), b"\0\0\0\0\0\0\0\x01\0\0\0\x01A\0\0\0\0\0\0\0\0\0\0\0\0\0");
}

#[test]
fn the_event_stream_stays_open() {
    let answer = routes::route(&mut State::new(), &bundle(), &get(b"/api/events"), &[]);
    assert_eq!((answer.head.body, answer.head.close), (Body::Chunked, false));
}

#[test]
fn the_event_stream_declares_its_type() {
    let answer = routes::route(&mut State::new(), &bundle(), &get(b"/api/events"), &[]);
    let kind =
        answer.head.headers.iter().find(|header| header.name.as_ref() == b"Content-Type").expect("event stream type");
    assert_eq!(kind.value.as_ref(), b"text/event-stream");
}

#[test]
fn a_replayed_add_is_not_broadcast_again() {
    let mut state = State::new();
    let add = b"\0\0\0\0\0\0\0\x01\0\0\0\x01A\0\0\0\0\0\0\0\0\0\0\0\0\0";
    state.add(add).expect("fixture add accepted");
    state.take_made();
    state.add(add).expect("fixture add accepted");
    assert_eq!(state.take_made(), None);
}
