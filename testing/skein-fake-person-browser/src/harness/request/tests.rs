use super::*;

const SOCKET: Token = Token::new(300);

#[test]
fn teardown_after_a_status_closes_the_client_once_before_the_socket_settles() {
    let asked = Asked {
        method: skein_http::Method::Get,
        path: b"/".as_slice().into(),
        host: b"other.test".as_slice().into(),
        headers: vec![],
        body: Box::new([]),
    };
    let mut exchange = Exchange::new(0, 1, asked, "127.0.0.1:9000", Time::ZERO);
    exchange.requests.clear();
    exchange.event(io::Event::Connecting { owner: exchange.owner, socket: SOCKET });
    exchange.event(io::Event::Connected { owner: exchange.owner });
    exchange.event(io::Event::Stream { owner: exchange.owner, up: stream::Up::Room });
    exchange.requests.clear();
    for line in b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".split_inclusive(|byte| *byte == b'\n') {
        exchange.requests.clear();
        exchange.event(io::Event::Stream { owner: exchange.owner, up: stream::Up::Bytes(line.into()) });
    }
    assert_eq!(exchange.status, Some(200));
    assert_eq!(exchange.client.waiting(), client::Waiting::Nothing);
    assert!(!exchange.closed);
    exchange.stop();
    assert_eq!(exchange.requests, VecDeque::from([Request::Abort { entity: SOCKET }]));
    assert!(exchange.above.is_empty());
    exchange.event(io::Event::Closed { owner: exchange.owner });
    assert!(exchange.closed);
}
