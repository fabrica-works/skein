//! Ignored signals delivered by `EVFILT_SIGNAL` (shell.md, 7.5).
//!
//! # The `unsafe`, and why it is sound
//!
//! Sigaction copies initialized local dispositions, whose handler is `SIG_IGN`,
//! never a Rust callback. Kevent borrows local changes and output only until
//! return, with a zero timeout when reading. The harness's signal pipes carry
//! one integer below `PIPE_BUF` and are tracked separately from kqueues.

use super::{kqueue, last_errno};
use skein_io::kernel::{Done, Error, Fd, ServiceSignal, Signalled};
use std::{
    collections::BTreeSet,
    mem,
    os::fd::{AsRawFd, IntoRawFd},
    ptr,
    sync::Mutex,
};

// Descriptor identities are process-wide; the lock serializes their ownership
// when harnesses or separately hosted loops share one process (shell.md, 7.5).
static PIPES: Mutex<BTreeSet<Fd>> = Mutex::new(BTreeSet::new());
pub(super) fn remember_pipe(fd: Fd) {
    let _inserted = PIPES.lock().expect("signal pipe table is not poisoned").insert(fd);
}
pub(super) fn forget(fd: Fd) {
    let _removed = PIPES.lock().expect("signal pipe table is not poisoned").remove(&fd);
}
pub(super) fn open_termination_signals() -> Result<Fd, i32> {
    open_signals(crate::Watch::Termination)
}
pub(super) fn open_signals(watch: crate::Watch) -> Result<Fd, i32> {
    let queue = kqueue::open()?;
    let signals = match watch {
        crate::Watch::Termination => [libc::SIGINT, libc::SIGTERM, 0],
        crate::Watch::TerminationAndWindow => [libc::SIGINT, libc::SIGTERM, libc::SIGWINCH],
    };
    let mut prior = Vec::new();
    let result = (|| {
        for signal in signals.into_iter().filter(|signal| *signal != 0) {
            // SAFETY: sigaction is a C record valid when zeroed, then initialized below.
            let mut ignored: libc::sigaction = unsafe { mem::zeroed() };
            ignored.sa_sigaction = libc::SIG_IGN;
            // SAFETY: the live signal set is writable until sigemptyset returns.
            super::status(unsafe { libc::sigemptyset(ptr::from_mut(&mut ignored.sa_mask)) })?;
            // SAFETY: the prior action record is valid output storage at zero.
            let mut old: libc::sigaction = unsafe { mem::zeroed() };
            // SAFETY: both initialized records live through sigaction; SIG_IGN retains no pointer.
            super::status(unsafe { libc::sigaction(signal, ptr::from_ref(&ignored), ptr::from_mut(&mut old)) })?;
            prior.push((signal, old));
            kqueue::change(
                queue.as_raw_fd(),
                kqueue::event(usize::try_from(signal).expect("positive signal"), libc::EVFILT_SIGNAL, libc::EV_ADD, 0),
            )?;
        }
        Ok(())
    })();
    if let Err(errno) = result {
        for (signal, old) in prior.into_iter().rev() {
            // SAFETY: old is the disposition saved above; the pointer lives through restoration.
            let _restored = unsafe { libc::sigaction(signal, ptr::from_ref(&old), ptr::null_mut()) };
        }
        return Err(errno);
    }
    Ok(Fd::new(queue.into_raw_fd()))
}
fn decode(number: i32) -> Result<Done, Error> {
    let signal = match number {
        libc::SIGINT => Signalled::Termination { signal: ServiceSignal::Interrupt, sender: None },
        libc::SIGTERM => Signalled::Termination { signal: ServiceSignal::Terminate, sender: None },
        libc::SIGWINCH => Signalled::WindowChanged,
        _ => return Err(Error::Other(libc::EINVAL)),
    };
    Ok(Done::Signalled(signal))
}
pub(crate) fn read_signal(fd: Fd) -> Result<Done, Error> {
    if PIPES.lock().expect("signal pipe table is not poisoned").contains(&fd) {
        let mut bytes = [0; size_of::<i32>()];
        let count = super::read(fd, &mut bytes).map_err(Error::Other)?;
        if usize::try_from(count).expect("u32 fits usize") != bytes.len() {
            return Err(Error::Other(libc::EIO));
        }
        return decode(i32::from_ne_bytes(bytes));
    }
    let mut event = kqueue::event(0, 0, 0, 0);
    let timeout = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: one exclusive output event and a live zero timeout; no input changes.
    let count =
        unsafe { libc::kevent(fd.raw(), ptr::null(), 0, ptr::from_mut(&mut event), 1, ptr::from_ref(&timeout)) };
    if count < 0 {
        return Err(Error::Other(last_errno()));
    }
    if count == 0 {
        return Err(Error::Other(libc::EAGAIN));
    }
    if event.filter != libc::EVFILT_SIGNAL || event.flags & libc::EV_ERROR != 0 {
        return Err(Error::Other(libc::EINVAL));
    }
    decode(i32::try_from(event.ident).map_err(|_| Error::Other(libc::EINVAL))?)
}
