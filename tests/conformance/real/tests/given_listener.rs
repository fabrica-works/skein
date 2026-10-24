//! Given listeners survive exec, retain their backlog and remain the parent's.

#[path = "support/given_listener.rs"]
mod support;

use std::net::{Ipv4Addr, TcpStream};
use std::path::Path;

use skein_io::kernel::{Exit, Fd, Pipe, Spawn, Way};
use skein_io::{self as io, Event, Request};
use skein_lib::{Duration, Env, Queue, Token};
use skein_shell::{Backend, Clock, Config, Kernel, Wait};

fn spawn_and_exit(kernel: &mut Kernel, state: &mut io::Io, root: Fd, held: Token, address: std::net::SocketAddr) {
    let clock = Clock::new();
    let limits = support::limits();
    let now = clock.now();
    let env = Env { now: now.now, wall: now.wall, limits };
    let deadline = now.now.saturating_add(Duration::from_secs(3));
    let mut submissions = Queue::with_capacity(16);
    let mut completions = Queue::with_capacity(16);
    let mut events = Queue::with_capacity(16);
    let spawn = Spawn {
        program: Box::from(env!("CARGO_BIN_EXE_process_fixture").as_bytes()),
        args: Box::new([Box::from(&b"given-listener"[..]), Box::from(address.to_string().as_bytes())]),
        env: Box::new([Box::from(&b"LISTEN_FDS=1"[..])]),
        root,
        dir: Box::from(&b"."[..]),
        pipes: Box::new([Pipe { child: 3, way: Way::Given(held), parent: None }]),
    };
    io::down(state, &env, Request::Spawn { owner: Token::new(100), spawn }, &mut submissions);
    let mut spawned = false;
    let mut exited = false;
    let mut closed = false;
    for _ in 0..1000 {
        state.reclaim();
        let now = clock.now();
        assert!(now.now < deadline, "given child must settle");
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
                Event::Spawned { owner, pipes, .. } => {
                    assert_eq!(owner, Token::new(100));
                    assert!(!spawned);
                    assert!(pipes.is_empty());
                    spawned = true;
                }
                Event::Exited { owner, exit } => {
                    assert!(spawned && !exited);
                    assert_eq!((owner, exit), (Token::new(100), Exit::Code(0)));
                    exited = true;
                }
                Event::Closed { owner } => {
                    assert!(exited && !closed);
                    assert_eq!(owner, Token::new(100));
                    closed = true;
                }
                other @ (Event::Listening { .. }
                | Event::Accepted { .. }
                | Event::Connecting { .. }
                | Event::Connected { .. }
                | Event::Stream { .. }
                | Event::Output { .. }
                | Event::Usage { .. }
                | Event::Resized
                | Event::Window { .. }
                | Event::Shutdown { .. }
                | Event::Failed { .. }) => panic!("unexpected spawn event: {other:?}"),
            }
        }
        state.reclaim();
        if closed {
            assert_eq!(kernel.in_flight(), 0);
            return;
        }
        kernel.submit(&mut submissions, Wait::Until(now.now.saturating_add(Duration::from_millis(5))));
    }
    panic!("given child must settle");
}

fn given_listener(backend: Backend) {
    let (fd, address) = skein_shell::open_listener((Ipv4Addr::LOCALHOST, 0).into(), 2).expect("held listener");
    let root = skein_shell::open_root(Path::new(".")).expect("spawn root");
    let limits = support::limits();
    let mut state = io::Io::new(&limits);
    let held = state.adopt_listener(fd, address).expect("adopt held listener");
    let mut kernel = Kernel::open_on(Config { operations: io::operations(&limits).expect("operation bound") }, backend)
        .expect("parent kernel");
    for _ in 0..2 {
        let _client = TcpStream::connect(address).expect("connection queued before spawn");
        spawn_and_exit(&mut kernel, &mut state, root, held, address);
        assert_eq!(skein_shell::listener_at(fd), Ok(address), "parent still holds a listening socket after exit");
    }
    let _client = TcpStream::connect(address).expect("parent copy accepts after both children exit");
    support::claim_and_accept(&mut kernel, &mut state, held, address);
    skein_shell::close_keeper_fd(root);
}

#[test]
fn given_listener_survives_exec_and_child_exit_on_readiness() {
    given_listener(Backend::Readiness);
}

#[cfg(target_os = "linux")]
#[test]
fn given_listener_survives_exec_and_child_exit_on_ring() {
    given_listener(Backend::Ring);
}
