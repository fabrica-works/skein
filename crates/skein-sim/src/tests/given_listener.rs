use alloc::boxed::Box;
use core::net::{Ipv4Addr, SocketAddr};

use skein_io::kernel::{Complete, Done, Exit, Family, Fd, Op, Pipe, Spawn, Submit, Way};
use skein_io::{Addr, Event, Io, Limits, Request};
use skein_lib::{Duration, Env, Queue, Token};

use crate::{Answer, Config, Handle, Pid, Program, Reply, Sim};

fn call(sim: &mut Sim, pid: Pid, op: Op) -> Complete {
    let mut submissions = Queue::with_capacity(1);
    submissions.push(Submit { op: Token::new(100), kind: op });
    sim.submit(pid, &mut submissions);
    let mut completions = Queue::with_capacity(1);
    sim.reap(pid, &mut completions);
    completions.pop().expect("calm operation completes")
}

fn socket(sim: &mut Sim, pid: Pid) -> Fd {
    let Ok(Done::Fd(fd)) = call(sim, pid, Op::Socket { family: Family::Ipv4 }).result else {
        unreachable!("a calm socket succeeds")
    };
    fd
}

fn claim_and_accept(sim: &mut Sim, pid: Pid, fd: Fd, address: SocketAddr) {
    let limits = Limits {
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
    };
    let mut io = Io::new(&limits);
    let held = io.adopt_listener(fd, address).expect("adopt child descriptor");
    let env = Env { now: sim.now(), wall: sim.wall(), limits };
    let mut submissions = Queue::with_capacity(8);
    let mut completions = Queue::with_capacity(8);
    let mut events = Queue::with_capacity(8);
    skein_io::down(&mut io, &env, Request::Listen { owner: Token::new(1), addr: Addr::Ip(address) }, &mut submissions);
    assert!(submissions.is_empty(), "the inherited listener is claimed by its address");
    io.reclaim();
    while io.is_ready() {
        skein_io::resume(&mut io, &env, &mut events, &mut submissions);
    }
    assert_eq!(events.pop(), Some(Event::Listening { owner: Token::new(1), listener: held, addr: Addr::Ip(address) }));
    sim.submit(pid, &mut submissions);
    sim.reap(pid, &mut completions);
    while let Some(complete) = completions.pop() {
        skein_io::up(&mut io, &env, complete, &mut events, &mut submissions);
    }
    let Some(Event::Accepted { owner, socket, .. }) = events.pop() else {
        unreachable!("a queued connection is accepted by the child claim")
    };
    assert_eq!(owner, Token::new(1));
    skein_io::down(&mut io, &env, Request::Reject { socket }, &mut submissions);
    skein_io::down(&mut io, &env, Request::Close { entity: held }, &mut submissions);
    for _ in 0..8_u32 {
        sim.submit(pid, &mut submissions);
        sim.reap(pid, &mut completions);
        while let Some(complete) = completions.pop() {
            skein_io::up(&mut io, &env, complete, &mut events, &mut submissions);
        }
        while events.pop().is_some() {}
        io.reclaim();
        if io.is_empty() {
            return;
        }
    }
    assert!(io.is_empty(), "child claim closes");
}

#[test]
fn given_fd_three_claims_the_parents_backlog_and_leaves_the_parent_listening() {
    let mut sim = Sim::new(7, Config::calm());
    let parent = sim.spawn_process();
    let client = sim.spawn_process();
    let root = sim.root(parent, Handle::new(1));
    let listener = socket(&mut sim, parent);
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 8000));
    assert_eq!(call(&mut sim, parent, Op::Bind { fd: listener, addr: address }).result, Ok(Done::Bound(address)));
    assert_eq!(call(&mut sim, parent, Op::Listen { fd: listener, backlog: 2 }).result, Ok(Done::Nothing));
    for _ in 0..2_u32 {
        let connection = socket(&mut sim, client);
        assert_eq!(call(&mut sim, client, Op::Connect { fd: connection, addr: address }).result, Ok(Done::Nothing));
        let mut submissions = Queue::with_capacity(1);
        submissions.push(Submit {
            op: Token::new(1),
            kind: Op::Spawn {
                spawn: Box::new(Spawn {
                    program: Box::from(&b"service"[..]),
                    args: Box::new([]),
                    env: Box::new([]),
                    root,
                    dir: Box::new([]),
                    pipes: Box::new([Pipe { child: 3, way: Way::Given(Token::new(2)), parent: Some(listener) }]),
                }),
            },
        });
        sim.submit(parent, &mut submissions);
        let mut calls = Queue::with_capacity(1);
        sim.calls(&mut calls);
        let asked = calls.pop().expect("spawn call");
        let mut answers = Queue::with_capacity(1);
        answers.push(Answer { ticket: asked.ticket, result: Ok(Reply::Program(Program::Service)) });
        sim.answer(&mut answers);
        let mut completions = Queue::with_capacity(1);
        sim.reap(parent, &mut completions);
        let complete = completions.pop().expect("spawn completed");
        let Ok(Done::Spawned { pidfd, .. }) = complete.result else {
            unreachable!("the fake machine answered Service")
        };
        let (child, inherited) = sim.bind_service(parent, pidfd);
        assert_eq!(inherited, [(3, Fd::new(3))]);
        claim_and_accept(&mut sim, child, Fd::new(3), address);
        sim.finish_service(child, Exit::Code(0));
        sim.assert_no_open_fds(child);
        assert_eq!(call(&mut sim, parent, Op::Wait { pidfd, reap: false }).result, Ok(Done::Exit(Exit::Code(0))));
        assert_eq!(call(&mut sim, parent, Op::Wait { pidfd, reap: true }).result, Ok(Done::Exit(Exit::Code(0))));
        assert_eq!(call(&mut sim, parent, Op::Close { fd: pidfd }).result, Ok(Done::Nothing));
        assert_eq!(call(&mut sim, client, Op::Close { fd: connection }).result, Ok(Done::Nothing));
    }
    let connection = socket(&mut sim, client);
    call(&mut sim, client, Op::Connect { fd: connection, addr: address });
    claim_and_accept(&mut sim, parent, listener, address);
    call(&mut sim, client, Op::Close { fd: connection });
    // The root crosses the fake-machine seam when closed.
    let _closed = sim.cut(parent);
    sim.assert_no_open_fds(parent);
    sim.assert_no_open_fds(client);
}
