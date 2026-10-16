use crate::origins as serve;
use skein_io::{Event, Request};
use skein_lib::stream::{self, Held, Read};
use skein_lib::{Time, Token, Wall};
use std::net::{Ipv4Addr, SocketAddr};
pub(super) fn response(path: &str) -> Vec<u8> {
    let mut pages = serve::Pages::new(
        vec![serve::Page {
            path: b"/".as_slice().into(),
            kind: b"text/html".as_slice().into(),
            body: b"http://{address}/".as_slice().into(),
            headers: vec![],
        }],
        Time::ZERO,
        Wall::EPOCH,
    );
    pages.address("127.0.0.1:9090");
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
