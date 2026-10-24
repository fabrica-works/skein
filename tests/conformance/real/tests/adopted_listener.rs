//! A listener opened before the loop retains its queued connection when claimed.

use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::os::fd::IntoRawFd;

use skein_io::{self as io, Addr, Event, Request, kernel::Fd};
use skein_lib::{Duration, Env, Queue, Token};
use skein_shell::{Backend, Clock, Config, Kernel, Wait};

fn limits() -> io::Limits {
    io::Limits {
        sockets: 2,
        names: 0,
        refusals: 1,
        intake: 8,
        receive: 4,
        output: 8,
        sends: 1,
        accepts: 1,
        backlog: 1,
        close_timeout: Duration::from_secs(1),
        retry: Duration::from_millis(1),
    }
}

fn queued_before_claim(backend: Backend) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("startup listener");
    listener.set_nonblocking(true).expect("nonblocking listener");
    let address = listener.local_addr().expect("bound address");
    let client = TcpStream::connect(address).expect("connection queued before the loop");
    let peer = client.local_addr().expect("queued client's address");
    let limits = limits();
    let mut state = io::Io::new(&limits);
    let token = state.adopt_listener(Fd::new(listener.into_raw_fd()), address).expect("listener adopted");
    let mut kernel = Kernel::open_on(Config { operations: io::operations(&limits).expect("operation bound") }, backend)
        .expect("real backend");
    let clock = Clock::new();
    let deadline = clock.now().now.saturating_add(Duration::from_secs(2));
    let mut submissions = Queue::with_capacity(16);
    let mut completions = Queue::with_capacity(16);
    let mut events = Queue::with_capacity(16);
    let now = clock.now();
    io::down(
        &mut state,
        &Env { now: now.now, wall: now.wall, limits },
        Request::Listen { owner: Token::new(100), addr: Addr::Ip(address) },
        &mut submissions,
    );
    assert!(submissions.is_empty(), "claim skips socket, bind and listen");
    let mut listening = false;
    let mut accepted = false;
    let mut closed = false;
    for _ in 0..1000 {
        state.reclaim();
        let now = clock.now();
        assert!(now.now < deadline, "listener adoption must settle");
        let env = Env { now: now.now, wall: now.wall, limits };
        while state.is_ready() {
            io::resume(&mut state, &env, &mut events, &mut submissions);
        }
        kernel.reap(&mut completions);
        while let Some(complete) = completions.pop() {
            io::up(&mut state, &env, complete, &mut events, &mut submissions);
        }
        while let Some(event) = events.pop() {
            match event {
                Event::Listening { owner, listener, addr } => {
                    assert!(!listening);
                    assert_eq!((owner, listener, addr), (Token::new(100), token, Addr::Ip(address)));
                    listening = true;
                }
                Event::Accepted { owner, socket, peer: address } => {
                    assert!(listening && !accepted);
                    assert_eq!((owner, address), (Token::new(100), Addr::Ip(peer)));
                    accepted = true;
                    io::down(&mut state, &env, Request::Reject { socket }, &mut submissions);
                    io::down(&mut state, &env, Request::Close { entity: token }, &mut submissions);
                }
                Event::Closed { owner } => {
                    assert!(!closed);
                    assert_eq!(owner, Token::new(100));
                    closed = true;
                }
                other @ (Event::Connecting { .. }
                | Event::Connected { .. }
                | Event::Stream { .. }
                | Event::Output { .. }
                | Event::Spawned { .. }
                | Event::Exited { .. }
                | Event::Usage { .. }
                | Event::Resized
                | Event::Window { .. }
                | Event::Shutdown { .. }
                | Event::Failed { .. }) => panic!("unexpected listener event: {other:?}"),
            }
        }
        state.reclaim();
        if state.is_empty() {
            assert!(listening && accepted && closed);
            assert_eq!(kernel.in_flight(), 0);
            return;
        }
        kernel.submit(&mut submissions, Wait::Until(now.now.saturating_add(Duration::from_millis(5))));
    }
    panic!("listener adoption must settle");
}

#[test]
fn a_held_listener_accepts_a_connection_queued_before_claim_on_readiness() {
    queued_before_claim(Backend::Readiness);
}

#[cfg(target_os = "linux")]
#[test]
fn a_held_listener_accepts_a_connection_queued_before_claim_on_ring() {
    queued_before_claim(Backend::Ring);
}
