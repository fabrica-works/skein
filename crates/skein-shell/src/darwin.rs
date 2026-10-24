//! Darwin's system boundary (shell.md, 7.5).
//!
//! # The `unsafe`, and why it is sound
//!
//! Socket addresses are initialized local storage, aligned for either IP
//! address and carrying Darwin's length byte. Calls borrow buffers only for
//! their supplied lengths until return; none retains Rust pointers. Socket
//! options borrow one initialized integer. Clock and uname output are local
//! C records, read only after success. Entropy writes exclusive slices of at
//! most 256 bytes. Descriptor and identity calls take integers only. Fresh
//! sockets are closed on configuration failure; the loop also owns spawning,
//! so no spawn can intervene before close-on-exec is set. Other cases are
//! documented beside the files, listing, process, signals and terminal calls.

#![expect(unsafe_code, reason = "Darwin's synchronous calls retain no borrowed buffers (shell.md, 7.5)")]

pub(crate) mod files;
pub(crate) mod group;
mod kqueue;
#[path = "listener_calls.rs"]
pub(crate) mod listener_calls;
mod listing;
pub(crate) mod process;
mod signals;
pub(crate) mod terminal;

use skein_io::kernel::{Addr, Family, Fd};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::{ffi::CStr, mem, ptr};

pub use files::{directory_access, effective_user, make_root, open_append, open_root, prepare_output, process_id};
pub(crate) use files::{lock, opened, read_file, read_private_file};
pub(crate) use kqueue::{Event, Poller};
pub(crate) use listing::Listings;
pub(crate) use signals::read_signal;

pub(crate) const PATH_ONLY: i32 = libc::O_EVTONLY;
pub(crate) const FLOOR: (u32, u32) = (24, 0);
pub(crate) const UNWATCHABLE: i32 = libc::EINVAL;

pub(crate) fn last_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(libc::EIO)
}
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
fn count(result: isize) -> Result<u32, i32> {
    if result < 0 { Err(last_errno()) } else { Ok(u32::try_from(result).expect("io transfer counts fit u32")) }
}
pub(crate) fn status_flags(fd: i32) -> Result<i32, i32> {
    // SAFETY: F_GETFL takes a descriptor and no pointer.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 { Err(last_errno()) } else { Ok(flags) }
}
pub(crate) fn cloexec(fd: Fd) -> Result<(), i32> {
    retry(|| {
        // SAFETY: F_GETFD takes only integer arguments.
        let flags = unsafe { libc::fcntl(fd.raw(), libc::F_GETFD) };
        if flags < 0 {
            return Err(last_errno());
        }
        // SAFETY: F_SETFD sets integer flags on this owned descriptor.
        status(unsafe { libc::fcntl(fd.raw(), libc::F_SETFD, flags | libc::FD_CLOEXEC) })
    })
}
pub(crate) fn nonblocking(fd: Fd) -> Result<(), i32> {
    let flags = retry(|| status_flags(fd.raw()))?;
    retry(|| {
        // SAFETY: integer flags on the backend's own description.
        status(unsafe { libc::fcntl(fd.raw(), libc::F_SETFL, flags | libc::O_NONBLOCK) })
    })
}
fn configure(raw: i32) -> Result<Fd, i32> {
    if raw < 0 {
        return Err(last_errno());
    }
    let fd = Fd::new(raw);
    if let Err(errno) = cloexec(fd).and_then(|()| nonblocking(fd)) {
        let _closed = close(fd);
        return Err(errno);
    }
    Ok(fd)
}
pub(crate) fn socket(family: Family) -> Result<Fd, i32> {
    let domain = match family {
        Family::Ipv4 => libc::AF_INET,
        Family::Ipv6 => libc::AF_INET6,
    };
    retry(|| {
        // SAFETY: socket takes integers, returning a new descriptor.
        configure(unsafe { libc::socket(domain, libc::SOCK_STREAM, libc::IPPROTO_TCP) })
    })
}
fn socklen<T>() -> libc::socklen_t {
    libc::socklen_t::try_from(size_of::<T>()).expect("socket size fits socklen_t")
}
fn storage() -> libc::sockaddr_storage {
    // SAFETY: sockaddr_storage is an integer C record valid at zero.
    unsafe { mem::zeroed() }
}
fn encode(addr: &Addr, storage: &mut libc::sockaddr_storage) -> libc::socklen_t {
    match addr {
        SocketAddr::V4(v4) => {
            let raw = libc::sockaddr_in {
                sin_len: u8::try_from(size_of::<libc::sockaddr_in>()).expect("IPv4 size fits u8"),
                sin_family: u8::try_from(libc::AF_INET).expect("family fits u8"),
                sin_port: v4.port().to_be(),
                sin_addr: libc::in_addr { s_addr: u32::from_ne_bytes(v4.ip().octets()) },
                sin_zero: [0; 8],
            };
            // SAFETY: storage is larger and at least as aligned as sockaddr_in.
            unsafe { ptr::from_mut(storage).cast::<libc::sockaddr_in>().write(raw) };
            socklen::<libc::sockaddr_in>()
        }
        SocketAddr::V6(v6) => {
            let raw = libc::sockaddr_in6 {
                sin6_len: u8::try_from(size_of::<libc::sockaddr_in6>()).expect("IPv6 size fits u8"),
                sin6_family: u8::try_from(libc::AF_INET6).expect("family fits u8"),
                sin6_port: v6.port().to_be(),
                sin6_flowinfo: v6.flowinfo(),
                sin6_addr: libc::in6_addr { s6_addr: v6.ip().octets() },
                sin6_scope_id: v6.scope_id(),
            };
            // SAFETY: storage is larger and at least as aligned as sockaddr_in6.
            unsafe { ptr::from_mut(storage).cast::<libc::sockaddr_in6>().write(raw) };
            socklen::<libc::sockaddr_in6>()
        }
    }
}
fn decode(storage: &libc::sockaddr_storage, len: libc::socklen_t) -> Result<Addr, i32> {
    match i32::from(storage.ss_family) {
        libc::AF_INET if len >= socklen::<libc::sockaddr_in>() => {
            // SAFETY: the returned family and length identify a complete IPv4 address.
            let raw = unsafe { ptr::from_ref(storage).cast::<libc::sockaddr_in>().read() };
            Ok(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::from(raw.sin_addr.s_addr.to_ne_bytes()),
                u16::from_be(raw.sin_port),
            )))
        }
        libc::AF_INET6 if len >= socklen::<libc::sockaddr_in6>() => {
            // SAFETY: the returned family and length identify a complete IPv6 address.
            let raw = unsafe { ptr::from_ref(storage).cast::<libc::sockaddr_in6>().read() };
            Ok(SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::from(raw.sin6_addr.s6_addr),
                u16::from_be(raw.sin6_port),
                raw.sin6_flowinfo,
                raw.sin6_scope_id,
            )))
        }
        _ => Err(libc::EAFNOSUPPORT),
    }
}
pub(crate) fn bind(fd: Fd, addr: &Addr) -> Result<(), i32> {
    let mut storage = storage();
    let len = encode(addr, &mut storage);
    retry(|| {
        // SAFETY: the initialized encoded address lives across bind.
        status(unsafe { libc::bind(fd.raw(), ptr::from_ref(&storage).cast(), len) })
    })
}
pub(crate) fn connect(fd: Fd, addr: &Addr) -> Result<(), i32> {
    let mut storage = storage();
    let len = encode(addr, &mut storage);
    retry(|| {
        // SAFETY: the initialized encoded address lives across connect.
        status(unsafe { libc::connect(fd.raw(), ptr::from_ref(&storage).cast(), len) })
    })
}
pub(crate) fn bound_addr(fd: Fd) -> Result<Addr, i32> {
    let mut storage = storage();
    let mut len = socklen::<libc::sockaddr_storage>();
    retry(|| {
        // SAFETY: storage is writable for its stated capacity; len is an exclusive local.
        status(unsafe { libc::getsockname(fd.raw(), ptr::from_mut(&mut storage).cast(), ptr::from_mut(&mut len)) })
    })?;
    decode(&storage, len)
}
pub(crate) fn accept(fd: Fd) -> Result<(Fd, Addr), i32> {
    let mut storage = storage();
    retry(|| {
        let mut len = socklen::<libc::sockaddr_storage>();
        // SAFETY: storage and len are exclusive, live output buffers of the stated capacity.
        let raw = unsafe { libc::accept(fd.raw(), ptr::from_mut(&mut storage).cast(), ptr::from_mut(&mut len)) };
        let accepted = configure(raw)?;
        match decode(&storage, len) {
            Ok(addr) => Ok((accepted, addr)),
            Err(errno) => {
                let _closed = close(accepted);
                Err(errno)
            }
        }
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
    retry(|| {
        // SAFETY: listen takes only integer arguments.
        status(unsafe { libc::listen(fd.raw(), i32::try_from(backlog).unwrap_or(i32::MAX)) })
    })
}
pub(crate) fn recv(fd: Fd, buf: &mut [u8]) -> Result<u32, i32> {
    retry(|| {
        // SAFETY: buf is exclusive for its length until recv returns.
        count(unsafe { libc::recv(fd.raw(), buf.as_mut_ptr().cast(), buf.len(), libc::MSG_DONTWAIT) })
    })
}
pub(crate) fn send(fd: Fd, bytes: &[u8]) -> Result<u32, i32> {
    retry(|| {
        // SAFETY: bytes remains readable for its length until send returns.
        count(unsafe { libc::send(fd.raw(), bytes.as_ptr().cast(), bytes.len(), libc::MSG_DONTWAIT) })
    })
    .map_err(|errno| {
        // XNU bsd/kern/uipc_socket.c:1900-1926: a disconnected stream
        // answers EPIPE at SS_CANTSENDMORE before checking so_error or
        // connection state. ENOTCONN here is a socket never connected,
        // which Linux answers EPIPE; give it up as EPIPE for one error table.
        if errno == libc::ENOTCONN { libc::EPIPE } else { errno }
    })
}
pub(crate) fn read(fd: Fd, buf: &mut [u8]) -> Result<u32, i32> {
    retry(|| {
        // SAFETY: buf is exclusive for its supplied length until read returns.
        count(unsafe { libc::read(fd.raw(), buf.as_mut_ptr().cast(), buf.len()) })
    })
}
pub(crate) fn write(fd: Fd, bytes: &[u8]) -> Result<u32, i32> {
    retry(|| {
        // SAFETY: bytes is readable for its supplied length until write returns.
        count(unsafe { libc::write(fd.raw(), bytes.as_ptr().cast(), bytes.len()) })
    })
}
pub(crate) fn close(fd: Fd) -> Result<(), i32> {
    // SAFETY: ownership of this integer descriptor is relinquished exactly once.
    status(unsafe { libc::close(fd.raw()) })
}
pub(crate) fn shutdown(fd: Fd) -> Result<(), i32> {
    retry(|| {
        // SAFETY: shutdown takes only integers.
        status(unsafe { libc::shutdown(fd.raw(), libc::SHUT_WR) })
    })
}
pub(crate) fn setsockopt(fd: i32, level: i32, name: i32) -> Result<(), i32> {
    let one: libc::c_int = 1;
    // SAFETY: the call reads exactly one initialized local integer.
    status(unsafe { libc::setsockopt(fd, level, name, ptr::from_ref(&one).cast(), socklen::<libc::c_int>()) })
}
pub(crate) fn getsockopt(fd: Fd) -> Result<i32, i32> {
    retry(|| {
        let mut error: libc::c_int = 0;
        let mut len = socklen::<libc::c_int>();
        // SAFETY: error and len are exclusive locals, with the output capacity supplied.
        status(unsafe {
            libc::getsockopt(
                fd.raw(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                ptr::from_mut(&mut error).cast(),
                ptr::from_mut(&mut len),
            )
        })?;
        Ok(error)
    })
}
pub(crate) fn release() -> Result<Box<[u8]>, i32> {
    // SAFETY: utsname contains only byte arrays, valid when zeroed.
    let mut name: libc::utsname = unsafe { mem::zeroed() };
    // SAFETY: uname writes one exclusive local utsname.
    status(unsafe { libc::uname(ptr::from_mut(&mut name)) })?;
    // SAFETY: successful uname NUL-terminated the release array.
    Ok(Box::from(unsafe { CStr::from_ptr(name.release.as_ptr()) }.to_bytes()))
}
pub(crate) fn clock_nanos(id: libc::clockid_t) -> Option<u64> {
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: time is a writable timespec alive until return.
    let rc = unsafe { libc::clock_gettime(id, ptr::from_mut(&mut time)) };
    assert_eq!(rc, 0, "the shell's clocks exist");
    u64::try_from(time.tv_sec).ok()?.checked_mul(1_000_000_000)?.checked_add(u64::try_from(time.tv_nsec).ok()?)
}
pub(crate) fn random(bytes: &mut [u8]) -> Result<(), i32> {
    for chunk in bytes.chunks_mut(256) {
        // SAFETY: the exclusive buffer is at most getentropy's 256-byte limit.
        status(unsafe { libc::getentropy(chunk.as_mut_ptr().cast(), chunk.len()) })?;
    }
    Ok(())
}
