//! Child process used by the attachment tests; no Forgejo binary is needed.
use std::io::{Read, Write};
use std::net::TcpListener;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
    println!("{}", listener.local_addr().expect("bound address").port());
    for socket in listener.incoming() {
        let mut socket = socket.expect("accepted");
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).expect("request head");
            head.extend_from_slice(&byte);
        }
        let body = if head.starts_with(b"GET /api/v1/version ") {
            b"{\"version\":\"17.0.0\"}".to_vec()
        } else {
            format!("{{\"login\":\"{}\",\"id\":{}}}", args[1], args[2]).into_bytes()
        };
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).expect("head");
        socket.write_all(&body).expect("body");
    }
}
