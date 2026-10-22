use crate::origins as serve;
use skein_io::{Event, Request};
use skein_lib::stream::{self, Held, Read};
use skein_lib::{Time, Token, Wall};
use std::net::{Ipv4Addr, SocketAddr};
pub(super) fn response(path: &str) -> Vec<u8> {
    response_body(path, b"http://{address}/", &[])
}
fn response_body(path: &str, body: &[u8], peers: &[(Box<[u8]>, String)]) -> Vec<u8> {
    let pages = vec![serve::Page {
        path: b"/".as_slice().into(),
        kind: b"text/html".as_slice().into(),
        body: body.into(),
        headers: vec![],
    }];
    served(serve::Pages::new(pages, Time::ZERO, Wall::EPOCH), path, peers)
}
fn served(mut pages: serve::Pages, path: &str, peers: &[(Box<[u8]>, String)]) -> Vec<u8> {
    pages.address("127.0.0.1:9090");
    pages.peers(peers);
    assert!(pages.pending());
    assert!(matches!(pages.take(), Some(Request::Listen { owner: serve::OWNER, .. })));
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 33333));
    pages.update_time(Time::ZERO, Wall::EPOCH);
    pages.event(Event::Listening { owner: serve::OWNER, listener: Token::new(200), addr: skein_io::Addr::Ip(addr) });
    assert_eq!(pages.addr, Some(addr));
    let owner = Token::new(serve::ORIGINS_FIRST + 1);
    pages.event(Event::Accepted { owner: serve::OWNER, socket: Token::new(300), peer: skein_io::Addr::Ip(addr) });
    assert!(pages.owns(owner));

    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    let mut held = Held::new(request.into_bytes().into_boxed_slice());
    let mut wire = Vec::new();
    for _ in 0..200 {
        match pages.take().expect("page server has the next step") {
            Request::Bind { socket, owner: bound } => {
                assert_eq!(socket, Token::new(300));
                assert_eq!(bound, owner);
            }
            Request::Stream { stream, down } => {
                assert_eq!(stream, Token::new(300));
                match down {
                    stream::Down::Demand { read: Read::Nothing, room: 0 } => {}
                    stream::Down::Demand { room: 1.., .. } => {
                        pages.event(Event::Stream { owner, up: stream::Up::Room });
                    }
                    stream::Down::Demand { read, room: 0 } => {
                        pages.event(Event::Stream { owner, up: held.answer(read).expect("request input") });
                    }
                    stream::Down::Send(bytes) => wire.extend_from_slice(&bytes),
                    stream::Down::Finish => panic!("HTTP server closes through io"),
                }
            }
            Request::Close { entity } if entity == Token::new(300) => {
                pages.event(Event::Closed { owner });
                break;
            }
            other @ (Request::Listen { .. }
            | Request::Connect { .. }
            | Request::Reject { .. }
            | Request::Output { .. }
            | Request::Spawn { .. }
            | Request::Signal { .. }
            | Request::Usage { .. }
            | Request::Window { .. }
            | Request::Close { .. }
            | Request::Abort { .. }) => panic!("unexpected page request: {other:?}"),
        }
    }
    assert!(!pages.owns(owner));
    pages.stop();
    assert_eq!(pages.take(), Some(Request::Close { entity: Token::new(200) }));
    pages.event(Event::Closed { owner: serve::OWNER });
    assert!(pages.is_closed());
    wire
}

#[test]
fn origin_bodies_replace_peer_and_server_placeholders() {
    let wire = response_body(
        "/",
        b"{neighbor}/{address}/{neighbor}",
        &[(b"neighbor".as_slice().into(), "127.0.0.1:8080".into())],
    );
    assert!(wire.ends_with(b"127.0.0.1:8080/127.0.0.1:9090/127.0.0.1:8080"));
}

#[test]
fn verbatim_origins_serve_placeholders_as_written() {
    let body = b"{neighbor}/{address}/{neighbor}";
    let pages = vec![serve::Page {
        path: b"/".as_slice().into(),
        kind: b"text/html".as_slice().into(),
        body: body.as_slice().into(),
        headers: vec![],
    }];
    let wire = served(
        serve::Pages::verbatim(pages, Time::ZERO, Wall::EPOCH, 0),
        "/",
        &[(b"neighbor".as_slice().into(), "127.0.0.1:8080".into())],
    );
    assert!(wire.ends_with(body));
}

fn query_response(verbatim: bool, target: &str) -> Vec<u8> {
    let pages = vec![serve::Page {
        path: b"/a.html".as_slice().into(),
        kind: b"text/html".as_slice().into(),
        body: b"page at {address}".as_slice().into(),
        headers: vec![(b"X-Page".as_slice().into(), b"fixture".as_slice().into())],
    }];
    let pages = if verbatim {
        serve::Pages::verbatim(pages, Time::ZERO, Wall::EPOCH, 0)
    } else {
        serve::Pages::new(pages, Time::ZERO, Wall::EPOCH)
    };
    served(pages, target, &[])
}

fn assert_query_page(verbatim: bool, target: &str, body: &[u8]) {
    let wire = query_response(verbatim, target);
    assert!(wire.starts_with(b"HTTP/1.1 200 OK\r\n"), "{target}, verbatim={verbatim}");
    let wire_text = String::from_utf8_lossy(&wire);
    assert!(wire_text.contains("\r\nContent-Type: text/html\r\n"));
    assert!(wire_text.contains("\r\nX-Page: fixture\r\n"));
    assert!(wire.ends_with(body), "{target}, verbatim={verbatim}");
}

#[test]
fn origin_queries_leave_the_page_response_intact() {
    for (verbatim, body) in [(false, b"page at 127.0.0.1:9090".as_slice()), (true, b"page at {address}".as_slice())] {
        for target in ["/a.html?x=1", "/a.html?", "/a.html?x=1?y=2"] {
            assert_query_page(verbatim, target, body);
        }
    }
}

#[test]
fn origin_paths_with_queries_do_not_climb() {
    for verbatim in [false, true] {
        for target in ["/../a.html?x=1", "/x/..%2fa.html?y", "/a.html/..?x", "/unknown?x=1"] {
            let wire = query_response(verbatim, target);
            assert!(wire.starts_with(b"HTTP/1.1 404 Not Found\r\n"), "{target}, verbatim={verbatim}");
            assert!(wire.ends_with(b"not found"));
        }
    }
}

#[test]
fn origin_queries_can_hold_slashes_and_parent_segments() {
    for (verbatim, body) in [(false, b"page at 127.0.0.1:9090".as_slice()), (true, b"page at {address}".as_slice())] {
        assert_query_page(verbatim, "/a.html?p=../../etc/passwd", body);
    }
}

#[test]
fn origin_favicon_queries_are_no_content() {
    for verbatim in [false, true] {
        let wire = query_response(verbatim, "/favicon.ico?v=1");
        assert!(wire.starts_with(b"HTTP/1.1 204 No Content\r\n"));
        assert!(wire.ends_with(b"\r\n\r\n"));
    }
}
