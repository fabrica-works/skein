//! The ring adapter (shell.md, 3): io's kernel records
//! (`skein_io::kernel`, whose module documentation is the contract kept
//! here) on io_uring. It maps each record onto one submission entry, keeps
//! the record in its in-flight table, and when the entry completes decodes
//! the result and hands the record back. It makes no decisions.
//!
//! # The in-flight table
//!
//! One slot per operation the [`Config`] allows, allocated once at its final
//! capacity in a `Box<[Slot]>` that is never resized or replaced while
//! anything is in flight, so a slot's address is fixed for the life of the
//! [`crate::Kernel`]. A slot holds the [`Submit`] record and the kernel structures
//! its operation needs (a socket address and its length, an `open_how`, a
//! `statx` buffer, a `signalfd_siginfo`, and a path or two as the NUL-terminated strings the
//! kernel reads); those structures never leave this module, and a record's
//! buffers are pointed at in place, inside the record's own `Box`.
//!
//! An operation's `user_data` is its slot's index and the slot's generation,
//! which moves on every time the slot is freed, so a completion or an async
//! cancel can never name the slot's next operation. A map from token to
//! `user_data` finds a cancel's target and catches a token already in flight.
//!
//! # The `unsafe`, and why it is sound
//!
//! - **Pushing an entry** hands the kernel pointers into the slot (an
//!   address and its length, an `open_how`, a `statx` buffer, a signal
//!   record), into the
//!   slot's paths, and into the record's buffers. All stay valid and
//!   unmoved until the entry's completion is reaped: the slot is not freed
//!   or moved until then, and no Rust code reads or writes it (only its
//!   token's entry in the map is read). The structures inline in the slot
//!   sit in `UnsafeCell`s: the kernel gets the cells' raw pointers, and
//!   Rust reaches their contents through `get_mut` only before the entry is
//!   pushed and after its completion has moved the operation out of its
//!   slot. The paths are boxes the slot owns, copies of the record's with a
//!   NUL after them, made before the entry is pushed and only read by the
//!   kernel. The record's `Box`es stay in the slot; a `Recv` or `Read`
//!   buffer is written by the kernel, a `Send` or `Write` buffer only read.
//!   Reaching a free slot borrows the table's slice for a moment, which
//!   touches no byte of a busy slot.
//! - **Entering the ring** passes the count of pushed entries and, for a
//!   deadline, an argument that lives across the call.
//! - **Dropping a [`crate::Kernel`]** with operations in flight leaks the table
//!   through `Box::into_raw`, creating no reference to it, so the kernel
//!   never writes into freed memory while the ring winds down.
//!
//! The completion queue holds as many entries as the table has slots, and
//! every slot has at most one entry in the kernel (single-shot operations
//! only, each cancel in a slot of its own), so it cannot overflow. An
//! `Open` that a `..` race failed with `EAGAIN` is pushed again from its
//! slot, under the same `user_data`, once its first entry has completed:
//! still one entry a slot.

#![expect(unsafe_code, reason = "the ring adapter keeps submitted memory live until completion (shell.md, 3)")]

use std::cell::UnsafeCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::CStr;
use std::fmt;
use std::io;
use std::marker::PhantomData;
use std::mem::{self, size_of};

use io_uring::{EnterFlags, IoUring, Probe, opcode, squeue, types};
use skein_io::kernel::{Complete, Done, Error, Family, Fd, Op, OpenHow, Submit};
use skein_lib::{Queue, Token};

use super::opens::{Fields, fields};
use super::process::{signal_child, spawn};
use super::sync::{
    STAT_MASK, c_path, child_exit, close, decode_addr, domain, encode, errno_of, list, local_addr, lock, opened,
    read_signal, set_option, socklen_of, stat, zeroed_statx, zeroed_storage,
};
use super::{process, terminal};
use crate::calls::{AGAIN, LISTING, check_path_access, error};
use crate::{Config, OpenError, Wait};

/// io's kernel, on io_uring: opened once, then submitted to and reaped from
/// once per iteration (programming-model.md, section 2).
pub(crate) struct Ring {
    #[expect(
        clippy::struct_field_names,
        reason = "the ring adapter keeps its io_uring field named for the ring it owns"
    )]
    ring: IoUring,
    table: Table,
    /// Neither `Send` nor `Sync`: `SINGLE_ISSUER` and `DEFER_TASKRUN` bind
    /// the ring to the thread that opened it.
    thread: PhantomData<*const ()>,
}

impl fmt::Debug for Ring {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ring")
            .field("operations", &self.table.slots.len())
            .field("in_flight", &self.table.tokens.len())
            .field("ready", &self.table.ready.len())
            .finish_non_exhaustive()
    }
}

/// The in-flight table and what finds things in it.
struct Table {
    /// Allocated once, at its final capacity; see the module documentation.
    slots: Box<[Slot]>,
    /// The free slots, the lowest on top.
    free: Vec<u32>,
    /// Slots whose operation the adapter completed itself, oldest first.
    ready: VecDeque<u32>,
    /// Every operation in flight: its token, and its `user_data`.
    tokens: BTreeMap<Token, u64>,
    /// The operations a `Cancel` was submitted for, so an `EINTR` reads as
    /// `Cancelled` on them only.
    cancelled: BTreeSet<Token>,
    /// What `getdents64` writes, for a `List`: the call is synchronous, so
    /// one buffer serves every one.
    listing: Box<[u8]>,
    /// How many times an `Open` was submitted again after `EAGAIN`.
    resubmitted: u64,
}

/// What a reaped entry comes to: a completion to hand up, or an `Open` to
/// submit again, its slot kept.
enum Reaped {
    Complete(Complete),
    Again(u32),
}

struct Slot {
    generation: u32,
    flight: Option<Flight>,
}

/// An operation in flight: the record, and what the kernel reads or writes
/// for it beside the record's own buffers.
struct Flight {
    op: Token,
    kind: Op,
    /// The address a `Bind` or `Connect` asks for, the peer an `Accept`
    /// writes, the address a `Bind` reads back with `getsockname`.
    /// In a cell: the kernel holds a pointer to it while the operation is in
    /// flight (see the module documentation).
    addr: UnsafeCell<libc::sockaddr_storage>,
    addr_len: UnsafeCell<libc::socklen_t>,
    /// An `Open`'s path, or the name a `Rename`, `Remove` or
    /// `MakeDirectory` acts on, with a NUL after it: what the kernel reads.
    path: Box<[u8]>,
    /// A `Rename`'s new name, with a NUL after it.
    to: Box<[u8]>,
    /// How an `Open` opens, which the kernel reads; in a cell, as the
    /// address is.
    open_how: UnsafeCell<types::OpenHow>,
    /// What a `Stat` reads back, which the kernel writes; in a cell, as the
    /// address is.
    statx: UnsafeCell<libc::statx>,
    /// `WaitId` writes this while its ring entry is in flight.
    siginfo: UnsafeCell<libc::siginfo_t>,
    /// One signalfd record read by the ring, decoded before crossing the kernel boundary.
    signal_info: UnsafeCell<libc::signalfd_siginfo>,
    /// How many times an `Open` was submitted again after `EAGAIN`.
    again: u32,
    /// The result, for an operation the adapter completed itself.
    ready: Option<Result<Done, Error>>,
}

/// What a record becomes: one submission entry, or a result the adapter
/// reached itself (a synchronous operation, or a cancel whose target is not
/// in flight).
enum Prepared {
    Entry(squeue::Entry),
    Done(Result<Done, Error>),
}

/// The operations the ring adapter submits, which the kernel's ring must
/// have: all of them at the floor, 6.12.
const OPERATIONS: [(u8, &str); 20] = [
    (opcode::Socket::CODE, "IORING_OP_SOCKET"),
    (opcode::Bind::CODE, "IORING_OP_BIND"),
    (opcode::Listen::CODE, "IORING_OP_LISTEN"),
    (opcode::Accept::CODE, "IORING_OP_ACCEPT"),
    (opcode::Connect::CODE, "IORING_OP_CONNECT"),
    (opcode::Recv::CODE, "IORING_OP_RECV"),
    (opcode::Send::CODE, "IORING_OP_SEND"),
    (opcode::Shutdown::CODE, "IORING_OP_SHUTDOWN"),
    (opcode::Close::CODE, "IORING_OP_CLOSE"),
    (opcode::OpenAt2::CODE, "IORING_OP_OPENAT2"),
    (opcode::Read::CODE, "IORING_OP_READ"),
    (opcode::Write::CODE, "IORING_OP_WRITE"),
    (opcode::Ftruncate::CODE, "IORING_OP_FTRUNCATE"),
    (opcode::Fsync::CODE, "IORING_OP_FSYNC"),
    (opcode::Statx::CODE, "IORING_OP_STATX"),
    (opcode::RenameAt::CODE, "IORING_OP_RENAMEAT"),
    (opcode::UnlinkAt::CODE, "IORING_OP_UNLINKAT"),
    (opcode::MkDirAt::CODE, "IORING_OP_MKDIRAT"),
    (opcode::AsyncCancel::CODE, "IORING_OP_ASYNC_CANCEL"),
    (opcode::WaitId::CODE, "IORING_OP_WAITID"),
];

const NANOS_PER_SEC: u64 = 1_000_000_000;

/// The path a `Stat` names with `AT_EMPTY_PATH`: the descriptor itself.
const EMPTY: &CStr = c"";

fn open(how: OpenHow) -> types::OpenHow {
    let Fields { flags, mode, resolve } = fields(how);
    types::OpenHow::new().flags(flags).mode(mode).resolve(resolve)
}

impl Ring {
    /// Sets up the ring for this thread alone: one issuer, and completions
    /// processed only when the loop enters the ring (shell.md, 3).
    /// Probes the ring for every operation it uses, and refuses to open below
    /// the floor.
    pub fn open(config: Config) -> Result<Ring, OpenError> {
        let operations = config.operations;
        if operations == 0 {
            return Err(OpenError::NoOperations);
        }
        let built = IoUring::builder()
            .setup_single_issuer()
            .setup_defer_taskrun()
            .setup_submit_all()
            .setup_cqsize(operations)
            .build(operations);
        let ring: IoUring = match built {
            Ok(ring) => ring,
            Err(error) => return Err(OpenError::Setup(errno_of(&error))),
        };
        let mut probe = Probe::new();
        if let Err(error) = ring.submitter().register_probe(&mut probe) {
            return Err(OpenError::Setup(errno_of(&error)));
        }
        for (code, name) in OPERATIONS {
            if !probe.is_supported(code) {
                return Err(OpenError::Missing(name));
            }
        }
        if !ring.params().is_feature_min_timeout() {
            return Err(OpenError::Missing("absolute wait timeouts"));
        }
        assert!(
            ring.params().sq_entries() >= operations && ring.params().cq_entries() >= operations,
            "the kernel sizes each ring at least as asked"
        );
        if let Err(errno) = process::block_sigpipe() {
            return Err(OpenError::Setup(errno));
        }
        Ok(Ring { ring, table: Table::new(operations), thread: PhantomData })
    }

    /// How many operations are in flight, those completed but not yet reaped
    /// included.
    #[must_use]
    pub fn in_flight(&self) -> u32 {
        u32::try_from(self.table.tokens.len()).expect("no more in flight than the config's u32")
    }

    /// How many more records [`submit`](crate::Kernel::submit) would take now.
    #[must_use]
    pub fn room(&self) -> u32 {
        u32::try_from(self.table.free.len()).expect("no more slots than the config's u32")
    }

    /// Takes records from the front of `records` while a slot is free, maps
    /// each onto one submission entry (or completes it itself, at the next
    /// reap, when it is not a ring operation), submits them, and waits as
    /// `wait` says. A record that finds no free slot stays in `records`.
    ///
    /// Panics on the contract's broken invariants that the ring checks: a
    /// record that is not [`Op::is_valid`], and a token already in flight;
    /// and on [`Wait::Forever`] with nothing in flight or ready, which would
    /// never return.
    pub fn submit(&mut self, records: &mut Queue<Submit>, wait: Wait) {
        while !self.table.free.is_empty() {
            let Some(record) = records.pop() else {
                break;
            };
            self.start(record);
        }
        self.enter(wait);
    }

    /// Moves completions into `out` while it has room, each with its record
    /// handed back: first those the adapter reached itself, then the ring's.
    /// What does not fit waits for the next reap.
    pub fn reap(&mut self, out: &mut Queue<Complete>) {
        while out.room() > 0 {
            let Some(index) = self.table.ready.pop_front() else {
                break;
            };
            let (flight, _cancelled) = self.table.retire(index);
            let result = flight.ready.expect("a ready slot holds its result");
            out.push(Complete { op: flight.op, kind: flight.kind, result });
        }
        let mut again = Vec::new();
        let mut completions = self.ring.completion();
        while out.room() > 0 {
            let Some(entry) = completions.next() else {
                break;
            };
            match self.table.complete(entry.user_data(), entry.result()) {
                Reaped::Complete(complete) => out.push(complete),
                Reaped::Again(index) => again.push(index),
            }
        }
        drop(completions);
        for index in again {
            self.resubmit(index);
        }
    }

    /// How many times an `Open` was submitted again after `EAGAIN`, its `..`
    /// having raced a rename or a mount: for tests, which provoke it.
    #[must_use]
    pub const fn resubmitted(&self) -> u64 {
        self.table.resubmitted
    }

    /// Pushes an `Open` that failed with `EAGAIN` again, in its slot under the
    /// same `user_data`; the next enter submits it.
    fn resubmit(&mut self, index: u32) {
        let slot = self.table.slots.get(slot_index(index)).expect("an index in flight names a slot");
        let flight = slot.flight.as_ref().expect("an operation submitted again is in its slot");
        let Op::Open { root, .. } = flight.kind else {
            unreachable!("only an Open is submitted again");
        };
        let entry = open_entry(root, &flight.path, &flight.open_how).user_data(user_data(index, slot.generation));
        // SAFETY: as at the first push (`start`): the path and the
        // `open_how` the entry points at are this slot's, which stays in
        // place, untouched by Rust code, until the new completion is reaped;
        // the kernel had done with them, having posted the first.
        let pushed = unsafe { self.ring.submission().push(&entry) };
        pushed.expect("the submission queue has an entry for every slot");
        self.table.resubmitted = self.table.resubmitted.saturating_add(1);
    }

    fn start(&mut self, record: Submit) {
        let Submit { op, kind } = record;
        assert!(kind.is_valid(), "io submits only valid records (skein_io::kernel, broken invariants)");
        check_path_access(&kind);
        assert!(!self.table.tokens.contains_key(&op), "io never reuses a token in flight (skein_io::kernel)");
        let index = self.table.free.pop().expect("a record is taken only while a slot is free");
        let Table { slots, tokens, cancelled, ready, listing, .. } = &mut self.table;
        let slot = slots.get_mut(slot_index(index)).expect("a free index names a slot");
        assert!(slot.flight.is_none(), "a free slot holds no operation");
        let user_data = user_data(index, slot.generation);
        let flight = slot.flight.insert(Flight::new(op, kind));
        match prepare(flight, tokens, cancelled, listing) {
            Prepared::Entry(entry) => {
                let entry = entry.user_data(user_data);
                // SAFETY: every pointer in the entry points into this slot's
                // flight (its address and length, its `open_how`, its
                // `statx`), into the paths it owns, or into its record's
                // `Box` buffers. The slot's table is never resized, and the
                // flight stays in place, untouched by Rust code, until
                // `complete` takes it after the kernel posts this entry's
                // completion.
                let pushed = unsafe { self.ring.submission().push(&entry) };
                pushed.expect("the submission queue has an entry for every slot");
            }
            Prepared::Done(result) => {
                flight.ready = Some(result);
                ready.push_back(index);
            }
        }
        let previous = tokens.insert(op, user_data);
        assert!(previous.is_none(), "the token was checked not in flight");
    }

    /// Submits what was pushed, and runs the completions the kernel deferred
    /// until now (`GETEVENTS`, always), waiting as `wait` says. A completion
    /// the adapter reached itself is already waiting, so it never blocks.
    fn enter(&mut self, wait: Wait) {
        let pending = u32::try_from(self.ring.submission().len()).expect("no more entries than the config's u32");
        let blocks = self.table.ready.is_empty();
        let flags = types::SubmitArgs::new();
        let entered = match wait {
            Wait::Until(deadline) if blocks => {
                let nanos = deadline.as_nanos();
                let subsec = u32::try_from(nanos.rem_euclid(NANOS_PER_SEC)).expect("below a second");
                let timespec = types::Timespec::new().sec(nanos.div_euclid(NANOS_PER_SEC)).nsec(subsec);
                let args = flags.timespec(&timespec);
                let bits = EnterFlags::GETEVENTS | EnterFlags::EXT_ARG | EnterFlags::ABS_TIMER;
                // SAFETY: `args` is the `io_uring_getevents_arg` that `EXT_ARG`
                // announces (`SubmitArgs` is its transparent wrapper), and it
                // and the timespec it points at live across the call;
                // `pending` entries were pushed and published.
                unsafe { self.ring.submitter().enter(pending, 1, bits.bits(), Some(&args)) }
            }
            Wait::No | Wait::Until(_) => self.enter_plain(pending, 0),
            Wait::Forever => {
                assert!(
                    !blocks || !self.table.tokens.is_empty(),
                    "waiting forever with nothing in flight never returns"
                );
                self.enter_plain(pending, u32::from(blocks))
            }
        };
        let Err(error) = entered else {
            return;
        };
        match errno_of(&error) {
            // The deadline passed, or a signal came: back to the loop, which
            // decides what is due. Out of kernel memory for a submission:
            // the entries stay queued, and the next enter submits them.
            libc::ETIME | libc::EINTR | libc::EAGAIN | libc::ENOMEM => {}
            other => panic!("the ring refused an enter: {}", io::Error::from_raw_os_error(other)),
        }
    }

    fn enter_plain(&self, pending: u32, min_complete: u32) -> io::Result<usize> {
        let bits = EnterFlags::GETEVENTS.bits();
        // SAFETY: no extended argument (a null signal mask), and `pending`
        // entries were pushed and published.
        unsafe { self.ring.submitter().enter::<libc::sigset_t>(pending, min_complete, bits, None) }
    }
}

/// Dropping a kernel with operations in flight is a shutdown path: the ring
/// is closed, the kernel cancels what it still holds in its own time, and
/// the table, with every record and buffer in it, is leaked so that nothing
/// the kernel may still write is freed. Descriptors are not closed: only a
/// `Close` record closes one.
impl Drop for Ring {
    fn drop(&mut self) {
        if self.table.tokens.is_empty() {
            return;
        }
        let slots = mem::take(&mut self.table.slots);
        let _leaked: *mut [Slot] = Box::into_raw(slots);
    }
}

impl Table {
    fn new(operations: u32) -> Table {
        let capacity = slot_index(operations);
        let mut slots = Vec::with_capacity(capacity);
        let mut free = Vec::with_capacity(capacity);
        for index in (0..operations).rev() {
            slots.push(Slot { generation: 0, flight: None });
            free.push(index);
        }
        Table {
            slots: slots.into_boxed_slice(),
            free,
            ready: VecDeque::with_capacity(capacity),
            tokens: BTreeMap::new(),
            cancelled: BTreeSet::new(),
            listing: vec![0; LISTING].into_boxed_slice(),
            resubmitted: 0,
        }
    }

    /// Takes the operation out of its slot, and frees the slot for the next
    /// one under a new generation.
    /// Also says whether a `Cancel` was submitted for it.
    fn retire(&mut self, index: u32) -> (Flight, bool) {
        let slot = self.slots.get_mut(slot_index(index)).expect("an index in flight names a slot");
        let flight = slot.flight.take().expect("a slot in flight holds its operation");
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(index);
        let removed = self.tokens.remove(&flight.op);
        assert!(removed.is_some(), "an operation in flight has its token");
        let cancelled = self.cancelled.remove(&flight.op);
        (flight, cancelled)
    }

    /// The completion of the ring entry named `user_data`, its result `res`
    /// decoded.
    fn complete(&mut self, user_data: u64, res: i32) -> Reaped {
        let (index, generation) = split(user_data);
        let slot = self.slots.get_mut(slot_index(index)).expect("a completion names a slot");
        assert!(slot.generation == generation, "every completion names an operation in flight, once");
        let flight = slot.flight.as_mut().expect("every completion names an operation in flight, once");
        // RESOLVE_BENEATH answers EAGAIN when a `..` raced a rename or a
        // mount anywhere on the system: the same Open is likely to pass.
        if let Op::Open { .. } = flight.kind
            && res == -libc::EAGAIN
            && flight.again < AGAIN
            && !self.cancelled.contains(&flight.op)
        {
            flight.again = flight.again.saturating_add(1);
            return Reaped::Again(index);
        }
        let (mut flight, cancelled) = self.retire(index);
        let result = decode(&mut flight, res, cancelled);
        Reaped::Complete(Complete { op: flight.op, kind: flight.kind, result })
    }
}

impl Flight {
    fn new(op: Token, kind: Op) -> Flight {
        Flight {
            op,
            kind,
            addr: UnsafeCell::new(zeroed_storage()),
            addr_len: UnsafeCell::new(0),
            path: Box::default(),
            to: Box::default(),
            open_how: UnsafeCell::new(types::OpenHow::new()),
            statx: UnsafeCell::new(zeroed_statx()),
            // SAFETY: siginfo_t is an integer-bearing C output structure;
            // waitid fills it before decode reads its fields.
            siginfo: UnsafeCell::new(unsafe { mem::zeroed() }),
            // SAFETY: signalfd_siginfo is a plain output structure, filled by Read.
            signal_info: UnsafeCell::new(unsafe { mem::zeroed() }),
            again: 0,
            ready: None,
        }
    }
}

/// Maps a record onto its one submission entry, with the backend defaults
/// that are not records (`skein_io::kernel`): close-on-exec on every new
/// descriptor, `SO_REUSEADDR` on a socket that binds (set here, before the
/// `Bind`), `MSG_NOSIGNAL` on every send, the modes of new files and
/// directories. A `Cancel` whose target is not in flight is too late,
/// without asking the kernel; a `List` runs here, synchronously.
#[expect(clippy::too_many_lines, reason = "one exhaustive operation table maps each record to its ring entry")]
fn prepare(
    flight: &mut Flight,
    tokens: &BTreeMap<Token, u64>,
    cancelled: &mut BTreeSet<Token>,
    listing: &mut [u8],
) -> Prepared {
    let Flight { kind, addr, addr_len, path, to: to_path, open_how, statx, siginfo, signal_info, .. } = flight;
    let entry = match kind {
        Op::Socket { family } => {
            let kind = libc::SOCK_STREAM | libc::SOCK_CLOEXEC;
            opcode::Socket::new(domain(*family), kind, libc::IPPROTO_TCP).build()
        }
        Op::Bind { fd, addr: to } => {
            let (fd, to) = (*fd, *to);
            if let Err(errno) = set_option(fd.raw(), libc::SOL_SOCKET, libc::SO_REUSEADDR) {
                return Prepared::Done(Err(error(kind, errno, false)));
            }
            let len = encode(&to, addr.get_mut());
            *addr_len.get_mut() = len;
            opcode::Bind::new(types::Fd(fd.raw()), addr.get().cast_const().cast(), len).build()
        }
        Op::Listen { fd, backlog } => {
            // The backlog is a hint, clamped by the kernel to somaxconn.
            let backlog = i32::try_from(*backlog).unwrap_or(i32::MAX);
            opcode::Listen::new(types::Fd(fd.raw()), backlog).build()
        }
        Op::Accept { fd } => {
            *addr_len.get_mut() = socklen_of::<libc::sockaddr_storage>();
            opcode::Accept::new(types::Fd(fd.raw()), addr.get().cast(), addr_len.get())
                .flags(libc::SOCK_CLOEXEC)
                .build()
        }
        Op::Connect { fd, addr: to } => {
            let len = encode(to, addr.get_mut());
            *addr_len.get_mut() = len;
            opcode::Connect::new(types::Fd(fd.raw()), addr.get().cast_const().cast(), len).build()
        }
        Op::Recv { fd, buf } => {
            let len = u32::try_from(buf.len()).expect("a valid Recv buffer's length is a count");
            opcode::Recv::new(types::Fd(fd.raw()), buf.as_mut_ptr(), len).build()
        }
        Op::PipeRead { fd, buf } => {
            let len = u32::try_from(buf.len()).expect("a valid pipe read buffer's length is a count");
            opcode::Read::new(types::Fd(fd.raw()), buf.as_mut_ptr(), len).offset(u64::MAX).build()
        }
        Op::Send { fd, bytes, from } => {
            let from = usize::try_from(*from).expect("a u32 fits in a usize");
            let left = bytes.get(from..).expect("a valid Send has bytes left from its offset");
            let len = u32::try_from(left.len()).expect("a valid Send's length is a count");
            opcode::Send::new(types::Fd(fd.raw()), left.as_ptr(), len).flags(libc::MSG_NOSIGNAL).build()
        }
        Op::PipeWrite { fd, bytes, from } => {
            let start = usize::try_from(*from).expect("a u32 fits in a usize");
            let left = bytes.get(start..).expect("a valid pipe write has bytes left");
            let len = u32::try_from(left.len()).expect("a valid pipe write's length is a count");
            opcode::Write::new(types::Fd(fd.raw()), left.as_ptr(), len).offset(u64::MAX).build()
        }
        Op::Shutdown { fd } => opcode::Shutdown::new(types::Fd(fd.raw()), libc::SHUT_WR).build(),
        Op::Close { fd } => opcode::Close::new(types::Fd(fd.raw())).build(),
        Op::Open { root, path: asked, how } => {
            *path = c_path(asked);
            *open_how.get_mut() = open(*how);
            open_entry(*root, path, open_how)
        }
        Op::Read { fd, buf, at } => {
            let len = u32::try_from(buf.len()).expect("a valid Read buffer's length is a count");
            opcode::Read::new(types::Fd(fd.raw()), buf.as_mut_ptr(), len).offset(*at).build()
        }
        Op::Write { fd, bytes, from, at } => {
            let from = usize::try_from(*from).expect("a u32 fits in a usize");
            let left = bytes.get(from..).expect("a valid Write has bytes left from its offset");
            let len = u32::try_from(left.len()).expect("a valid Write's length is a count");
            opcode::Write::new(types::Fd(fd.raw()), left.as_ptr(), len).offset(*at).build()
        }
        Op::Append { fd, bytes, from } => {
            let from = usize::try_from(*from).expect("a u32 fits in a usize");
            let left = bytes.get(from..).expect("a valid Append has bytes left");
            let len = u32::try_from(left.len()).expect("a valid Append counts bytes");
            opcode::Write::new(types::Fd(fd.raw()), left.as_ptr(), len).offset(u64::MAX).build()
        }
        Op::Truncate { fd, length } => opcode::Ftruncate::new(types::Fd(fd.raw()), *length).build(),
        Op::Sync { fd } => opcode::Fsync::new(types::Fd(fd.raw())).build(),
        Op::Stat { fd } => opcode::Statx::new(types::Fd(fd.raw()), EMPTY.as_ptr(), statx.get().cast())
            .flags(libc::AT_EMPTY_PATH)
            .mask(STAT_MASK)
            .build(),
        Op::Rename { from_dir, from, to_dir, to, no_replace } => {
            *path = c_path(from);
            *to_path = c_path(to);
            let (from_dir, to_dir) = (types::Fd(from_dir.raw()), types::Fd(to_dir.raw()));
            let flags = if *no_replace { libc::RENAME_NOREPLACE } else { 0 };
            opcode::RenameAt::new(from_dir, path.as_ptr().cast(), to_dir, to_path.as_ptr().cast()).flags(flags).build()
        }
        Op::Remove { dir, name, directory } => {
            *path = c_path(name);
            let flags = if *directory { libc::AT_REMOVEDIR } else { 0 };
            opcode::UnlinkAt::new(types::Fd(dir.raw()), path.as_ptr().cast()).flags(flags).build()
        }
        Op::MakeDirectory { dir, name, mode } => {
            *path = c_path(name);
            opcode::MkDirAt::new(types::Fd(dir.raw()), path.as_ptr().cast()).mode(*mode).build()
        }
        Op::List { fd, entries, names } => {
            let listed = list(fd.raw(), entries, names, listing);
            return Prepared::Done(match listed {
                Ok(n) => Ok(Done::Count(n)),
                Err(errno) => Err(error(kind, errno, false)),
            });
        }
        Op::Lock { fd } => {
            let locked = lock(*fd);
            return Prepared::Done(locked.map(|()| Done::Nothing).map_err(|errno| error(kind, errno, false)));
        }
        Op::Spawn { spawn: command } => return Prepared::Done(spawn(command)),
        Op::Signal { pidfd, signal, to } => return Prepared::Done(signal_child(*pidfd, *signal, *to)),
        Op::Window { fd } => return Prepared::Done(terminal::size(*fd).map(Done::Window)),
        Op::Usage => return Prepared::Done(process::usage().map(Done::Usage)),
        Op::ReadSignal { fd } => opcode::Read::new(
            types::Fd(fd.raw()),
            signal_info.get().cast(),
            u32::try_from(size_of::<libc::signalfd_siginfo>()).expect("signal record fits u32"),
        )
        .offset(u64::MAX)
        .build(),
        Op::Wait { pidfd, reap } => opcode::WaitId::new(
            libc::P_PIDFD,
            u32::try_from(pidfd.raw()).expect("a pidfd is positive"),
            libc::WEXITED | if *reap { 0 } else { libc::WNOWAIT },
        )
        .infop(siginfo.get().cast_const())
        .build(),
        Op::Cancel { target } => {
            let Some(target_data) = tokens.get(target) else {
                return Prepared::Done(Err(Error::TooLate));
            };
            let _first: bool = cancelled.insert(*target);
            opcode::AsyncCancel::new(*target_data).build()
        }
    };
    Prepared::Entry(entry)
}

/// The result of an operation the kernel completed with `res`, with the
/// backend defaults applied to a new socket: `TCP_NODELAY` on every TCP
/// socket (so on every connected one) and on every accepted one, and
/// `IPV6_V6ONLY` on every IPv6 socket, before io can bind it. A `Bind` reads
/// the address bound back with `getsockname`.
fn decode(flight: &mut Flight, res: i32, cancelled: bool) -> Result<Done, Error> {
    let Flight { kind, addr, addr_len, statx, siginfo, signal_info, .. } = flight;
    let (addr, addr_len) = (addr.get_mut(), addr_len.get_mut());
    if res < 0 {
        return Err(error(kind, 0_i32.saturating_sub(res), cancelled));
    }
    match kind {
        Op::Socket { family } => {
            let mut configured = set_option(res, libc::IPPROTO_TCP, libc::TCP_NODELAY);
            if configured.is_ok() && *family == Family::Ipv6 {
                configured = set_option(res, libc::IPPROTO_IPV6, libc::IPV6_V6ONLY);
            }
            match configured {
                Ok(()) => Ok(Done::Fd(Fd::new(res))),
                // Not the operation's own error, so not mapped as one: an
                // EINVAL here is no bug of io's record.
                Err(errno) => {
                    close(res);
                    Err(Error::Other(errno))
                }
            }
        }
        Op::Accept { .. } => {
            // Both are kernel answers that cannot happen on a TCP socket the
            // kernel just accepted; failing here would take a connection
            // the contract says a failed Accept leaves waiting.
            let configured = set_option(res, libc::IPPROTO_TCP, libc::TCP_NODELAY);
            assert!(configured.is_ok(), "TCP_NODELAY applies to every accepted TCP socket: {configured:?}");
            let peer = decode_addr(addr, *addr_len).expect("an accepted TCP socket's peer is IPv4 or IPv6");
            Ok(Done::Accepted { fd: Fd::new(res), peer })
        }
        Op::Bind { fd, .. } => {
            let fd = *fd;
            match local_addr(fd.raw(), addr, addr_len) {
                Ok(()) => match decode_addr(addr, *addr_len) {
                    Some(bound) => Ok(Done::Bound(bound)),
                    None => Err(Error::Other(libc::EAFNOSUPPORT)),
                },
                Err(errno) => Err(error(kind, errno, false)),
            }
        }
        Op::Recv { .. }
        | Op::Send { .. }
        | Op::Read { .. }
        | Op::Write { .. }
        | Op::Append { .. }
        | Op::PipeRead { .. }
        | Op::PipeWrite { .. } => Ok(Done::Count(u32::try_from(res).expect("a non-negative i32 fits in a u32"))),
        Op::Open { how: OpenHow::PathNoFollow, .. } => Ok(Done::Fd(Fd::new(res))),
        Op::Open { .. } => opened(res),
        Op::Stat { .. } => Ok(Done::Stat(stat(statx.get_mut()))),
        Op::Wait { .. } => child_exit(siginfo.get_mut()).map(Done::Exit),
        Op::ReadSignal { .. } => read_signal(signal_info.get_mut(), usize::try_from(res).expect("non-negative result")),
        Op::Listen { .. }
        | Op::Connect { .. }
        | Op::Shutdown { .. }
        | Op::Close { .. }
        | Op::Truncate { .. }
        | Op::Sync { .. }
        | Op::Rename { .. }
        | Op::Remove { .. }
        | Op::MakeDirectory { .. }
        | Op::Cancel { .. } => Ok(Done::Nothing),
        Op::Lock { .. } | Op::Window { .. } | Op::Usage | Op::List { .. } | Op::Spawn { .. } | Op::Signal { .. } => {
            unreachable!("a synchronous operation completes at its submit")
        }
    }
}

/// An `Open`'s entry, of the path and the `open_how` its slot holds.
fn open_entry(root: Fd, path: &[u8], open_how: &UnsafeCell<types::OpenHow>) -> squeue::Entry {
    opcode::OpenAt2::new(types::Fd(root.raw()), path.as_ptr().cast(), open_how.get().cast_const()).build()
}

/// An operation's `user_data`: its slot's generation, then its slot's index.
fn user_data(index: u32, generation: u32) -> u64 {
    (u64::from(generation) << 32_u32) | u64::from(index)
}

fn split(user_data: u64) -> (u32, u32) {
    let index = u32::try_from(user_data & u64::from(u32::MAX)).expect("masked to 32 bits");
    let generation = u32::try_from(user_data >> 32_u32).expect("shifted to 32 bits");
    (index, generation)
}

fn slot_index(index: u32) -> usize {
    usize::try_from(index).expect("a u32 fits in a usize")
}

// The synchronous calls: what is not a ring operation, done when the record
// is submitted or completed (kernel.md), and the clock and the
// seed (shell.md, 5).

#[cfg(test)]
mod tests {
    use skein_io::kernel::{Done, Op, ServiceSignal, Submit};
    use skein_lib::{Duration, Queue, Token};

    use super::{Config, Ring, Wait};
    use crate::Clock;
    use crate::open_termination_signals;

    #[test]
    fn blocked_termination_signal_arrives_through_the_ring() {
        let fd = open_termination_signals().expect("block the signals and open signalfd");
        let mut kernel = Ring::open(Config { operations: 2 }).expect("open the ring");
        let mut submits = Queue::with_capacity(1);
        submits.push(Submit { op: Token::new(1), kind: Op::ReadSignal { fd } });
        kernel.submit(&mut submits, Wait::No);

        // SAFETY: pthread_self returns this live test thread's identifier.
        let thread = unsafe { libc::pthread_self() };
        // SAFETY: SIGTERM is blocked on this thread, so pthread_kill queues
        // it for the signalfd instead of invoking its default action.
        assert_eq!(unsafe { libc::pthread_kill(thread, libc::SIGTERM) }, 0);
        let deadline = Clock::new().now().now.saturating_add(Duration::from_secs(1));
        let mut answers = Queue::with_capacity(1);
        for _ in 0..3 {
            kernel.submit(&mut submits, Wait::Until(deadline));
            kernel.reap(&mut answers);
            if !answers.is_empty() {
                break;
            }
        }
        let answer = answers.pop().expect("the signal read completed");
        assert_eq!(answer.kind, Op::ReadSignal { fd });
        assert_eq!(
            answer.result,
            Ok(Done::Signalled(skein_io::kernel::Signalled::Termination {
                signal: ServiceSignal::Terminate,
                sender: Some(skein_io::kernel::Sender::new(u64::from(std::process::id())))
            }))
        );

        submits.push(Submit { op: Token::new(2), kind: Op::Close { fd } });
        kernel.submit(&mut submits, Wait::No);
        let deadline = Clock::new().now().now.saturating_add(Duration::from_secs(1));
        for _ in 0..3 {
            kernel.submit(&mut submits, Wait::Until(deadline));
            kernel.reap(&mut answers);
            if !answers.is_empty() {
                break;
            }
        }
        let answer = answers.pop().expect("the signal descriptor closed");
        assert_eq!(answer.kind, Op::Close { fd });
        assert_eq!(answer.result, Ok(Done::Nothing));
    }
}
