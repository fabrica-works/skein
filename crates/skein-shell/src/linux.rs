//! Linux's calls and ring adapter (shell.md, 7.5).
//!
//! # The `unsafe`, and why it is sound
//!
//! - Socket, listen, shutdown and close take only integers; no pointer is
//!   retained. A successful socket descriptor is handed to io for Close.
//! - Bind and connect read a local, initialized `sockaddr_storage` encoded by
//!   the address encoder, with exactly the encoded length.
//! - Accept writes into initialized local address storage and its length,
//!   whose capacity is passed in; both live until the call returns.
//! - Recv writes only into the exclusive slice's capacity. Send reads only
//!   its borrowed slice. Neither call retains a pointer after returning.
//! - `SO_ERROR` writes one initialized integer, with its size passed by a
//!   local `socklen_t`. The setsockopt and getsockname wrappers share
//!   their address and option checks with this backend.
//! - Uname writes a whole local utsname, a plain C structure of byte arrays.
//! - File calls' paths, offsets and buffers are documented in `files`.
//! - Fcntl changes only the flags of a just-spawned parent pipe description;
//!   it takes integers and retains no pointer.
//! - A stream read/write borrows its slice for the call only. A signal read
//!   writes one initialized local signalfd record, with its exact size passed.
//! - Epoll's event pointers and ownership are documented in `epoll`.

#![expect(unsafe_code, reason = "Linux calls borrow buffers only until they return (shell.md, 7.5)")]

pub(crate) mod epoll;
pub(crate) mod files;
pub(crate) mod group;

use std::{mem, ptr};

use skein_io::kernel::{Addr, Family, Fd};

pub(crate) mod opens;
pub(crate) mod process;
pub(crate) mod ring;
mod roots;
mod startup;
mod sync;
pub(crate) mod terminal;

pub use roots::make_root;
pub(crate) use startup::{read_file, read_private_file};
use sync::encode;
pub(crate) use sync::{
    clock_nanos, decode_addr, domain, last_errno, list, lock, opened, random, socklen_of, status_flags, zeroed_storage,
};
pub(crate) use sync::{close as close_unchecked, local_addr as getsockname, set_option as setsockopt};
pub use sync::{directory_access, effective_user, open_append, open_root, prepare_output, process_id};

/// Interrupted calls have made no progress; retry without changing counts.
pub(crate) fn retry<T>(mut call: impl FnMut() -> Result<T, i32>) -> Result<T, i32> {
    loop {
        match call() {
            Err(libc::EINTR) => {}
            result => return result,
        }
    }
}

fn status(result: i32) -> Result<(), i32> {
    if result < 0 { Err(last_errno()) } else { Ok(()) }
}

pub(crate) fn socket(family: Family) -> Result<Fd, i32> {
    retry(|| {
        // SAFETY: socket takes integer arguments and returns a new descriptor.
        let fd = unsafe {
            libc::socket(
                domain(family),
                libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                libc::IPPROTO_TCP,
            )
        };
        if fd < 0 { Err(last_errno()) } else { Ok(Fd::new(fd)) }
    })
}

pub(crate) fn bind(fd: Fd, addr: &Addr) -> Result<(), i32> {
    let mut storage = zeroed_storage();
    let len = encode(addr, &mut storage);
    retry(|| {
        // SAFETY: encode initialized len bytes of storage, which lives across bind.
        status(unsafe { libc::bind(fd.raw(), ptr::from_ref(&storage).cast(), len) })
    })
}

/// Waits up to `bound` for `POLLIN`, without consuming input or driving a
/// `Kernel`. Returns false on timeout and an errno on descriptor or poll
/// errors. Interrupted waits keep the same monotonic deadline.
pub fn wait_readable(fd: Fd, bound: skein_lib::Duration) -> Result<bool, i32> {
    if fd.raw() < 0 {
        return Err(libc::EBADF);
    }
    let clock = crate::Clock::new();
    let deadline = clock.now().now.saturating_add(bound);
    loop {
        let left = deadline.saturating_since(clock.now().now);
        let millis = i32::try_from(left.as_nanos().div_ceil(1_000_000)).unwrap_or(i32::MAX);
        let mut watched = libc::pollfd { fd: fd.raw(), events: libc::POLLIN, revents: 0 };
        // SAFETY: watched is one initialized, exclusive pollfd, writable until
        // poll returns. The call retains no pointer and takes no ownership of fd.
        let result = unsafe { libc::poll(ptr::from_mut(&mut watched), 1, millis) };
        if result < 0 {
            let errno = last_errno();
            if errno == libc::EINTR && clock.now().now < deadline {
                continue;
            }
            return Err(errno);
        }
        if watched.revents & libc::POLLNVAL != 0 {
            return Err(libc::EBADF);
        }
        if watched.revents & (libc::POLLERR | libc::POLLHUP) != 0 {
            return Err(libc::EIO);
        }
        if watched.revents & libc::POLLIN != 0 {
            return Ok(true);
        }
        if clock.now().now >= deadline {
            return Ok(false);
        }
    }
}

pub(crate) fn listen(fd: Fd, backlog: u32) -> Result<(), i32> {
    let backlog = i32::try_from(backlog).unwrap_or(i32::MAX);
    retry(|| {
        // SAFETY: listen takes only integers.
        status(unsafe { libc::listen(fd.raw(), backlog) })
    })
}

pub(crate) fn accept(fd: Fd) -> Result<(Fd, Addr), i32> {
    let mut storage = zeroed_storage();
    retry(|| {
        let mut len = socklen_of::<libc::sockaddr_storage>();
        // SAFETY: storage and len are exclusive locals, of the stated capacity.
        let accepted = unsafe {
            libc::accept4(
                fd.raw(),
                ptr::from_mut(&mut storage).cast(),
                ptr::from_mut(&mut len),
                libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            )
        };
        if accepted < 0 {
            return Err(last_errno());
        }
        let peer = decode_addr(&storage, len).expect("an accepted TCP socket's peer is IPv4 or IPv6");
        Ok((Fd::new(accepted), peer))
    })
}

pub(crate) fn connect(fd: Fd, addr: &Addr) -> Result<(), i32> {
    let mut storage = zeroed_storage();
    let len = encode(addr, &mut storage);
    retry(|| {
        // SAFETY: storage is initialized for len bytes and lives across the call.
        status(unsafe { libc::connect(fd.raw(), ptr::from_ref(&storage).cast(), len) })
    })
}

pub(crate) fn recv(fd: Fd, buf: &mut [u8]) -> Result<u32, i32> {
    retry(|| {
        // SAFETY: buf is exclusively writable for its length during this call.
        let result = unsafe { libc::recv(fd.raw(), buf.as_mut_ptr().cast(), buf.len(), libc::MSG_DONTWAIT) };
        if result < 0 { Err(last_errno()) } else { Ok(u32::try_from(result).expect("a valid Recv's count fits u32")) }
    })
}

pub(crate) fn send(fd: Fd, bytes: &[u8]) -> Result<u32, i32> {
    retry(|| {
        // SAFETY: bytes is readable for its length during this call only.
        let result = unsafe {
            libc::send(fd.raw(), bytes.as_ptr().cast(), bytes.len(), libc::MSG_NOSIGNAL | libc::MSG_DONTWAIT)
        };
        if result < 0 { Err(last_errno()) } else { Ok(u32::try_from(result).expect("a valid Send's count fits u32")) }
    })
}

pub(crate) fn shutdown(fd: Fd) -> Result<(), i32> {
    retry(|| {
        // SAFETY: shutdown takes only integers.
        status(unsafe { libc::shutdown(fd.raw(), libc::SHUT_WR) })
    })
}

pub(crate) fn close(fd: Fd) -> Result<(), i32> {
    // Linux releases the descriptor even on EINTR; retry could close a reused fd.
    // SAFETY: close takes only the descriptor, whose ownership io relinquishes.
    status(unsafe { libc::close(fd.raw()) })
}

pub(crate) fn getsockopt(fd: Fd) -> Result<i32, i32> {
    retry(|| {
        let mut error: libc::c_int = 0;
        let mut len = socklen_of::<libc::c_int>();
        // SAFETY: error and len are exclusive locals; len states error's size.
        let result = unsafe {
            libc::getsockopt(
                fd.raw(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                ptr::from_mut(&mut error).cast(),
                ptr::from_mut(&mut len),
            )
        };
        status(result)?;
        Ok(error)
    })
}

pub(crate) fn release() -> Result<Box<[u8]>, i32> {
    // SAFETY: utsname is a plain C structure of byte arrays, valid when zeroed.
    let mut name: libc::utsname = unsafe { mem::zeroed() };
    // SAFETY: name is exclusive and writable for one utsname throughout the call.
    status(unsafe { libc::uname(ptr::from_mut(&mut name)) })?;
    // SAFETY: uname wrote a NUL-terminated release in name, which lives across this borrow.
    let release = unsafe { std::ffi::CStr::from_ptr(name.release.as_ptr()) };
    Ok(Box::from(release.to_bytes()))
}

pub(crate) const PATH_ONLY: i32 = libc::O_PATH;
pub(crate) const FLOOR: (u32, u32) = (6, 12);
pub(crate) const UNWATCHABLE: i32 = libc::EPERM;
pub(crate) use epoll::{Epoll as Poller, Event};

pub(crate) fn nonblocking(fd: Fd) -> Result<(), i32> {
    let flags = retry(|| status_flags(fd.raw()))?;
    retry(|| {
        // SAFETY: fcntl takes integers and fd is the parent's fresh pipe end.
        status(unsafe { libc::fcntl(fd.raw(), libc::F_SETFL, flags | libc::O_NONBLOCK) })
    })
}

pub(crate) fn read(fd: Fd, buf: &mut [u8]) -> Result<u32, i32> {
    retry(|| {
        // SAFETY: buf is exclusively writable for its stated length during read.
        let result = unsafe { libc::read(fd.raw(), buf.as_mut_ptr().cast(), buf.len()) };
        count(result)
    })
}

pub(crate) fn write(fd: Fd, bytes: &[u8]) -> Result<u32, i32> {
    retry(|| {
        // SAFETY: bytes remains readable for its stated length during write.
        let result = unsafe { libc::write(fd.raw(), bytes.as_ptr().cast(), bytes.len()) };
        count(result)
    })
}

fn count(result: isize) -> Result<u32, i32> {
    if result < 0 { Err(last_errno()) } else { Ok(u32::try_from(result).expect("a valid transfer's count fits u32")) }
}

fn signal_record(fd: Fd) -> Result<(libc::signalfd_siginfo, usize), i32> {
    // SAFETY: signalfd_siginfo is an integer C record, valid when zeroed.
    let mut info: libc::signalfd_siginfo = unsafe { mem::zeroed() };
    let count = retry(|| {
        // SAFETY: info is exclusively writable for exactly the size passed to read.
        let result =
            unsafe { libc::read(fd.raw(), ptr::from_mut(&mut info).cast(), size_of::<libc::signalfd_siginfo>()) };
        if result < 0 { Err(last_errno()) } else { Ok(usize::try_from(result).expect("a nonnegative read count")) }
    })?;
    Ok((info, count))
}

pub(crate) fn read_signal(fd: Fd) -> Result<skein_io::kernel::Done, skein_io::kernel::Error> {
    let (info, count) = signal_record(fd)
        .map_err(|errno| crate::calls::error(&skein_io::kernel::Op::ReadSignal { fd }, errno, false))?;
    sync::read_signal(&info, count)
}

pub(crate) fn bound_addr(fd: Fd) -> Result<Addr, i32> {
    let mut storage = zeroed_storage();
    let mut len = 0;
    retry(|| getsockname(fd.raw(), &mut storage, &mut len))?;
    decode_addr(&storage, len).ok_or(libc::EAFNOSUPPORT)
}

#[derive(Debug)]
pub(crate) struct Listings(Box<[u8]>);
impl Listings {
    pub(crate) fn new() -> Self {
        Self(vec![0; crate::calls::LISTING].into_boxed_slice())
    }
    pub(crate) fn list(
        &mut self,
        fd: Fd,
        entries: &mut [skein_io::kernel::Entry],
        names: &mut [u8],
    ) -> Result<u32, i32> {
        list(fd.raw(), entries, names, &mut self.0)
    }
    #[expect(clippy::unused_self, reason = "the listing face owns per-descriptor streams on Darwin")]
    pub(crate) fn forget(&mut self, _fd: Fd) {}
}
