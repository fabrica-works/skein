//! Safe completion emulation over non-blocking calls and descriptor readiness (shell.md, 7).
//! Records move only between synchronous calls; the kernel keeps no pointer
//! into a slot, so dropping this table frees every record even while waiting.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use skein_io::kernel::{Complete, Done, Error, Family, Fd, Op, OpenHow, Submit};
use skein_lib::{Queue, Token};

use crate::system::Poller;
use crate::{Config, OpenError, Wait, calls, system};

#[derive(Debug)]
pub(crate) struct Readiness {
    slots: Box<[Slot]>,
    free: Vec<u32>,
    tokens: BTreeMap<Token, u32>,
    ready: VecDeque<(u32, u32)>,
    waiters: BTreeMap<Fd, Waiters>,
    nonblocking: BTreeSet<Fd>,
    poller: Poller,
    listing: system::Listings,
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
        if !calls::release_at_floor(&system::release().map_err(OpenError::Setup)?, system::FLOOR)
            .map_err(OpenError::Setup)?
        {
            return Err(OpenError::BelowFloor);
        }
        system::process::block_sigpipe().map_err(OpenError::Setup)?;
        let poller = Poller::open(config.operations).map_err(OpenError::Setup)?;
        let capacity = usize::try_from(config.operations).expect("u32 fits usize");
        let slots = (0..config.operations).map(|_| Slot { generation: 0, record: None, result: None }).collect();
        Ok(Self {
            slots,
            free: (0..config.operations).rev().collect(),
            tokens: BTreeMap::new(),
            ready: VecDeque::with_capacity(capacity),
            waiters: BTreeMap::new(),
            nonblocking: BTreeSet::new(),
            poller,
            listing: system::Listings::new(),
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
                    let now = system::clock_nanos(libc::CLOCK_MONOTONIC).expect("the monotonic clock is available");
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
        let count = match self.poller.wait(timeout) {
            Ok(count) => count,
            Err(libc::EINTR) => return,
            Err(errno) => panic!("the poller refused a wait: {errno}"),
        };
        for index in 0..count {
            let event: system::Event = self.poller.event(index);
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
        calls::check_path_access(&record.kind);
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
        let kind = &self.slot(index).record.as_ref().expect("just inserted").kind;
        let attempt_now = side.is_none()
            || matches!(kind, Op::Recv { .. } | Op::Send { .. } | Op::Wait { .. })
            || side.is_some_and(|(fd, _)| self.nonblocking.contains(&fd));
        if attempt_now && self.attempt(index, false, true) {
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
        let mut waiters = self.waiters.get(&fd).copied().unwrap_or(Waiters { read: None, write: None });
        match side {
            Side::Read => waiters.read = Some(index),
            Side::Write => waiters.write = Some(index),
        }
        match self.poller.interest(fd, existed, waiters.read.is_some(), waiters.write.is_some()) {
            Ok(()) => {
                let _previous = self.waiters.insert(fd, waiters);
            }
            Err(system::UNWATCHABLE) => {
                // A descriptor the poller cannot watch makes its call without registering.
                assert!(self.attempt(index, false, false), "an unwatchable descriptor completes its one call");
            }
            Err(errno) => {
                let kind = &self.slot(index).record.as_ref().expect("an in-flight record").kind;
                let error = calls::error(kind, errno, false);
                self.finish(index, Err(error));
            }
        }
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
        self.poller
            .interest(fd, existed, waiters.read.is_some(), waiters.write.is_some())
            .expect("removing a waiter preserves the live descriptor and its existing interest");
        if waiters.read.is_none() && waiters.write.is_none() {
            let _removed = self.waiters.remove(&fd);
        }
    }

    fn resume(&mut self, fd: Fd, side: Side, index: u32) {
        if self.attempt(index, true, true) {
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
    fn attempt(&mut self, index: u32, resumed: bool, waitable: bool) -> bool {
        let record = self.slot(index).record.as_ref().expect("an in-flight record");
        let nonblocking = side(&record.kind).is_some_and(|(fd, _)| self.nonblocking.contains(&fd));
        let slot = self.slots.get_mut(usize::try_from(index).expect("u32 fits usize")).expect("an allocated slot");
        let kind = &mut slot.record.as_mut().expect("an in-flight record").kind;
        let mut again = 0;
        let result = loop {
            let result = perform(kind, resumed, nonblocking, &mut self.listing);
            // Open retries at openat2, before the descriptor check; repeating a
            // successful create after that check failed could meet its own file.
            // Linux releases a closed descriptor even when close reports EINTR.
            if result == Err(Error::Other(libc::EINTR)) && !matches!(kind, Op::Open { .. } | Op::Close { .. }) {
                continue;
            }
            if matches!(kind, Op::Open { .. }) && result == Err(Error::Other(libc::EAGAIN)) && again < calls::AGAIN {
                again = again.saturating_add(1);
                self.resubmitted = self.resubmitted.saturating_add(1);
            } else {
                break result;
            }
        };
        match result {
            Err(Error::Other(errno))
                if waitable
                    && side(kind).is_some()
                    && (errno == libc::EAGAIN
                        || errno == libc::EWOULDBLOCK
                        || (errno == libc::EINPROGRESS && matches!(kind, Op::Connect { .. }))) =>
            {
                false
            }
            mut result => {
                if let (Op::Socket { family }, Ok(Done::Fd(fd))) = (&*kind, &result) {
                    let configured =
                        system::retry(|| system::setsockopt(fd.raw(), libc::IPPROTO_TCP, libc::TCP_NODELAY)).and_then(
                            |()| {
                                if *family == Family::Ipv6 {
                                    system::retry(|| {
                                        system::setsockopt(fd.raw(), libc::IPPROTO_IPV6, libc::IPV6_V6ONLY)
                                    })
                                } else {
                                    Ok(())
                                }
                            },
                        );
                    if let Err(errno) = configured {
                        let _closed = system::close(*fd);
                        result = Err(Error::Other(errno));
                    }
                }
                match (&*kind, &result) {
                    (Op::Socket { .. }, Ok(Done::Fd(fd))) | (Op::Accept { .. }, Ok(Done::Accepted { fd, .. })) => {
                        let _inserted = self.nonblocking.insert(*fd);
                    }
                    (Op::Spawn { spawn }, Ok(Done::Spawned { .. })) => {
                        for pipe in &spawn.pipes {
                            let fd = pipe.parent.expect("a successful Spawn fills each parent end");
                            let configured = system::nonblocking(fd);
                            assert!(
                                configured.is_ok(),
                                "O_NONBLOCK applies to each parent pipe end Spawn just made: {configured:?}"
                            );
                            let _inserted = self.nonblocking.insert(fd);
                        }
                    }
                    _ => {}
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

fn perform(kind: &mut Op, resumed: bool, nonblocking: bool, listing: &mut system::Listings) -> Result<Done, Error> {
    match kind {
        Op::Open { root, path, how } => {
            let raw = system::files::open(*root, path, *how).map_err(|errno| calls::error(kind, errno, false))?;
            if matches!(kind, Op::Open { how: OpenHow::PathNoFollow, .. }) {
                Ok(Done::Fd(Fd::new(raw)))
            } else {
                system::opened(raw)
            }
        }
        Op::Spawn { spawn } => system::process::spawn(spawn),
        Op::Signal { pidfd, signal, to } => system::process::signal_child(*pidfd, *signal, *to),
        Op::Usage => system::process::usage().map(Done::Usage),
        Op::Window { fd } => system::terminal::size(*fd).map(Done::Window),
        Op::Wait { pidfd, reap } => {
            let result = system::process::poll_child(*pidfd, *reap);
            if let Err(Error::Other(errno)) = &result {
                return Err(calls::error(kind, *errno, false));
            }
            result.and_then(|exit| exit.map(Done::Exit).ok_or(Error::Other(libc::EAGAIN)))
        }
        Op::ReadSignal { fd } => system::read_signal(*fd),
        Op::Socket { .. }
        | Op::Bind { .. }
        | Op::Listen { .. }
        | Op::Accept { .. }
        | Op::Connect { .. }
        | Op::Recv { .. }
        | Op::Send { .. }
        | Op::Shutdown { .. }
        | Op::Close { .. }
        | Op::Read { .. }
        | Op::Write { .. }
        | Op::Append { .. }
        | Op::Truncate { .. }
        | Op::Sync { .. }
        | Op::Stat { .. }
        | Op::Rename { .. }
        | Op::Remove { .. }
        | Op::MakeDirectory { .. }
        | Op::List { .. }
        | Op::Lock { .. }
        | Op::PipeRead { .. }
        | Op::PipeWrite { .. }
        | Op::Cancel { .. } => {
            perform_call(kind, resumed, nonblocking, listing).map_err(|errno| calls::error(kind, errno, false))
        }
    }
}

fn perform_call(kind: &mut Op, resumed: bool, nonblocking: bool, listing: &mut system::Listings) -> Result<Done, i32> {
    match kind {
        Op::Socket { family } => system::socket(*family).map(Done::Fd),
        Op::Bind { fd, addr } => {
            system::retry(|| system::setsockopt(fd.raw(), libc::SOL_SOCKET, libc::SO_REUSEADDR))?;
            system::bind(*fd, addr)?;
            let bound = system::bound_addr(*fd)?;
            Ok(Done::Bound(bound))
        }
        Op::Listen { fd, backlog } => system::listen(*fd, *backlog).map(|()| Done::Nothing),
        Op::Accept { fd } => {
            let (fd, peer) = system::accept(*fd)?;
            let configured = system::retry(|| system::setsockopt(fd.raw(), libc::IPPROTO_TCP, libc::TCP_NODELAY));
            assert!(configured.is_ok(), "TCP_NODELAY applies to every accepted TCP socket: {configured:?}");
            Ok(Done::Accepted { fd, peer })
        }
        Op::Connect { fd, addr } => {
            if resumed {
                match system::getsockopt(*fd)? {
                    0 => Ok(Done::Nothing),
                    errno => Err(errno),
                }
            } else {
                system::connect(*fd, addr).map(|()| Done::Nothing)
            }
        }
        Op::Recv { fd, buf } => system::recv(*fd, buf).map(Done::Count),
        Op::Send { fd, bytes, from } => {
            let from = usize::try_from(*from).expect("u32 fits usize");
            system::send(*fd, bytes.get(from..).expect("a valid Send has bytes left")).map(Done::Count)
        }
        Op::Shutdown { fd } => system::shutdown(*fd).map(|()| Done::Nothing),
        Op::Close { fd } => {
            listing.forget(*fd);
            system::process::forget(*fd);
            system::close(*fd).map(|()| Done::Nothing)
        }
        Op::Read { fd, buf, at } => system::files::read(*fd, buf, *at).map(Done::Count),
        Op::Write { fd, bytes, from, at } => system::files::write(*fd, remaining(bytes, *from), *at).map(Done::Count),
        Op::Append { fd, bytes, from } => system::write(*fd, remaining(bytes, *from)).map(Done::Count),
        Op::Truncate { fd, length } => system::files::truncate(*fd, *length).map(|()| Done::Nothing),
        Op::Sync { fd } => system::files::sync(*fd).map(|()| Done::Nothing),
        Op::Stat { fd } => system::files::stat(*fd).map(Done::Stat),
        Op::Rename { from_dir, from, to_dir, to, no_replace } => {
            system::files::rename(*from_dir, from, *to_dir, to, *no_replace).map(|()| Done::Nothing)
        }
        Op::Remove { dir, name, directory } => system::files::remove(*dir, name, *directory).map(|()| Done::Nothing),
        Op::MakeDirectory { dir, name, mode } => system::files::mkdir(*dir, name, *mode).map(|()| Done::Nothing),
        Op::List { fd, entries, names } => system::retry(|| listing.list(*fd, entries, names)).map(Done::Count),
        Op::Lock { fd } => system::lock(*fd).map(|()| Done::Nothing),
        Op::PipeRead { fd, buf } => system::read(*fd, buf).map(Done::Count),
        Op::PipeWrite { fd, bytes, from } => {
            let left = remaining(bytes, *from);
            let left = if nonblocking {
                left
            } else {
                let maximum = libc::PIPE_BUF;
                left.get(..left.len().min(maximum)).expect("the capped write fits the remaining bytes")
            };
            system::write(*fd, left).map(Done::Count)
        }
        Op::Open { .. }
        | Op::Spawn { .. }
        | Op::Wait { .. }
        | Op::Signal { .. }
        | Op::Usage
        | Op::Window { .. }
        | Op::ReadSignal { .. } => unreachable!("shared decoding runs before raw calls"),
        Op::Cancel { .. } => unreachable!("cancel is settled against the table at submit"),
    }
}

fn remaining(bytes: &[u8], from: u32) -> &[u8] {
    bytes.get(usize::try_from(from).expect("u32 fits usize")..).expect("a valid write has bytes left")
}

#[cfg(test)]
mod tests {
    use super::Readiness;
    use crate::{Clock, Config, Wait, system};
    use skein_io::kernel::{Done, Error, Family, Fd, Op, Submit};
    use skein_lib::{Duration, Queue, Token};

    fn submit(kernel: &mut Readiness, token: u64, kind: Op) {
        let mut records = Queue::with_capacity(1);
        records.push(Submit { op: Token::new(token), kind });
        kernel.submit(&mut records, Wait::No);
        assert!(records.is_empty(), "the test kernel has room");
    }

    fn listener() -> Fd {
        let fd = system::socket(Family::Ipv4).expect("a test socket");
        system::bind(fd, &"127.0.0.1:0".parse().expect("loopback address")).expect("bind loopback");
        system::listen(fd, 1).expect("listen on loopback");
        fd
    }

    #[test]
    fn ready_records_hold_slots_until_reaped_and_preserve_order() {
        let mut kernel = Readiness::open(Config { operations: 2 }).expect("the poller is usable");
        let mut records = Queue::with_capacity(3);
        for token in 1..=3 {
            records.push(Submit { op: Token::new(token), kind: Op::Usage });
        }
        kernel.submit(&mut records, Wait::Forever);
        assert_eq!(records.len(), 1, "submit leaves the record past its capacity");
        assert_eq!((kernel.in_flight(), kernel.room()), (2, 0));
        let mut out = Queue::with_capacity(1);
        kernel.reap(&mut out);
        let complete = out.pop().expect("oldest ready completion");
        assert_eq!(complete.op, Token::new(1));
        assert!(matches!(complete.result, Ok(Done::Usage(_))));
        assert_eq!((kernel.in_flight(), kernel.room()), (1, 1));
        kernel.submit(&mut records, Wait::No);
        for token in 2..=3 {
            kernel.reap(&mut out);
            let complete = out.pop().expect("the next completion");
            assert_eq!(complete.op, Token::new(token));
            assert!(matches!(complete.result, Ok(Done::Usage(_))));
        }
        assert_eq!((kernel.in_flight(), kernel.room()), (0, 2));
    }

    #[test]
    fn an_inherited_blocking_pipe_waits_before_reading() {
        use std::io::Write;
        use std::os::fd::AsRawFd;
        let (reader, mut writer) = std::io::pipe().expect("a blocking test pipe");
        let fd = Fd::new(reader.as_raw_fd());
        let mut kernel = Readiness::open(Config { operations: 1 }).expect("the poller is usable");
        submit(&mut kernel, 1, Op::PipeRead { fd, buf: vec![0; 8].into_boxed_slice() });
        assert_eq!(kernel.waiters.get(&fd).expect("read interest registered").read, Some(0));
        assert!(kernel.ready.is_empty(), "an empty blocking pipe is not called at submit");
        assert!(!kernel.nonblocking.contains(&fd), "an inherited pipe keeps its flags");
        writer.write_all(b"pipe").expect("four bytes fit in an empty pipe");
        kernel.submit(&mut Queue::with_capacity(0), Wait::Forever);
        let mut out = Queue::with_capacity(1);
        kernel.reap(&mut out);
        let complete = out.pop().expect("one pipe read completes");
        assert_eq!(complete.result, Ok(Done::Count(4)));
        let Op::PipeRead { buf, .. } = complete.kind else { unreachable!("the read record comes back") };
        assert_eq!(buf.get(..4), Some(b"pipe".as_slice()));
        assert!(kernel.waiters.is_empty(), "a completed read removes its interest");
    }

    #[test]
    fn an_inherited_blocking_pipe_write_is_capped_at_pipe_buf() {
        use std::io::Read;
        use std::os::fd::AsRawFd;
        let (mut reader, writer) = std::io::pipe().expect("a blocking test pipe");
        let maximum = libc::PIPE_BUF;
        let mut kernel = Readiness::open(Config { operations: 1 }).expect("the poller is usable");
        submit(
            &mut kernel,
            1,
            Op::PipeWrite {
                fd: Fd::new(writer.as_raw_fd()),
                bytes: vec![7; maximum.checked_mul(2).expect("two PIPE_BUFs fit")].into_boxed_slice(),
                from: 0,
            },
        );
        let mut out = Queue::with_capacity(1);
        kernel.reap(&mut out);
        let complete = out.pop().expect("the empty pipe is writable");
        assert_eq!(complete.result, Ok(Done::Count(u32::try_from(maximum).expect("PIPE_BUF fits u32"))));
        let mut buf = vec![0; maximum];
        reader.read_exact(&mut buf).expect("the one write put PIPE_BUF bytes in the pipe");
        assert!(buf.iter().all(|byte| *byte == 7));
        assert!(kernel.waiters.is_empty());
    }

    #[test]
    #[expect(clippy::disallowed_types, reason = "the inherited descriptor in this test is a std File (shell.md, 9)")]
    fn a_regular_file_pipe_read_completes_without_interest() {
        use std::io::{Seek, SeekFrom, Write};
        use std::os::fd::AsRawFd;
        let scratch = skein_scratch::Scratch::new("readiness");
        let path = scratch.path().join("file");
        let mut file =
            std::fs::File::options().create_new(true).read(true).write(true).open(&path).expect("a scratch file");
        std::fs::remove_file(path).expect("the open scratch file needs no name");
        file.write_all(b"file").expect("four scratch bytes");
        file.seek(SeekFrom::Start(0)).expect("read from the beginning");
        let mut kernel = Readiness::open(Config { operations: 1 }).expect("the poller is usable");
        submit(&mut kernel, 1, Op::PipeRead { fd: Fd::new(file.as_raw_fd()), buf: vec![0; 8].into_boxed_slice() });
        assert!(kernel.waiters.is_empty(), "the completed file read keeps no interest");
        assert!(kernel.nonblocking.is_empty(), "a file is not a backend stream");
        let mut out = Queue::with_capacity(1);
        kernel.reap(&mut out);
        let complete = out.pop().expect("a file is always ready");
        assert_eq!(complete.result, Ok(Done::Count(4)));
        let Op::PipeRead { buf, .. } = complete.kind else { unreachable!("the read record comes back") };
        assert_eq!(buf.get(..4), Some(b"file".as_slice()));
    }

    #[test]
    fn a_signal_source_waits_then_reports_its_signal() {
        use skein_io::kernel::{ServiceSignal, Signalled};
        let fd = system::process::open_signals(crate::Watch::Termination).expect("open the termination signal source");
        let mut kernel = Readiness::open(Config { operations: 1 }).expect("the poller is usable");
        submit(&mut kernel, 1, Op::ReadSignal { fd });
        assert_eq!(kernel.waiters.get(&fd).expect("signal source read interest").read, Some(0));
        assert!(kernel.ready.is_empty(), "a signal source waits before its first call");
        crate::signal_current_thread(ServiceSignal::Interrupt).expect("queue SIGINT on this thread");
        kernel.submit(&mut Queue::with_capacity(0), Wait::Forever);
        let mut out = Queue::with_capacity(1);
        kernel.reap(&mut out);
        let sender = if cfg!(target_os = "linux") {
            Some(skein_io::kernel::Sender::new(u64::from(std::process::id())))
        } else {
            None
        };
        assert_eq!(
            out.pop().expect("one signal read").result,
            Ok(Done::Signalled(Signalled::Termination { signal: ServiceSignal::Interrupt, sender }))
        );
        assert!(kernel.waiters.is_empty());
        submit(&mut kernel, 2, Op::Close { fd });
        kernel.reap(&mut out);
        assert_eq!(out.pop().expect("the signal source closes").result, Ok(Done::Nothing));
    }

    // EPOLL_CTL_ADD refuses duplicate interest; kevent EV_ADD modifies it.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_registration_error_completes_without_calling_or_adding_a_waiter() {
        use std::os::fd::AsRawFd;
        let (reader, _writer) = std::io::pipe().expect("a blocking test pipe");
        let fd = Fd::new(reader.as_raw_fd());
        let mut kernel = Readiness::open(Config { operations: 1 }).expect("the poller is usable");
        kernel.poller.interest(fd, false, true, false).expect("pre-register the descriptor to provoke EEXIST");
        submit(&mut kernel, 1, Op::PipeRead { fd, buf: vec![0; 8].into_boxed_slice() });
        assert!(kernel.waiters.is_empty(), "failed registration preserves the waiter map");
        let mut out = Queue::with_capacity(1);
        kernel.reap(&mut out);
        let complete = out.pop().expect("the registration error completes the record");
        assert_eq!(complete.result, Err(Error::Other(libc::EEXIST)));
        let Op::PipeRead { buf, .. } = complete.kind else { unreachable!("the record comes back") };
        assert_eq!(&*buf, &[0; 8], "a registration error makes no read");
        kernel.poller.interest(fd, true, false, false).expect("remove the test's injected interest");
    }

    #[test]
    fn cancelled_slots_can_be_reused_without_cancelling_their_next_record() {
        let mut kernel = Readiness::open(Config { operations: 3 }).expect("the poller is usable");
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
        let mut kernel = Readiness::open(Config { operations: 2 }).expect("the poller is usable");
        let fd = listener();
        assert!(kernel.nonblocking.insert(fd));
        submit(&mut kernel, 1, Op::Accept { fd });
        submit(&mut kernel, 2, Op::Accept { fd });
    }

    #[test]
    #[should_panic(expected = "no Close beside a waiter")]
    fn a_close_beside_a_waiter_breaks_the_contract() {
        let mut kernel = Readiness::open(Config { operations: 2 }).expect("the poller is usable");
        let fd = listener();
        assert!(kernel.nonblocking.insert(fd));
        submit(&mut kernel, 1, Op::Accept { fd });
        submit(&mut kernel, 2, Op::Close { fd });
    }

    #[test]
    fn an_idle_deadline_does_not_wake_early() {
        let mut kernel = Readiness::open(Config { operations: 1 }).expect("the poller is usable");
        let clock = Clock::new();
        for _ in 0..4 {
            let deadline = clock.now().now.saturating_add(Duration::from_millis(2));
            kernel.submit(&mut Queue::with_capacity(0), Wait::Until(deadline));
            assert!(clock.now().now >= deadline, "the poller must not wake before the deadline");
        }
    }
}
