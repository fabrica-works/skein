//! Startup's inherited and held listeners (shell.md, 6.6).

use std::net::SocketAddr;
use std::os::unix::ffi::OsStrExt;

use skein_io::kernel::{Error, Family, Fd, Op};

use crate::system;

/// Startup inspects at most this many inherited sockets. The fixed bound
/// limits allocation and synchronous startup work before service limits
/// exist; io's adoption separately enforces the service's socket capacity.
pub const MAX_INHERITED_LISTENERS: u32 = 1024;

/// Why startup refused inherited listeners, beside `RootsRefusal` for roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenersRefusal {
    /// `LISTEN_FDS` contains something other than ASCII decimal digits.
    BadCount,
    /// `LISTEN_FDS` exceeds the startup inspection bound (including overflow).
    TooMany { bound: u32 },
    /// The descriptor is not open.
    NotOpen { fd: Fd },
    /// The descriptor's stat mode identifies a non-socket, such as a pipe.
    NotSocket { fd: Fd, mode: u32 },
    /// The socket is not a stream socket; `kind` is its `SO_TYPE`.
    NotStream { fd: Fd, kind: i32 },
    /// The stream socket is not listening.
    NotListening { fd: Fd },
    /// The listening socket's address is neither IPv4 nor IPv6.
    NotIp { fd: Fd },
    /// A descriptor check or close-on-exec update failed with this errno.
    Kernel { fd: Fd, errno: i32 },
}

/// Reads `LISTEN_FDS`, never `LISTEN_PID`, and validates descriptors starting
/// at 3. Absent, empty and zero counts return none. The environment is left
/// intact. Successful descriptors are made close-on-exec, without changing
/// their status flags or taking ownership. On refusal all descriptors remain
/// the caller's; preceding descriptors may already be close-on-exec.
pub fn inherited_listeners() -> Result<Box<[(Fd, SocketAddr)]>, ListenersRefusal> {
    let count = match std::env::var_os("LISTEN_FDS") {
        None => 0,
        Some(value) => count(value.as_os_str().as_bytes())?,
    };
    let mut listeners = Vec::with_capacity(usize::try_from(count).expect("bounded count fits usize"));
    for offset in 0..count {
        let fd = Fd::new(i32::try_from(offset).expect("bounded offset fits i32").checked_add(3).expect("bounded fd"));
        let addr = listener_at(fd)?;
        system::listener_calls::close_on_exec(fd).map_err(|errno| ListenersRefusal::Kernel { fd, errno })?;
        listeners.push((fd, addr));
    }
    Ok(listeners.into_boxed_slice())
}

fn count(bytes: &[u8]) -> Result<u32, ListenersRefusal> {
    if !bytes.iter().all(u8::is_ascii_digit) {
        return Err(ListenersRefusal::BadCount);
    }
    let mut count = 0_u32;
    for digit in bytes {
        count = count
            .checked_mul(10)
            .and_then(|n| n.checked_add(u32::from(digit.saturating_sub(b'0'))))
            .ok_or(ListenersRefusal::TooMany { bound: MAX_INHERITED_LISTENERS })?;
        if count > MAX_INHERITED_LISTENERS {
            return Err(ListenersRefusal::TooMany { bound: MAX_INHERITED_LISTENERS });
        }
    }
    Ok(count)
}

/// Validates one listening IPv4 or IPv6 stream socket, leaving its flags
/// and ownership as they are (shell.md, 6.6).
pub fn listener_at(fd: Fd) -> Result<SocketAddr, ListenersRefusal> {
    system::listener_calls::listener_at(fd)
}

/// Opens a held listener, returning its kernel-selected address at port 0.
/// Uses io's socket defaults: nonblocking, close-on-exec, `TCP_NODELAY`,
/// IPv6-only for IPv6, and `SO_REUSEADDR` before bind. The caller owns the
/// returned descriptor; a failed setup closes the fresh descriptor.
pub fn open_listener(addr: SocketAddr, backlog: u32) -> Result<(Fd, SocketAddr), Error> {
    let family = Family::of(&addr);
    let fd = system::socket(family).map_err(|errno| crate::calls::error(&Op::Socket { family }, errno, false))?;
    let result = configure(fd, family).and_then(|()| {
        system::bind(fd, &addr).map_err(|errno| crate::calls::error(&Op::Bind { fd, addr }, errno, false))?;
        system::listen(fd, backlog).map_err(|errno| crate::calls::error(&Op::Listen { fd, backlog }, errno, false))?;
        system::bound_addr(fd).map_err(Error::Other)
    });
    match result {
        Ok(bound) => Ok((fd, bound)),
        Err(error) => {
            let _closed = system::close(fd);
            Err(error)
        }
    }
}

fn configure(fd: Fd, family: Family) -> Result<(), Error> {
    system::retry(|| system::setsockopt(fd.raw(), libc::IPPROTO_TCP, libc::TCP_NODELAY)).map_err(Error::Other)?;
    if family == Family::Ipv6 {
        system::retry(|| system::setsockopt(fd.raw(), libc::IPPROTO_IPV6, libc::IPV6_V6ONLY)).map_err(Error::Other)?;
    }
    system::retry(|| system::setsockopt(fd.raw(), libc::SOL_SOCKET, libc::SO_REUSEADDR)).map_err(Error::Other)
}

#[cfg(test)]
mod tests {
    use super::{ListenersRefusal, MAX_INHERITED_LISTENERS, count};

    #[test]
    fn decimal_count_boundaries() {
        for bytes in [b"".as_slice(), b"0", b"000"] {
            assert_eq!(count(bytes), Ok(0), "empty or zero count");
        }
        assert_eq!(count(b"1024"), Ok(MAX_INHERITED_LISTENERS), "bound admitted");
        for bytes in [b"1025".as_slice(), b"999999999999999999999999999999999999999"] {
            assert_eq!(
                count(bytes),
                Err(ListenersRefusal::TooMany { bound: MAX_INHERITED_LISTENERS }),
                "past bound refused"
            );
        }
        for bytes in [b"+1".as_slice(), b"-1", b" 1", b"1 ", b"1x", &[0xff]] {
            assert_eq!(count(bytes), Err(ListenersRefusal::BadCount), "only ASCII decimal digits");
        }
    }
}
