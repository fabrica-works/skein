//! Linux's synchronous readiness calls (shell.md, 7.5).
//!
//! # The `unsafe`, and why it is sound
//!
//! - Socket, listen, shutdown and close take only integers; no pointer is
//!   retained. A successful socket descriptor is handed to io for Close.
//! - Bind and connect read a local, initialized `sockaddr_storage` encoded by
//!   the ring's address encoder, with exactly the encoded length.
//! - Accept writes into initialized local address storage and its length,
//!   whose capacity is passed in; both live until the call returns.
//! - Recv writes only into the exclusive slice's capacity. Send reads only
//!   its borrowed slice. Neither call retains a pointer after returning.
//! - `SO_ERROR` writes one initialized integer, with its size passed by a
//!   local `socklen_t`. The ring's setsockopt and getsockname wrappers share
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

use crate::ring::{decode_addr, domain, encode, last_errno, socklen_of, zeroed_storage};
pub(crate) use crate::ring::{local_addr as getsockname, set_option as setsockopt};

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

pub(crate) fn at_floor() -> Result<bool, i32> {
    // SAFETY: utsname is a plain C structure of byte arrays, valid when zeroed.
    let mut name: libc::utsname = unsafe { mem::zeroed() };
    // SAFETY: name is exclusive and writable for one utsname throughout the call.
    status(unsafe { libc::uname(ptr::from_mut(&mut name)) })?;
    // SAFETY: uname wrote a NUL-terminated release in name, which lives across this borrow.
    let release = unsafe { std::ffi::CStr::from_ptr(name.release.as_ptr()) };
    release_at_floor(release.to_bytes())
}

fn release_at_floor(release: &[u8]) -> Result<bool, i32> {
    let mut parts = release.split(|byte| *byte == b'.');
    let number = |part: &[u8]| {
        let mut digits = part.iter().copied().take_while(u8::is_ascii_digit).peekable();
        digits.peek()?;
        let mut value = 0_u32;
        for byte in digits {
            let digit = u32::from(byte.checked_sub(b'0')?);
            value = value.checked_mul(10)?.checked_add(digit)?;
        }
        Some(value)
    };
    let major = parts.next().and_then(number).ok_or(libc::EINVAL)?;
    let minor = parts.next().and_then(number).ok_or(libc::EINVAL)?;
    Ok((major, minor) >= (6, 12))
}

pub(crate) fn nonblocking(fd: Fd) -> Result<(), i32> {
    let flags = retry(|| crate::ring::status_flags(fd.raw()))?;
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

pub(crate) fn read_signal(fd: Fd) -> Result<(libc::signalfd_siginfo, usize), i32> {
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

#[cfg(test)]
mod tests {
    use super::release_at_floor;

    #[test]
    fn releases_at_above_below_and_without_the_floor() {
        let cases: &[(&[u8], Result<bool, i32>)] = &[
            (b"6.12", Ok(true)),
            (b"6.12.107+deb13-cloud-amd64", Ok(true)),
            (b"6.13-rc1", Ok(true)),
            (b"7.0.0", Ok(true)),
            (b"6.11.9", Ok(false)),
            (b"5.15.0-91-generic", Ok(false)),
            (b"10.1", Ok(true)),
            (b"6", Err(libc::EINVAL)),
            (b"", Err(libc::EINVAL)),
            (b"six.12", Err(libc::EINVAL)),
            (b"6.x", Err(libc::EINVAL)),
        ];
        for (release, expected) in cases {
            assert_eq!(release_at_floor(release), *expected, "{release:?}");
        }
    }
}
