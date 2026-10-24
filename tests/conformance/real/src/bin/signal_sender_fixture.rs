//! A single-threaded service: block SIGTERM before spawning its sender.

#[cfg(target_os = "linux")]
#[expect(
    clippy::too_many_lines,
    reason = "the single-threaded signal fixture owns startup and drains io to settlement"
)]
fn main() {
    use skein_io::kernel::{Pipe, ServiceSignal, Spawn, Way};
    use skein_io::{self as io, Event, Request};
    use skein_lib::{Duration, Env, Queue, Token};
    use skein_shell::{Backend, Clock, Config, Kernel, Wait};

    // Three entities give io 14 operation slots; 32 records hold a complete
    // batch and the events produced while it lands.
    const RECORDS: u32 = 32;
    let mut args = std::env::args().skip(1);
    let backend = match args.next().as_deref() {
        Some("ring") => Backend::Ring,
        Some("readiness") => Backend::Readiness,
        other => panic!("unknown backend {other:?}"),
    };
    let from_child = match args.next().as_deref() {
        Some("child") => true,
        Some("self") => false,
        other => panic!("unknown sender {other:?}"),
    };
    let signals = skein_shell::open_termination_signals().expect("block signals on the only thread");
    let root = skein_shell::open_root(std::path::Path::new(".")).expect("spawn root");
    // One signal source, one child and its input pipe; the pipe keeps the
    // sender live until the parent has consumed its signal (io.md, 7).
    let limits = io::Limits {
        sockets: 3,
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
    };
    let mut state = io::Io::new(&limits);
    let source = state.adopt_signals(signals).expect("source entity");
    let mut kernel = Kernel::open_on(Config { operations: io::operations(&limits).expect("operation bound") }, backend)
        .expect("kernel backend");
    let clock = Clock::new();
    let deadline = clock.now().now.saturating_add(Duration::from_secs(3));
    let mut submissions = Queue::with_capacity(RECORDS);
    let mut completions = Queue::with_capacity(RECORDS);
    let mut events = Queue::with_capacity(RECORDS);
    let now = clock.now();
    let env = Env { now: now.now, wall: now.wall, limits };
    let script = if from_child { b"kill -TERM \"$PPID\"; read line".as_slice() } else { b"read line".as_slice() };
    io::down(
        &mut state,
        &env,
        Request::Spawn {
            owner: Token::new(100),
            spawn: Spawn {
                root,
                dir: Box::from(b".".as_slice()),
                program: Box::from(b"/bin/sh".as_slice()),
                args: Box::new([Box::from(b"-c".as_slice()), Box::from(script)]),
                env: Box::new([]),
                pipes: Box::new([Pipe { child: 0, way: Way::In, parent: None }]),
            },
        },
        &mut submissions,
    );
    // The synchronous spawn completion lands before the first signal read.
    kernel.submit(&mut submissions, Wait::No);
    let mut child = None;
    let mut input = None;
    let mut heard = false;
    state.reclaim();
    loop {
        let now = clock.now();
        assert!(now.now < deadline, "signal sender service must settle: child {child:?}, heard {heard}");
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
                Event::Spawned { child: token, pipes, .. } => {
                    child = Some(token);
                    input = Some(*pipes.first().expect("the child input pipe"));
                    if !from_child {
                        skein_shell::signal_current_thread(ServiceSignal::Terminate).expect("self signal");
                    }
                }
                Event::Shutdown { signal, from, .. } => {
                    assert_eq!(signal, ServiceSignal::Terminate);
                    assert_eq!(from, if from_child { child } else { None });
                    assert!(child.is_some(), "the sender's spawn has completed");
                    assert!(!heard, "one signal");
                    heard = true;
                    io::down(
                        &mut state,
                        &env,
                        Request::Close { entity: child.expect("spawned child") },
                        &mut submissions,
                    );
                    io::down(
                        &mut state,
                        &env,
                        Request::Close { entity: input.expect("the child input pipe") },
                        &mut submissions,
                    );
                    io::down(&mut state, &env, Request::Close { entity: source }, &mut submissions);
                }
                Event::Exited { .. } | Event::Closed { .. } | Event::Stream { .. } => {}
                other @ (Event::Listening { .. }
                | Event::Accepted { .. }
                | Event::Connecting { .. }
                | Event::Connected { .. }
                | Event::Output { .. }
                | Event::Usage { .. }
                | Event::Resized
                | Event::Window { .. }
                | Event::Failed { .. }) => panic!("unexpected signal service event {other:?}"),
            }
        }
        state.reclaim();
        if state.is_empty() {
            assert!(heard, "the service heard its signal");
            assert_eq!(kernel.in_flight(), 0);
            skein_shell::close_keeper_fd(root);
            return;
        }
        let wait = if state.is_ready() || !events.is_empty() { Wait::No } else { Wait::Until(deadline) };
        kernel.submit(&mut submissions, wait);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {}
