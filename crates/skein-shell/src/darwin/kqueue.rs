//! Level-triggered descriptor readiness (shell.md, 7.2 and 7.5).
//!
//! # The `unsafe`, and why it is sound
//!
//! Kqueue returns a fresh descriptor, owned here and close-on-exec before
//! use. Kevent borrows initialized change records and exclusive output
//! records only for the call; udata is null. `EV_RECEIPT` reports each change
//! without consuming pending events. Wait writes at most the allocated
//! event capacity and borrows its relative timespec until return.

#![expect(clippy::disallowed_types, reason = "the poller's descriptor is internal, never adopted by io")]
use super::{last_errno, retry};
use skein_io::kernel::Fd;
use std::{
    collections::BTreeMap,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    ptr,
};

pub(super) fn open() -> Result<OwnedFd, i32> {
    // SAFETY: kqueue takes no arguments and returns a fresh descriptor.
    let raw = unsafe { libc::kqueue() };
    if raw < 0 {
        return Err(last_errno());
    }
    // SAFETY: raw is newly created and uniquely transferred here.
    let owned = unsafe { OwnedFd::from_raw_fd(raw) };
    super::cloexec(Fd::new(raw))?;
    Ok(owned)
}
pub(super) fn event(ident: usize, filter: i16, flags: u16, notes: u32) -> libc::kevent {
    libc::kevent { ident, filter, flags, fflags: notes, data: 0, udata: ptr::null_mut() }
}
pub(super) fn change(fd: i32, mut change: libc::kevent) -> Result<(), i32> {
    change.flags |= libc::EV_RECEIPT;
    retry(|| {
        let mut receipt = event(0, 0, 0, 0);
        let timeout = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: change and receipt are separate initialized records, alive
        // for this call; the zero timeout never waits or drains other events.
        let count = unsafe {
            libc::kevent(fd, ptr::from_ref(&change), 1, ptr::from_mut(&mut receipt), 1, ptr::from_ref(&timeout))
        };
        if count < 0 {
            return Err(last_errno());
        }
        if count != 1 || receipt.flags & libc::EV_ERROR == 0 {
            return Err(libc::EIO);
        }
        if receipt.data == 0 { Ok(()) } else { Err(i32::try_from(receipt.data).unwrap_or(libc::EIO)) }
    })
}
/// The longest timeout kevent takes: XNU refuses one past `INT32_MAX`
/// seconds with EINVAL (`timespec_is_valid`), where Linux's epoll takes any.
const LONGEST_WAIT_SECONDS: u64 = 0x7fff_ffff;
/// A relative timeout of `nanos`, cut to the longest kevent takes: some 68
/// years, so a wait woken early by the cut finds its deadline unpassed and
/// waits again.
fn timeout(nanos: u64) -> libc::timespec {
    libc::timespec {
        tv_sec: libc::time_t::try_from((nanos / 1_000_000_000).min(LONGEST_WAIT_SECONDS))
            .expect("the longest wait fits time_t"),
        tv_nsec: libc::c_long::try_from(nanos % 1_000_000_000).expect("subsecond fits c_long"),
    }
}
#[derive(Debug)]
pub(crate) struct Poller {
    fd: OwnedFd,
    events: Box<[libc::kevent]>,
    interests: BTreeMap<Fd, (bool, bool)>,
}
#[derive(Clone, Copy)]
pub(crate) struct Event {
    pub fd: Fd,
    pub read: bool,
    pub write: bool,
}
impl Poller {
    pub(crate) fn open(operations: u32) -> Result<Self, i32> {
        let capacity = usize::try_from(operations).expect("u32 fits usize");
        Ok(Self {
            fd: open()?,
            events: vec![event(0, 0, 0, 0); capacity].into_boxed_slice(),
            interests: BTreeMap::new(),
        })
    }
    pub(crate) fn interest(&mut self, fd: Fd, _existed: bool, read: bool, write: bool) -> Result<(), i32> {
        let (old_read, old_write) = self.interests.get(&fd).copied().unwrap_or((false, false));
        let side = |filter, add| {
            change(
                self.fd.as_raw_fd(),
                event(
                    usize::try_from(fd.raw()).expect("live fd"),
                    filter,
                    if add { libc::EV_ADD } else { libc::EV_DELETE },
                    0,
                ),
            )
        };
        if old_read != read {
            side(libc::EVFILT_READ, read)?;
        }
        if old_write != write
            && let Err(errno) = side(libc::EVFILT_WRITE, write)
        {
            if old_read != read {
                side(libc::EVFILT_READ, old_read).expect("restore the prior live interest");
            }
            return Err(errno);
        }
        if read || write {
            let _previous = self.interests.insert(fd, (read, write));
        } else {
            let _removed = self.interests.remove(&fd);
        }
        Ok(())
    }
    pub(crate) fn wait(&mut self, nanos: Option<u64>) -> Result<usize, i32> {
        let timeout = nanos.map(timeout);
        // SAFETY: the event buffer has the supplied writable capacity and
        // the optional relative timeout lives until return; no changes supplied.
        let count = unsafe {
            libc::kevent(
                self.fd.as_raw_fd(),
                ptr::null(),
                0,
                self.events.as_mut_ptr(),
                i32::try_from(self.events.len()).expect("event capacity fits i32"),
                timeout.as_ref().map_or(ptr::null(), ptr::from_ref),
            )
        };
        if count < 0 { Err(last_errno()) } else { Ok(usize::try_from(count).expect("nonnegative count")) }
    }
    pub(crate) fn event(&self, index: usize) -> Event {
        let raw = self.events.get(index).expect("an event returned by kevent");
        let both = raw.flags & (libc::EV_EOF | libc::EV_ERROR) != 0;
        Event {
            fd: Fd::new(i32::try_from(raw.ident).expect("the event identifies a descriptor")),
            read: both || raw.filter == libc::EVFILT_READ,
            write: both || raw.filter == libc::EVFILT_WRITE,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::{LONGEST_WAIT_SECONDS, timeout};

    #[test]
    fn a_timeout_keeps_its_nanoseconds_up_to_the_longest_kevent_takes() {
        let seconds = |time: libc::timespec| (time.tv_sec, time.tv_nsec);
        assert_eq!(seconds(timeout(0)), (0, 0));
        assert_eq!(seconds(timeout(1_500_000_000)), (1, 500_000_000));
        let longest = libc::time_t::try_from(LONGEST_WAIT_SECONDS).expect("fits time_t");
        assert_eq!(seconds(timeout(LONGEST_WAIT_SECONDS * 1_000_000_000 + 7)), (longest, 7));
        assert_eq!(seconds(timeout(u64::MAX)), (longest, 709_551_615));
        assert_eq!(i64::from(i32::MAX), longest, "XNU's timespec_is_valid takes up to INT32_MAX seconds");
    }
}
