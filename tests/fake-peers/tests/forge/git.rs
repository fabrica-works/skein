//! The private codec over kernel loopback, sharing the peer's object database.
use super::fixtures as f;
use skein_fake_forge_domain::{self as domain, api};
use skein_fake_forge_protocol::{
    Limits,
    git::{self, Answer, Call, Name},
};
use skein_fake_peers::{Error, forge::Peer};
use skein_json::tokenizer::{self as json, Tokenizer};
use skein_lib::{
    Duration, Env, Queue, Time, Wall,
    stream::{Down, Held},
};
use skein_shell::{Clock, Host, Kernel, Wait};
use std::{
    io::{Read, Write},
    net::TcpStream,
    rc::Rc,
};

struct Loop {
    peer: Peer,
    kernel: Kernel,
    clock: Clock,
}
impl Loop {
    fn pass(&mut self) {
        self.kernel.reap(self.peer.completions());
        let now = self.clock.now();
        self.peer.iterate(now.now, now.wall);
        self.kernel.submit(self.peer.submissions(), Wait::No);
    }
    fn exchange(&mut self, call: &Call) -> Answer {
        let limits = f::protocol_limits();
        let encoded = git::client::encode(call, &limits).expect("call encoding");
        let mut stream = TcpStream::connect(self.peer.address().expect("listening")).expect("kernel loopback");
        let authorization = "Basic ZW5naW5lOjExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTExMTE=";
        let head = format!(
            "POST {} HTTP/1.1\r\nHost: localhost\r\nAuthorization: {authorization}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            std::str::from_utf8(&encoded.target).expect("target"),
            encoded.body.len()
        );
        stream.write_all(head.as_bytes()).expect("request head");
        stream.write_all(&encoded.body).expect("request body");
        stream.set_nonblocking(true).expect("one loop");
        let deadline = self.clock.now().now.saturating_add(Duration::from_secs(3));
        let mut response = Vec::new();
        loop {
            self.pass();
            let mut buffer = [0; 4096];
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => response.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("response: {error}"),
            }
            assert!(self.clock.now().now < deadline, "kernel loopback exchange settles");
        }
        assert!(response.starts_with(b"HTTP/1.1 200 "), "{response:?}");
        let body = &response[skein_lib::bytes::find(&response, b"\r\n\r\n").expect("HTTP head").saturating_add(4)..];
        decode(body, call.name(), &limits)
    }
}
fn decode(body: &[u8], name: Name, limits: &Limits) -> Answer {
    let mut decoder = git::client::decoder(name, 200, Time::ZERO, limits);
    let mut tokenizer = Tokenizer::new(&limits.documents.json);
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: limits.documents.json };
    let mut above = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(1);
    let mut input = Held::new(f::boxed(body));
    json::down(&mut tokenizer, &env, git::client::first(&decoder), &mut above, &mut below);
    loop {
        if let Some(event) = above.pop() {
            match git::client::step(&mut decoder, limits, &event) {
                git::client::Step::Demand(request) => json::down(&mut tokenizer, &env, request, &mut above, &mut below),
                git::client::Step::Decoded(answer) => return answer.expect("git success"),
                git::client::Step::Malformed => panic!("git response grammar"),
            }
        } else {
            let Some(Down::Demand { read, .. }) = below.pop() else { panic!("tokenizer demand") };
            json::up(&mut tokenizer, &env, input.answer(read).expect("body stream"), &mut above, &mut below);
        }
    }
}
#[test]
fn private_git_calls_cross_kernel_loopback_and_move_shared_branch() {
    let shared = f::shared();
    {
        let mut forge = shared.borrow_mut();
        forge.config.latency_min = Duration::ZERO;
        forge.config.latency_max = Duration::ZERO;
        let config = forge.config;
        domain::commit_tree(&mut forge.domain, &config, 1, None, Box::new([]), b"second")
            .expect("second stored commit");
    }
    let peer = Peer::new(f::address(), f::limits(), f::config(), f::protocol_limits(), Rc::clone(&shared), f::SEED)
        .expect("bounded peer")
        .without_records();
    let kernel = Kernel::open(skein_shell::Config { operations: peer.operations() }).expect("kernel ring");
    let mut world = Loop { peer, kernel, clock: Clock::new() };
    let deadline = world.clock.now().now.saturating_add(Duration::from_secs(3));
    while world.peer.address().is_none() {
        world.pass();
        assert!(world.clock.now().now < deadline, "listener binds");
    }
    let rows = [
        (
            Call::Create { repository: f::boxed(f::REPOSITORY), branch: f::boxed(f::BRANCH), commit: 1 },
            Answer::Created(api::Created::Created),
        ),
        (
            Call::Push {
                repository: f::boxed(f::REPOSITORY),
                branch: f::boxed(f::BRANCH),
                commit: 2,
                expected: Some(1),
            },
            Answer::Pushed(api::Pushed::Pushed),
        ),
        (Call::Branch { repository: f::boxed(f::REPOSITORY), branch: f::boxed(f::BRANCH) }, Answer::Commit(2)),
        (
            Call::CommitTree { parent: 2, merging: Some(1), tree: Box::new([]), message: f::boxed(b"\xffthird") },
            Answer::Stored(Some(3)),
        ),
    ];
    for (call, expected) in rows {
        assert_eq!(world.exchange(&call), expected);
    }
    let answer = world.exchange(&Call::Objects { commits: Box::new([3]) });
    let Answer::Objects { objects, graph } = answer else { panic!("objects") };
    assert_eq!(
        objects.as_ref(),
        &[git::Object {
            commit: 3,
            parent: Some(2),
            merge_parent: Some(1),
            tree: Box::new([]),
            message: f::boxed(b"\xffthird")
        }]
    );
    assert_eq!(graph.as_ref(), &[git::Parents { commit: 2, parent: Some(1), merge_parent: None }]);
    let forge = shared.borrow();
    assert_eq!(
        forge.domain.inspect(&forge.config, f::REPOSITORY, &api::Read::Branch { branch: f::boxed(f::BRANCH) }),
        Ok(api::Answer::Commit(2))
    );
    drop(forge);
    world.peer.shutdown();
    while !world.peer.is_empty() {
        world.pass();
        assert!(world.clock.now().now < deadline, "peer settles");
    }
    assert_eq!(world.kernel.in_flight(), 0);
}
#[test]
fn peer_git_tree_room_holds_at_bound_and_refuses_one_less() {
    let shared = f::shared();
    let bound = git::tree_bound(&shared.borrow().config.limits).expect("tree bound");
    for (label, writer, request, accepted) in [
        ("answer bound", bound.answer, u64::from(bound.request), true),
        ("answer short", bound.answer.saturating_sub(1), u64::from(bound.request), false),
        ("request short", bound.answer, u64::from(bound.request.saturating_sub(1)), false),
    ] {
        let mut limits = f::protocol_limits();
        limits.documents.writer.length = writer;
        limits.http.body = request;
        let result = Peer::new(f::address(), f::limits(), f::config(), limits, Rc::clone(&shared), f::SEED);
        assert_eq!(result.err(), if accepted { None } else { Some(Error::Limits) }, "{label}");
    }
}
