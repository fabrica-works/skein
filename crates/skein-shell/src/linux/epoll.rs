//! Level-triggered interests and a single reusable event buffer (shell.md, 7.2).
//!
//! # The `unsafe`, and why it is sound
//!
//! Epoll creation transfers one fresh descriptor to `OwnedFd`, which closes
//! it on drop. Ctl reads one initialized local event during the call only;
//! DEL takes a null pointer as Linux specifies. Pwait2 writes at most the
//! buffer's stated number of initialized events, and reads a local timespec
//! when present; a null signal mask keeps this thread's mask. No call retains
//! a pointer into Rust memory. Events carry descriptor numbers, never pointers.

#![expect(
    clippy::disallowed_types,
    reason = "the internal epoll descriptor belongs to the backend, never to an io record"
)]

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::ptr;

use skein_io::kernel::Fd;

use super::last_errno;

#[derive(Debug)]
pub(crate) struct Epoll {
    fd: OwnedFd,
    events: Box<[libc::epoll_event]>,
}

#[derive(Clone, Copy)]
pub(crate) struct Event {
    pub fd: Fd,
    pub read: bool,
    pub write: bool,
}

impl Epoll {
    pub(crate) fn open(operations: u32) -> Result<Self, i32> {
        // SAFETY: integer flags only; success returns a fresh owned descriptor.
        let fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if fd < 0 {
            return Err(last_errno());
        }
        // SAFETY: fd is fresh, exclusively owned, and transferred exactly once.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let capacity = usize::try_from(operations).expect("a u32 fits usize");
        Ok(Self { fd, events: vec![libc::epoll_event { events: 0, u64: 0 }; capacity].into_boxed_slice() })
    }

    pub(crate) fn interest(&self, fd: Fd, existed: bool, read: bool, write: bool) -> Result<(), i32> {
        let bits = if read { libc::EPOLLIN } else { 0 } | if write { libc::EPOLLOUT } else { 0 };
        let op = if bits == 0 {
            libc::EPOLL_CTL_DEL
        } else if existed {
            libc::EPOLL_CTL_MOD
        } else {
            libc::EPOLL_CTL_ADD
        };
        let mut event = libc::epoll_event {
            events: u32::try_from(bits).expect("epoll flags are positive"),
            u64: u64::try_from(fd.raw()).expect("a live descriptor is nonnegative"),
        };
        super::retry(|| {
            let event = if bits == 0 { ptr::null_mut() } else { ptr::from_mut(&mut event) };
            // SAFETY: event lives across ctl; DEL ignores its null event pointer.
            let result = unsafe { libc::epoll_ctl(self.fd.as_raw_fd(), op, fd.raw(), event) };
            if result < 0 { Err(last_errno()) } else { Ok(()) }
        })
    }

    pub(crate) fn wait(&mut self, nanos: Option<u64>) -> Result<usize, i32> {
        let timeout = nanos.map(|nanos| libc::timespec {
            tv_sec: libc::time_t::try_from(nanos / 1_000_000_000).expect("a u64 nanosecond time fits time_t seconds"),
            tv_nsec: libc::c_long::try_from(nanos % 1_000_000_000).expect("subsecond fits c_long"),
        });
        let timeout = timeout.as_ref().map_or(ptr::null(), ptr::from_ref);
        let capacity = i32::try_from(self.events.len()).expect("epoll event capacity fits i32");
        // SAFETY: events holds capacity writable events; timeout, if present,
        // lives across the syscall. The null mask leaves the thread's mask alone.
        let result = unsafe {
            libc::syscall(
                libc::SYS_epoll_pwait2,
                self.fd.as_raw_fd(),
                self.events.as_mut_ptr(),
                capacity,
                timeout,
                ptr::null::<libc::sigset_t>(),
                0_usize,
            )
        };
        if result < 0 { Err(last_errno()) } else { Ok(usize::try_from(result).expect("nonnegative event count")) }
    }

    pub(crate) fn event(&self, index: usize) -> Event {
        let event = self.events.get(index).expect("an event returned by epoll");
        let flags = i32::try_from(event.events).expect("epoll flags fit i32");
        let both = flags & (libc::EPOLLERR | libc::EPOLLHUP) != 0;
        Event {
            fd: Fd::new(i32::try_from(event.u64).expect("event data is a descriptor")),
            read: both || flags & libc::EPOLLIN != 0,
            write: both || flags & libc::EPOLLOUT != 0,
        }
    }
}
