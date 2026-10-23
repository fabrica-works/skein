//! Safe completion emulation over non-blocking calls and epoll (shell.md, 7).
//! Records move only between synchronous calls; the kernel keeps no pointer
//! into a slot, so dropping this table frees every record even while waiting.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use skein_io::kernel::{Complete, Done, Error, Family, Fd, Op, Submit};
use skein_lib::{Queue, Token};

use crate::linux::{self, epoll::Epoll};
use crate::{Config, OpenError, Wait, ring};

#[derive(Debug)]
pub(crate) struct Readiness {
    slots: Box<[Slot]>,
    free: Vec<u32>,
    tokens: BTreeMap<Token, u32>,
    ready: VecDeque<(u32, u32)>,
    waiters: BTreeMap<Fd, Waiters>,
    nonblocking: BTreeSet<Fd>,
    epoll: Epoll,
    resubmitted: u64,
}

#[derive(Debug)]
struct Slot {
    generation: u32,
    record: Option<Submit>,
    result: Option<Result<Done, Error>>,
}

#[derive(Clone, Copy, Debug)]
struct Waiters {
    read: Option<u32>,
    write: Option<u32>,
}

#[derive(Clone, Copy)]
enum Side {
    Read,
    Write,
}

impl Readiness {
    pub(crate) fn open(config: Config) -> Result<Self, OpenError> {
        if config.operations == 0 {
            return Err(OpenError::NoOperations);
        }
        if !linux::at_floor().map_err(OpenError::Setup)? {
            return Err(OpenError::BelowFloor);
        }
        let epoll = Epoll::open(config.operations).map_err(OpenError::Setup)?;
        let capacity = usize::try_from(config.operations).expect("u32 fits usize");
        let slots = (0..config.operations).map(|_| Slot { generation: 0, record: None, result: None }).collect();
        Ok(Self {
            slots,
            free: (0..config.operations).rev().collect(),
            tokens: BTreeMap::new(),
            ready: VecDeque::with_capacity(capacity),
            waiters: BTreeMap::new(),
            nonblocking: BTreeSet::new(),
            epoll,
            resubmitted: 0,
        })
    }

    pub(crate) fn in_flight(&self) -> u32 {
        u32::try_from(self.tokens.len()).expect("at most Config's u32 operations")
    }

    pub(crate) fn room(&self) -> u32 {
        u32::try_from(self.free.len()).expect("at most Config's u32 slots")
    }

    pub(crate) const fn resubmitted(&self) -> u64 {
        self.resubmitted
    }

    fn slot(&mut self, index: u32) -> &mut Slot {
        self.slots.get_mut(usize::try_from(index).expect("u32 fits usize")).expect("an allocated slot")
    }

    fn finish(&mut self, index: u32, result: Result<Done, Error>) {
        let slot = self.slot(index);
        assert!(slot.result.is_none(), "an operation completes once (skein_io::kernel)");
        slot.result = Some(result);
        let generation = slot.generation;
        self.ready.push_back((index, generation));
    }

    pub(crate) fn reap(&mut self, out: &mut Queue<Complete>) {
        while out.room() > 0 {
            let Some((index, generation)) = self.ready.pop_front() else {
                break;
            };
            let slot = self.slot(index);
            assert_eq!(slot.generation, generation, "a ready entry names this generation");
            let Submit { op, kind } = slot.record.take().expect("a ready slot holds its record");
            let result = slot.result.take().expect("a ready slot holds its result");
            slot.generation = slot.generation.wrapping_add(1);
            let removed = self.tokens.remove(&op);
            assert_eq!(removed, Some(index), "every operation in flight has its token");
            self.free.push(index);
            out.push(Complete { op, kind, result });
        }
    }

    pub(crate) fn submit(&mut self, records: &mut Queue<Submit>, wait: Wait) {
        while !self.free.is_empty() {
            let Some(record) = records.pop() else {
                break;
            };
            self.start(record);
        }
        let timeout = if self.ready.is_empty() {
            match wait {
                Wait::No => Some(0),
                Wait::Until(deadline) => {
                    // The deadline and timeout are integer nanoseconds: their positive
                    // difference needs no rounding, and zero means the deadline passed.
                    let now = ring::clock_nanos(libc::CLOCK_MONOTONIC).expect("the monotonic clock is available");
                    Some(deadline.as_nanos().saturating_sub(now))
                }
                Wait::Forever => {
                    assert!(!self.waiters.is_empty(), "waiting forever requires a waiter (shell.md, 7.4)");
                    None
                }
            }
        } else {
            Some(0)
        };
        let count = match self.epoll.wait(timeout) {
            Ok(count) => count,
            Err(libc::EINTR) => return,
            Err(errno) => panic!("epoll refused a wait: {errno}"),
        };
        for index in 0..count {
            let event = self.epoll.event(index);
            let Some(waiters) = self.waiters.get(&event.fd).copied() else {
                continue;
            };
            if event.read
                && let Some(slot) = waiters.read
            {
                self.resume(event.fd, Side::Read, slot);
            }
            if event.write
                && let Some(slot) = waiters.write
            {
                self.resume(event.fd, Side::Write, slot);
            }
        }
    }

    fn start(&mut self, record: Submit) {
        assert!(record.kind.is_valid(), "io submits only valid records (skein_io::kernel, broken invariants)");
        assert!(!self.tokens.contains_key(&record.op), "io never reuses a token in flight (skein_io::kernel)");
        let side = side(&record.kind);
        if let Some((fd, side)) = side {
            self.assert_side_free(fd, side);
        }
        if let Op::Close { fd } = &record.kind {
            assert!(!self.waiters.contains_key(fd), "no Close beside a waiter (skein_io::kernel, broken invariants)");
            let _removed = self.nonblocking.remove(fd);
        }
        let index = self.free.pop().expect("a slot is free before taking a record");
        let _previous = self.tokens.insert(record.op, index);
        self.slot(index).record = Some(record);
        if let Op::Cancel { target } = &self.slot(index).record.as_ref().expect("just inserted").kind {
            let target = *target;
            let result = self.cancel(target);
            self.finish(index, result);
            return;
        }
        let attempt_now = match side {
            None => true,
            Some((fd, _)) => {
                self.nonblocking.contains(&fd)
                    || !matches!(
                        self.slot(index).record.as_ref().expect("just inserted").kind,
                        Op::Accept { .. } | Op::Connect { .. }
                    )
            }
        };
        if attempt_now && self.attempt(index, false) {
            return;
        }
        let (fd, side) = side.expect("only a stream operation can wait");
        self.add_waiter(fd, side, index);
    }

    fn assert_side_free(&self, fd: Fd, side: Side) {
        if let Some(waiters) = self.waiters.get(&fd) {
            let existing = match side {
                Side::Read => waiters.read,
                Side::Write => waiters.write,
            };
            assert!(existing.is_none(), "at most one waiter per descriptor side (skein_io::kernel, broken invariants)");
        }
    }

    fn add_waiter(&mut self, fd: Fd, side: Side, index: u32) {
        let existed = self.waiters.contains_key(&fd);
        let waiters = self.waiters.entry(fd).or_insert(Waiters { read: None, write: None });
        match side {
            Side::Read => waiters.read = Some(index),
            Side::Write => waiters.write = Some(index),
        }
        self.update(fd, existed);
    }

    fn remove_waiter(&mut self, fd: Fd, side: Side) {
        let waiters = self.waiters.get_mut(&fd).expect("a waiting descriptor");
        match side {
            Side::Read => waiters.read = None,
            Side::Write => waiters.write = None,
        }
        self.update(fd, true);
    }

    fn update(&mut self, fd: Fd, existed: bool) {
        let waiters = self.waiters.get(&fd).expect("a descriptor's waiters");
        self.epoll
            .interest(fd, existed, waiters.read.is_some(), waiters.write.is_some())
            .expect("epoll accepts a live pollable descriptor's interest");
        if waiters.read.is_none() && waiters.write.is_none() {
            let _removed = self.waiters.remove(&fd);
        }
    }

    fn resume(&mut self, fd: Fd, side: Side, index: u32) {
        if self.attempt(index, true) {
            self.remove_waiter(fd, side);
        }
    }

    fn cancel(&mut self, target: Token) -> Result<Done, Error> {
        let Some(index) = self.tokens.get(&target).copied() else {
            return Err(Error::TooLate);
        };
        let slot = self.slot(index);
        if slot.result.is_some() {
            return Err(Error::TooLate);
        }
        let (fd, side) =
            side(&slot.record.as_ref().expect("an in-flight record").kind).expect("only a stream operation waits");
        self.remove_waiter(fd, side);
        self.finish(index, Err(Error::Cancelled));
        Ok(Done::Nothing)
    }

    /// True once complete; false when the record still needs readiness.
    fn attempt(&mut self, index: u32, resumed: bool) -> bool {
        let kind = &mut self.slot(index).record.as_mut().expect("an in-flight record").kind;
        let result = perform(kind, resumed);
        match result {
            Err(errno)
                if side(kind).is_some()
                    && (errno == libc::EAGAIN
                        || errno == libc::EWOULDBLOCK
                        || (errno == libc::EINPROGRESS && matches!(kind, Op::Connect { .. }))) =>
            {
                false
            }
            result => {
                let mut result = result.map_err(|errno| ring::error(kind, errno, false));
                if let (Op::Socket { family }, Ok(Done::Fd(fd))) = (kind, &result) {
                    let configured = linux::retry(|| linux::setsockopt(fd.raw(), libc::IPPROTO_TCP, libc::TCP_NODELAY))
                        .and_then(|()| {
                            if *family == Family::Ipv6 {
                                linux::retry(|| linux::setsockopt(fd.raw(), libc::IPPROTO_IPV6, libc::IPV6_V6ONLY))
                            } else {
                                Ok(())
                            }
                        });
                    if let Err(errno) = configured {
                        let _closed = linux::close(*fd);
                        result = Err(Error::Other(errno));
                    }
                }
                match result {
                    Ok(Done::Fd(fd) | Done::Accepted { fd, .. }) => {
                        let _inserted = self.nonblocking.insert(fd);
                    }
                    Ok(
                        Done::Nothing
                        | Done::Bound(_)
                        | Done::Count(_)
                        | Done::Stat(_)
                        | Done::Exit(_)
                        | Done::Spawned { .. }
                        | Done::Signalled(_)
                        | Done::Usage(_)
                        | Done::Window(_),
                    )
                    | Err(_) => {}
                }
                self.finish(index, result);
                true
            }
        }
    }
}

fn side(kind: &Op) -> Option<(Fd, Side)> {
    match kind {
        Op::Recv { fd, .. } | Op::Accept { fd } | Op::PipeRead { fd, .. } | Op::ReadSignal { fd } => {
            Some((*fd, Side::Read))
        }
        Op::Wait { pidfd, .. } => Some((*pidfd, Side::Read)),
        Op::Send { fd, .. } | Op::PipeWrite { fd, .. } | Op::Connect { fd, .. } => Some((*fd, Side::Write)),
        Op::Socket { .. }
        | Op::Bind { .. }
        | Op::Listen { .. }
        | Op::Shutdown { .. }
        | Op::Close { .. }
        | Op::Open { .. }
        | Op::Read { .. }
        | Op::Write { .. }
        | Op::Append { .. }
        | Op::Truncate { .. }
        | Op::Sync { .. }
        | Op::Lock { .. }
        | Op::Stat { .. }
        | Op::Rename { .. }
        | Op::Remove { .. }
        | Op::MakeDirectory { .. }
        | Op::List { .. }
        | Op::Spawn { .. }
        | Op::Signal { .. }
        | Op::Window { .. }
        | Op::Usage
        | Op::Cancel { .. } => None,
    }
}

fn perform(kind: &mut Op, resumed: bool) -> Result<Done, i32> {
    match kind {
        Op::Socket { family } => linux::socket(*family).map(Done::Fd),
        Op::Bind { fd, addr } => {
            linux::retry(|| linux::setsockopt(fd.raw(), libc::SOL_SOCKET, libc::SO_REUSEADDR))?;
            linux::bind(*fd, addr)?;
            let mut storage = ring::zeroed_storage();
            let mut len = 0;
            linux::retry(|| linux::getsockname(fd.raw(), &mut storage, &mut len))?;
            let bound = ring::decode_addr(&storage, len).ok_or(libc::EAFNOSUPPORT)?;
            Ok(Done::Bound(bound))
        }
        Op::Listen { fd, backlog } => linux::listen(*fd, *backlog).map(|()| Done::Nothing),
        Op::Accept { fd } => {
            let (fd, peer) = linux::accept(*fd)?;
            let configured = linux::retry(|| linux::setsockopt(fd.raw(), libc::IPPROTO_TCP, libc::TCP_NODELAY));
            assert!(configured.is_ok(), "TCP_NODELAY applies to every accepted TCP socket: {configured:?}");
            Ok(Done::Accepted { fd, peer })
        }
        Op::Connect { fd, addr } => {
            if resumed {
                match linux::getsockopt(*fd)? {
                    0 => Ok(Done::Nothing),
                    errno => Err(errno),
                }
            } else {
                linux::connect(*fd, addr).map(|()| Done::Nothing)
            }
        }
        Op::Recv { fd, buf } => linux::recv(*fd, buf).map(Done::Count),
        Op::Send { fd, bytes, from } => {
            let from = usize::try_from(*from).expect("u32 fits usize");
            linux::send(*fd, bytes.get(from..).expect("a valid Send has bytes left")).map(Done::Count)
        }
        Op::Shutdown { fd } => linux::shutdown(*fd).map(|()| Done::Nothing),
        Op::Close { fd } => linux::close(*fd).map(|()| Done::Nothing),
        // transitional: M2's readiness task makes these (shell.md 7.3)
        Op::Open { .. }
        | Op::Read { .. }
        | Op::Write { .. }
        | Op::Append { .. }
        | Op::Truncate { .. }
        | Op::Sync { .. }
        | Op::Lock { .. }
        | Op::Stat { .. }
        | Op::Rename { .. }
        | Op::Remove { .. }
        | Op::MakeDirectory { .. }
        | Op::List { .. }
        | Op::Spawn { .. }
        | Op::Signal { .. }
        | Op::Wait { .. }
        | Op::Window { .. }
        | Op::Usage
        | Op::PipeRead { .. }
        | Op::PipeWrite { .. }
        | Op::ReadSignal { .. } => Err(libc::ENOSYS),
        Op::Cancel { .. } => unreachable!("cancel is settled against the table at submit"),
    }
}

#[cfg(test)]
mod tests {
    use super::Readiness;
    use crate::{Clock, Config, Wait, linux};
    use skein_io::kernel::{Done, Error, Family, Fd, Op, Submit};
    use skein_lib::{Duration, Queue, Token};

    fn submit(kernel: &mut Readiness, token: u64, kind: Op) {
        let mut records = Queue::with_capacity(1);
        records.push(Submit { op: Token::new(token), kind });
        kernel.submit(&mut records, Wait::No);
        assert!(records.is_empty(), "the test kernel has room");
    }

    fn listener() -> Fd {
        let fd = linux::socket(Family::Ipv4).expect("a test socket");
        linux::bind(fd, &"127.0.0.1:0".parse().expect("loopback address")).expect("bind loopback");
        linux::listen(fd, 1).expect("listen on loopback");
        fd
    }

    #[test]
    fn ready_records_hold_slots_until_reaped_and_preserve_order() {
        let mut kernel = Readiness::open(Config { operations: 2 }).expect("epoll is usable");
        let mut records = Queue::with_capacity(3);
        for token in 1..=3 {
            records.push(Submit { op: Token::new(token), kind: Op::Usage });
        }
        kernel.submit(&mut records, Wait::Forever);
        assert_eq!(records.len(), 1, "submit leaves the record past its capacity");
        assert_eq!((kernel.in_flight(), kernel.room()), (2, 0));
        let mut out = Queue::with_capacity(1);
        kernel.reap(&mut out);
        assert_eq!(out.pop().expect("oldest ready completion").op, Token::new(1));
        assert_eq!((kernel.in_flight(), kernel.room()), (1, 1));
        kernel.submit(&mut records, Wait::No);
        for token in 2..=3 {
            kernel.reap(&mut out);
            let complete = out.pop().expect("the next completion");
            assert_eq!(complete.op, Token::new(token));
            assert_eq!(complete.result, Err(Error::Other(libc::ENOSYS)));
        }
        assert_eq!((kernel.in_flight(), kernel.room()), (0, 2));
    }

    #[test]
    fn transitional_inherited_streams_complete_without_registering_interest() {
        let mut kernel = Readiness::open(Config { operations: 2 }).expect("epoll is usable");
        submit(&mut kernel, 1, Op::PipeRead { fd: Fd::new(0), buf: vec![0; 8].into_boxed_slice() });
        submit(&mut kernel, 2, Op::ReadSignal { fd: Fd::new(0) });
        assert!(kernel.waiters.is_empty(), "M2's calls are refused at submit");
        let mut out = Queue::with_capacity(2);
        kernel.reap(&mut out);
        for _ in 0..2 {
            assert_eq!(out.pop().expect("a transitional completion").result, Err(Error::Other(libc::ENOSYS)));
        }
    }

    #[test]
    fn cancelled_slots_can_be_reused_without_cancelling_their_next_record() {
        let mut kernel = Readiness::open(Config { operations: 3 }).expect("epoll is usable");
        let fd = listener();
        assert!(kernel.nonblocking.insert(fd));
        submit(&mut kernel, 1, Op::Accept { fd });
        submit(&mut kernel, 2, Op::Cancel { target: Token::new(1) });
        assert!(kernel.waiters.is_empty(), "cancelling removes the last interest");
        let mut out = Queue::with_capacity(1);
        kernel.reap(&mut out);
        let target = out.pop().expect("target precedes cancel");
        assert_eq!((target.op, target.result), (Token::new(1), Err(Error::Cancelled)));
        submit(&mut kernel, 3, Op::Accept { fd });
        submit(&mut kernel, 4, Op::Cancel { target: Token::new(1) });
        for expected in [Ok(Done::Nothing), Err(Error::TooLate)] {
            kernel.reap(&mut out);
            assert_eq!(out.pop().expect("cancel completion").result, expected);
        }
        assert_eq!(kernel.in_flight(), 1, "the new accept still waits");
        submit(&mut kernel, 5, Op::Cancel { target: Token::new(3) });
        let mut out = Queue::with_capacity(3);
        kernel.reap(&mut out);
        submit(&mut kernel, 6, Op::Close { fd });
        kernel.reap(&mut out);
        assert_eq!(kernel.in_flight(), 0);
        assert!(kernel.nonblocking.is_empty(), "Close forgets the descriptor");
    }

    #[test]
    #[should_panic(expected = "at most one waiter per descriptor side")]
    fn a_second_waiter_on_the_same_side_breaks_the_contract() {
        let mut kernel = Readiness::open(Config { operations: 2 }).expect("epoll is usable");
        let fd = listener();
        assert!(kernel.nonblocking.insert(fd));
        submit(&mut kernel, 1, Op::Accept { fd });
        submit(&mut kernel, 2, Op::Accept { fd });
    }

    #[test]
    #[should_panic(expected = "no Close beside a waiter")]
    fn a_close_beside_a_waiter_breaks_the_contract() {
        let mut kernel = Readiness::open(Config { operations: 2 }).expect("epoll is usable");
        let fd = listener();
        assert!(kernel.nonblocking.insert(fd));
        submit(&mut kernel, 1, Op::Accept { fd });
        submit(&mut kernel, 2, Op::Close { fd });
    }

    #[test]
    fn an_idle_deadline_does_not_wake_early() {
        let mut kernel = Readiness::open(Config { operations: 1 }).expect("epoll is usable");
        let clock = Clock::new();
        for _ in 0..4 {
            let deadline = clock.now().now.saturating_add(Duration::from_millis(2));
            kernel.submit(&mut Queue::with_capacity(0), Wait::Until(deadline));
            assert!(clock.now().now >= deadline, "epoll must not wake before the deadline");
        }
    }
}
