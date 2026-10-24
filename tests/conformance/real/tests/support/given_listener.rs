use std::net::SocketAddr;

use skein_io::{self as io, Addr, Event, Request};
use skein_lib::{Duration, Env, Queue, Token};
use skein_shell::{Clock, Kernel, Wait};

pub fn limits() -> io::Limits {
    io::Limits {
        sockets: 2,
        names: 0,
        refusals: 1,
        intake: 8,
        receive: 4,
        output: 8,
        sends: 1,
        accepts: 1,
        backlog: 2,
        close_timeout: Duration::from_secs(1),
        retry: Duration::from_millis(1),
    }
}

pub fn claim_and_accept(kernel: &mut Kernel, state: &mut io::Io, token: Token, address: SocketAddr) {
    let limits = limits();
    let clock = Clock::new();
    let deadline = clock.now().now.saturating_add(Duration::from_secs(2));
    let mut submissions = Queue::with_capacity(16);
    let mut completions = Queue::with_capacity(16);
    let mut events = Queue::with_capacity(16);
    let now = clock.now();
    io::down(
        state,
        &Env { now: now.now, wall: now.wall, limits },
        Request::Listen { owner: Token::new(100), addr: Addr::Ip(address) },
        &mut submissions,
    );
    assert!(submissions.is_empty(), "claim skips socket setup");
    let mut listening = false;
    let mut accepted = false;
    let mut closed = false;
    for _ in 0..1000 {
        state.reclaim();
        let now = clock.now();
        assert!(now.now < deadline, "inherited listener must settle");
        let env = Env { now: now.now, wall: now.wall, limits };
        while state.is_ready() {
            io::resume(state, &env, &mut events, &mut submissions);
        }
        kernel.reap(&mut completions);
        while let Some(complete) = completions.pop() {
            io::up(state, &env, complete, &mut events, &mut submissions);
        }
        while let Some(event) = events.pop() {
            match event {
                Event::Listening { owner, listener, addr } => {
                    assert!(!listening);
                    assert_eq!((owner, listener, addr), (Token::new(100), token, Addr::Ip(address)));
                    listening = true;
                }
                Event::Accepted { owner, socket, .. } => {
                    assert!(listening && !accepted);
                    assert_eq!(owner, Token::new(100));
                    accepted = true;
                    io::down(state, &env, Request::Reject { socket }, &mut submissions);
                    io::down(state, &env, Request::Close { entity: token }, &mut submissions);
                }
                Event::Closed { owner } => {
                    assert_eq!(owner, Token::new(100));
                    assert!(!closed);
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
    panic!("inherited listener must settle");
}
