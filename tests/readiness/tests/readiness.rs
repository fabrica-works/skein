//! The readiness backend's own behaviours through the public kernel (shell.md, 9).
//! Peers share this thread; every backend is forced to readiness.

use std::collections::BTreeMap;
use std::io::Write;
use std::os::fd::{AsRawFd, IntoRawFd};
use std::os::unix::net::UnixStream;

use skein_io::kernel::{Addr, Complete, Done, Error, Family, Fd, Op, Submit};
use skein_lib::{Duration, Queue, Time, Token};
use skein_shell::{Backend, Clock, Config, Kernel, Wait};

// Two transfers or a target and its cancel, with room for setup and close.
const OPERATIONS: u32 = 4;
// A failed readiness must fail the test rather than hang its runner.
const PATIENCE: Duration = Duration::from_secs(2);
// One byte is enough to distinguish the two readers of one description.
const BYTE: usize = 1;
// Large enough to exceed loopback's socket buffers (task C; shell.md, 9).
const TRANSFER: usize = 8 << 20;
// A prime period below 256 exposes changes to the byte order across receives.
const PATTERN_PERIOD: usize = 251;
// Receives drain a bounded piece of the large transfer on each turn.
const PIECE: usize = 64 << 10;
// A short wait detects a stale interest without adding a long idle test.
const QUIET: Duration = Duration::from_millis(5);

struct World {
    kernel: Kernel,
    clock: Clock,
    submits: Queue<Submit>,
    out: Queue<Complete>,
    arrived: BTreeMap<Token, Complete>,
    next: u64,
}

impl World {
    fn new() -> Self {
        Self {
            kernel: Kernel::open_on(Config { operations: OPERATIONS }, Backend::Readiness)
                .expect("the forced readiness backend is usable"),
            clock: Clock::new(),
            submits: Queue::with_capacity(OPERATIONS),
            out: Queue::with_capacity(OPERATIONS),
            arrived: BTreeMap::new(),
            next: 0,
        }
    }

    fn start(&mut self, kind: Op) -> Token {
        self.next += 1;
        let op = Token::new(self.next);
        self.submits.push(Submit { op, kind });
        self.kernel.submit(&mut self.submits, Wait::No);
        assert!(self.submits.is_empty(), "the small fixture fits its operation bound");
        op
    }

    fn reap(&mut self) {
        self.kernel.reap(&mut self.out);
        while let Some(complete) = self.out.pop() {
            assert!(complete.is_valid(), "the readiness backend kept the record contract: {complete:?}");
            assert!(self.arrived.insert(complete.op, complete).is_none(), "one completion per operation");
        }
    }

    fn later(&self, span: Duration) -> Time {
        self.clock.now().now.saturating_add(span)
    }

    fn turn(&mut self, until: Time) {
        self.kernel.submit(&mut self.submits, Wait::Until(until));
        self.reap();
    }

    fn wait(&mut self, op: Token) -> Complete {
        let until = self.later(PATIENCE);
        loop {
            self.reap();
            if let Some(complete) = self.arrived.remove(&op) {
                return complete;
            }
            assert!(self.clock.now().now < until, "{op:?} completes within patience");
            self.turn(until);
        }
    }

    fn run(&mut self, kind: Op) -> Complete {
        let op = self.start(kind);
        self.wait(op)
    }

    fn close(&mut self, fd: Fd) {
        assert_eq!(self.run(Op::Close { fd }).result, Ok(Done::Nothing));
    }

    fn cancel(&mut self, target: Token) {
        assert_eq!(self.run(Op::Cancel { target }).result, Ok(Done::Nothing));
        assert_eq!(self.wait(target).result, Err(Error::Cancelled));
    }

    fn socket(&mut self) -> Fd {
        let Ok(Done::Fd(fd)) = self.run(Op::Socket { family: Family::Ipv4 }).result else {
            panic!("a loopback socket opens")
        };
        fd
    }

    fn listener(&mut self) -> (Fd, Addr) {
        let fd = self.socket();
        let addr = "127.0.0.1:0".parse().expect("loopback");
        let Ok(Done::Bound(addr)) = self.run(Op::Bind { fd, addr }).result else { panic!("loopback binds") };
        assert_eq!(self.run(Op::Listen { fd, backlog: 1 }).result, Ok(Done::Nothing));
        (fd, addr)
    }

    fn connection(&mut self) -> (Fd, Fd, Fd) {
        let (listener, addr) = self.listener();
        let client = self.socket();
        let accept = self.start(Op::Accept { fd: listener });
        assert_eq!(self.run(Op::Connect { fd: client, addr }).result, Ok(Done::Nothing));
        let Ok(Done::Accepted { fd: server, .. }) = self.wait(accept).result else {
            panic!("the loopback connection is accepted")
        };
        (listener, client, server)
    }
}

fn recv(fd: Fd) -> Op {
    Op::recv(fd, vec![0; BYTE].into_boxed_slice()).expect("one byte of room")
}

fn received(complete: Complete, expected: &[u8]) {
    assert_eq!(complete.result, Ok(Done::Count(1)));
    let Op::Recv { buf, .. } = complete.kind else { panic!("a receive record returns") };
    assert_eq!(&buf[..expected.len()], expected);
}

#[test]
fn a_record_made_at_submit_and_one_that_waits_complete_in_order() {
    let mut world = World::new();
    let (reader, mut peer) = UnixStream::pair().expect("a peer connection");
    let usage = world.start(Op::Usage);
    let waiting = world.start(recv(Fd::new(reader.as_raw_fd())));
    assert_eq!(world.kernel.in_flight(), 2, "the ready Usage and waiting Recv occupy their slots");
    world.kernel.reap(&mut world.out);
    let complete = world.out.pop().expect("Usage arrives at the next reap");
    assert_eq!(complete.op, usage);
    assert!(matches!(complete.result, Ok(Done::Usage(_))));
    assert!(world.out.is_empty(), "Recv still waits");
    assert_eq!(world.kernel.in_flight(), 1);
    peer.write_all(b"a").expect("the peer sends one byte");
    received(world.wait(waiting), b"a");
    assert_eq!(world.kernel.in_flight(), 0);
}

#[test]
fn a_readiness_another_reader_took_first_waits_again() {
    let mut world = World::new();
    let (reader, mut peer) = UnixStream::pair().expect("a shared peer connection");
    reader.set_nonblocking(true).expect("the shared description is nonblocking");
    let clone = reader.try_clone().expect("two numbers for one description");
    let first = world.start(recv(Fd::new(reader.as_raw_fd())));
    let second = world.start(recv(Fd::new(clone.as_raw_fd())));
    peer.write_all(b"a").expect("only one reader can take this byte");
    let until = world.later(PATIENCE);
    while world.arrived.is_empty() {
        assert!(world.clock.now().now < until, "one reader completes");
        world.turn(until);
    }
    assert_eq!(world.arrived.len(), 1, "the other reader retries EAGAIN and waits again");
    assert_eq!(world.kernel.in_flight(), 1);
    let (done, complete) = world.arrived.pop_first().expect("one reader completed");
    received(complete, b"a");
    let waiting = if done == first { second } else { first };
    peer.write_all(b"b").expect("the next byte wakes the remaining reader");
    received(world.wait(waiting), b"b");
    assert_eq!(world.kernel.in_flight(), 0);
}

#[test]
fn a_large_transfer_completes_through_short_sends() {
    let mut world = World::new();
    let (listener, client, server) = world.connection();
    let bytes: Box<[u8]> = (0..TRANSFER).map(|at| u8::try_from(at % PATTERN_PERIOD).unwrap()).collect();
    let expected = bytes.clone();
    // No receive is submitted until the first send has filled the socket buffers.
    let first = world.run(Op::send(client, bytes, 0).expect("a nonempty transfer"));
    let (Ok(Done::Count(n)), Op::Send { bytes, .. }) = (first.result, first.kind) else {
        panic!("the first send makes progress")
    };
    assert!(usize::try_from(n).unwrap() < TRANSFER, "at least one send is short");
    let mut from = n;
    let mut outgoing = Some(bytes);
    let mut sending = None;
    let mut receiving = None;
    let mut got = Vec::with_capacity(TRANSFER);
    let until = world.later(PATIENCE);
    while got.len() < TRANSFER || sending.is_some() {
        assert!(world.clock.now().now < until, "the transfer finishes within patience");
        if sending.is_none() && usize::try_from(from).unwrap() < TRANSFER {
            sending = Some(world.start(Op::send(client, outgoing.take().unwrap(), from).unwrap()));
        }
        if receiving.is_none() && got.len() < TRANSFER {
            receiving = Some(world.start(Op::recv(server, vec![0; PIECE].into_boxed_slice()).unwrap()));
        }
        world.turn(until);
        if let Some(op) = sending
            && let Some(complete) = world.arrived.remove(&op)
        {
            let (Ok(Done::Count(n)), Op::Send { bytes, .. }) = (complete.result, complete.kind) else {
                panic!("a continued send makes progress")
            };
            from += n;
            outgoing = Some(bytes);
            sending = None;
        }
        if let Some(op) = receiving
            && let Some(complete) = world.arrived.remove(&op)
        {
            let (Ok(Done::Count(n)), Op::Recv { buf, .. }) = (complete.result, complete.kind) else {
                panic!("the peer receives bytes")
            };
            assert_ne!(n, 0, "the stream remains open");
            got.extend_from_slice(&buf[..usize::try_from(n).unwrap()]);
            receiving = None;
        }
    }
    assert_eq!(usize::try_from(from).unwrap(), TRANSFER);
    assert_eq!(got, expected.as_ref(), "every byte arrives in order");
    for fd in [client, server, listener] {
        world.close(fd);
    }
    assert_eq!(world.kernel.in_flight(), 0);
}

#[test]
fn a_closed_number_reused_reports_no_readiness_of_the_old_description() {
    let mut world = World::new();
    let (reader, mut old_peer) = UnixStream::pair().expect("the old connection");
    let child = reader.try_clone().expect("a child keeps the old description alive");
    let old = Fd::new(reader.into_raw_fd());
    let waiting = world.start(recv(old));
    world.reap();
    assert_eq!(world.kernel.in_flight(), 1, "the old description has a waiter");
    world.cancel(waiting);
    world.close(old);
    let (reader, mut peer) = UnixStream::pair().expect("a new connection reuses the free number");
    assert_eq!(reader.as_raw_fd(), old.raw(), "the new description takes the same number");
    let waiting = world.start(recv(Fd::new(reader.as_raw_fd())));
    old_peer.write_all(b"a").expect("only the old description is readable");
    world.turn(world.later(QUIET));
    assert!(!world.arrived.contains_key(&waiting), "no old readiness completes the new receive");
    assert_eq!(world.kernel.in_flight(), 1, "the new description still waits");
    peer.write_all(b"b").expect("the new description becomes readable");
    received(world.wait(waiting), b"b");
    drop(child);
}

#[test]
fn dropping_the_kernel_with_records_waiting_frees_them() {
    let mut world = World::new();
    let (listener, client, server) = world.connection();
    world.start(recv(server));
    world.start(Op::Accept { fd: listener });
    world.reap();
    assert_eq!(world.kernel.in_flight(), 2, "a receive and an accept wait");
    drop(world);
    let mut after = World::new();
    for fd in [server, client, listener] {
        after.close(fd);
    }
    assert_eq!(after.kernel.in_flight(), 0);
}
