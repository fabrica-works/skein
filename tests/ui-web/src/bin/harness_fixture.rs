//! Fixed, local children for the browser harness tests. No inherited settings.
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("server") => server(&args[1..]),
        other => panic!("unknown fixture mode: {other:?}"),
    }
}
fn server(peers: &[String]) {
    for peer in peers {
        let address: SocketAddr = peer.parse().expect("substituted peer address");
        assert!(address.ip().is_loopback());
        let mut client = TcpStream::connect(address).expect("peer listens before fixture starts");
        client.write_all(b"GET / HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n").expect("request peer");
        let mut reply = Vec::new();
        client.read_to_end(&mut reply).expect("fixed peer response");
        assert!(reply.ends_with(b"peer ready"), "peer response");
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("fixture listener");
    eprintln!("fixture listening {}", listener.local_addr().expect("fixture address"));
    for stream in listener.incoming() {
        let mut stream = stream.expect("fixture connection");
        let mut request = Vec::new();
        let mut byte = [0_u8];
        while !request.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).expect("fixture request") == 0 {
                break;
            }
            request.extend_from_slice(&byte);
            assert!(request.len() <= 8192, "bounded fixture request");
        }
        let body = b"<!doctype html><h1>Ready</h1>";
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("fixture head");
        stream.write_all(body).expect("fixture body");
    }
}
