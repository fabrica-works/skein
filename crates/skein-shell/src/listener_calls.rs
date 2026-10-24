//! Shared startup calls, compiled inside Linux's or Darwin's unsafe boundary.
//!
//! fstat and getsockopt write initialized local C records with their exact
//! capacities. No pointer escapes a call. fcntl takes only integers and
//! changes descriptor flags without changing the socket's status flags.

use std::net::SocketAddr;
use std::{mem, ptr};

use super::{bound_addr, last_errno, retry};
use crate::ListenersRefusal;
use skein_io::kernel::Fd;

pub(crate) fn listener_at(fd: Fd) -> Result<SocketAddr, ListenersRefusal> {
    inspect(fd).map_err(|errno| {
        if errno == libc::EBADF {
            ListenersRefusal::NotOpen { fd }
        } else if errno == libc::EAFNOSUPPORT {
            ListenersRefusal::NotIp { fd }
        } else {
            ListenersRefusal::Kernel { fd, errno }
        }
    })?
}

fn inspect(fd: Fd) -> Result<Result<SocketAddr, ListenersRefusal>, i32> {
    // SAFETY: stat is a plain C record whose integer fields are valid at zero.
    let mut stat: libc::stat = unsafe { mem::zeroed() };
    retry(|| {
        // SAFETY: stat is exclusively writable for one stat until fstat returns.
        status(unsafe { libc::fstat(fd.raw(), ptr::from_mut(&mut stat)) })
    })?;
    if stat.st_mode & libc::S_IFMT != libc::S_IFSOCK {
        #[cfg_attr(
            target_os = "linux",
            expect(
                clippy::useless_conversion,
                reason = "Darwin's mode_t is u16; the public refusal carries u32 on both systems"
            )
        )]
        let mode = u32::from(stat.st_mode);
        return Ok(Err(ListenersRefusal::NotSocket { fd, mode }));
    }
    let kind = option(fd, libc::SO_TYPE)?;
    if kind != libc::SOCK_STREAM {
        return Ok(Err(ListenersRefusal::NotStream { fd, kind }));
    }
    if option(fd, libc::SO_ACCEPTCONN)? == 0 {
        return Ok(Err(ListenersRefusal::NotListening { fd }));
    }
    bound_addr(fd).map(Ok)
}

fn option(fd: Fd, name: i32) -> Result<i32, i32> {
    retry(|| {
        let mut value: libc::c_int = 0;
        let mut len = libc::socklen_t::try_from(size_of::<libc::c_int>()).expect("integer size fits socklen_t");
        // SAFETY: value and len are exclusive initialized locals; len gives value's capacity.
        status(unsafe {
            libc::getsockopt(
                fd.raw(),
                libc::SOL_SOCKET,
                name,
                ptr::from_mut(&mut value).cast(),
                ptr::from_mut(&mut len),
            )
        })?;
        Ok(value)
    })
}

pub(crate) fn close_on_exec(fd: Fd) -> Result<(), i32> {
    let flags = retry(|| {
        // SAFETY: F_GETFD takes only an integer descriptor.
        let flags = unsafe { libc::fcntl(fd.raw(), libc::F_GETFD) };
        if flags < 0 { Err(last_errno()) } else { Ok(flags) }
    })?;
    retry(|| {
        // SAFETY: F_SETFD takes integers and preserves all existing descriptor flags.
        status(unsafe { libc::fcntl(fd.raw(), libc::F_SETFD, flags | libc::FD_CLOEXEC) })
    })
}

fn status(result: i32) -> Result<(), i32> {
    if result < 0 { Err(last_errno()) } else { Ok(()) }
}

#[cfg(test)]
#[path = "listener_tests.rs"]
mod tests;
