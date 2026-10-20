//! Fixed, local children for the browser harness tests. No inherited settings.
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("server") => server(&args[1..], false),
        Some("held-server") => server(&[], true),
        Some("command") => command(&args[1..]),
        other => panic!("unknown fixture mode: {other:?}"),
    }
}
fn server(peers: &[String], held: bool) {
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
    if held {
        eprintln!("http://{}/held", listener.local_addr().expect("fixture address"));
    } else {
        eprintln!("fixture listening {}", listener.local_addr().expect("fixture address"));
    }
    let mut visits = 0;
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
        let path =
            std::str::from_utf8(&request).expect("fixture request text").split_whitespace().nth(1).unwrap_or("/");
        if let Some(port) = path.strip_prefix("/signal/") {
            let port: u16 = port.parse().expect("signal port");
            TcpStream::connect((Ipv4Addr::LOCALHOST, port))
                .expect("command signal listener")
                .write_all(&[1])
                .expect("command signal");
        }
        let body = if path == "/held" {
            visits += 1;
            format!("held visit {visits}")
        } else {
            "<!doctype html><h1>Ready</h1><input type=hidden value='fixture value'>".to_owned()
        };
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("fixture head");
        stream.write_all(body.as_bytes()).expect("fixture body");
    }
}
fn command(args: &[String]) {
    match args[0].as_str() {
        "stdin" => {
            let mut input = String::new();
            std::io::stdin().read_to_string(&mut input).expect("stdin EOF");
            assert!(input.ends_with("/held\n"), "held server line includes LF");
            let link = input.trim_end();
            let address = link.strip_prefix("http://").expect("held URL").strip_suffix("/held").expect("held path");
            let response = request(address, "/held");
            assert!(response.ends_with("held visit 1"), "blank page never visits held link");
            println!("http://{address}/opened");
        }
        "closed" => {
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input).expect("closed stdin EOF");
            assert!(input.is_empty(), "closed stdin has no held line");
        }
        "link" => println!("{}", args[1]),
        "no-link" => eprintln!("fixture exits without a link"),
        "code" => std::process::exit(args[1].parse().expect("test code")),
        "wait" => {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("command signal listener");
            if let Some(address) = args.get(1) {
                println!("http://{address}/signal/{}", listener.local_addr().expect("signal address").port());
            }
            std::io::stdout().flush().expect("ready link");
            let (mut socket, _) = listener.accept().expect("loopback signal");
            let mut byte = [0];
            socket.read_exact(&mut byte).expect("signal byte");
            assert_eq!(byte, [1]);
        }
        "args" => {
            assert_addresses(&args[1], &args[2]);
        }
        "env" => {
            assert_addresses(&supplied("FIXTURE_PEER"), &supplied("FIXTURE_SERVER"));
            assert!(std::env::var_os("HOME").is_none(), "environment is explicit");
        }
        other => panic!("unknown command fixture {other}"),
    }
}
fn request(address: &str, path: &str) -> String {
    let address: SocketAddr = address.parse().expect("substituted address");
    assert!(address.ip().is_loopback());
    let mut stream = TcpStream::connect(address).expect("test listener");
    write!(stream, "GET {path} HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n").expect("test request");
    let mut output = String::new();
    stream.read_to_string(&mut output).expect("test response");
    output
}
fn assert_addresses(peer: &str, server: &str) {
    assert!(request(peer, "/").ends_with("peer ready"));
    assert!(request(server, "/").contains("<h1>Ready</h1>"));
}

#[expect(
    clippy::disallowed_methods,
    reason = "the fixture child verifies only invented environment values explicitly supplied by its test"
)]
fn supplied(name: &str) -> String {
    std::env::var(name).expect("supplied fixture value")
}
