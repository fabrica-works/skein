//! Shared operation errors, path checks and bounds (shell.md, 7.5).

use skein_io::kernel::{Error, Op};

use crate::system::{self, status_flags};

/// How many times an `Open` is submitted again after `EAGAIN` before its
/// `Other(EAGAIN)` goes up.
pub(crate) const AGAIN: u32 = 16;

/// The bytes of `getdents64`'s buffer: many entries a call, and room for
/// the largest one.
// Only Linux uses a getdents buffer; Darwin owns directory streams.
#[cfg(target_os = "linux")]
pub(crate) const LISTING: usize = 32 << 10;

/// The error number `errno` of `kind`, as `skein_io::kernel::Error`
/// documents it, per operation. `cancelled` says a `Cancel` was submitted
/// for the operation, so an `EINTR` is that cancel landing.
pub(crate) fn error(kind: &Op, errno: i32, cancelled: bool) -> Error {
    match kind {
        Op::Cancel { .. } => match errno {
            libc::ENOENT | libc::EALREADY => Error::TooLate,
            libc::EINVAL => Error::InvalidArgument,
            other => Error::Other(other),
        },
        // Cancelled only if a Cancel was submitted for it, which io does
        // for an Open, a Read, a Write or a Sync only.
        Op::Open { .. }
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
        | Op::List { .. } => match errno {
            libc::ECANCELED | libc::EINTR if cancelled => Error::Cancelled,
            libc::ENOBUFS | libc::ENOMEM => Error::NoBufferSpace,
            libc::EINVAL => Error::InvalidArgument,
            other => file_error(kind, other),
        },
        Op::Window { .. } | Op::Usage | Op::Spawn { .. } | Op::Signal { .. } => Error::Other(errno),
        Op::ReadSignal { .. } => match errno {
            libc::ECANCELED => Error::Cancelled,
            libc::EINTR if cancelled => Error::Cancelled,
            libc::EINVAL => Error::InvalidArgument,
            other => Error::Other(other),
        },
        Op::Wait { .. } => match errno {
            libc::ECANCELED | libc::EINTR if cancelled => Error::Cancelled,
            libc::ENOBUFS | libc::ENOMEM => Error::NoBufferSpace,
            libc::EINVAL => Error::InvalidArgument,
            other => Error::Other(other),
        },
        Op::Socket { .. }
        | Op::Bind { .. }
        | Op::Listen { .. }
        | Op::Accept { .. }
        | Op::Connect { .. }
        | Op::Recv { .. }
        | Op::Send { .. }
        | Op::PipeRead { .. }
        | Op::PipeWrite { .. }
        | Op::Shutdown { .. }
        | Op::Close { .. } => match errno {
            libc::ECANCELED => Error::Cancelled,
            libc::EINTR if cancelled => Error::Cancelled,
            libc::ENOBUFS | libc::ENOMEM => Error::NoBufferSpace,
            libc::EINVAL | libc::EAFNOSUPPORT => Error::InvalidArgument,
            other => operation_error(kind, other),
        },
    }
}

/// The errors that mean something on one operation and not another.
fn operation_error(kind: &Op, errno: i32) -> Error {
    let named = match kind {
        Op::Socket { .. } => match errno {
            libc::EMFILE | libc::ENFILE => Some(Error::TooManyOpenFiles),
            _ => None,
        },
        Op::Bind { .. } => match errno {
            libc::EADDRINUSE => Some(Error::AddressInUse),
            libc::EADDRNOTAVAIL => Some(Error::AddressNotAvailable),
            _ => None,
        },
        Op::Listen { .. } => match errno {
            libc::EADDRINUSE => Some(Error::AddressInUse),
            _ => None,
        },
        Op::Accept { .. } => match errno {
            libc::EMFILE | libc::ENFILE => Some(Error::TooManyOpenFiles),
            libc::ECONNABORTED => Some(Error::Reset),
            _ => None,
        },
        Op::Connect { .. } => match errno {
            libc::ECONNREFUSED => Some(Error::Refused),
            libc::EADDRNOTAVAIL => Some(Error::AddressNotAvailable),
            other => stream_error(other),
        },
        Op::Recv { .. } => match errno {
            libc::ENOTCONN => Some(Error::NotConnected),
            other => stream_error(other),
        },
        Op::Send { .. } => match errno {
            libc::EPIPE => Some(Error::BrokenPipe),
            other => stream_error(other),
        },
        Op::PipeRead { .. } | Op::PipeWrite { .. } => match errno {
            libc::EPIPE => Some(Error::BrokenPipe),
            _ => None,
        },
        Op::Shutdown { .. } => match errno {
            libc::ENOTCONN => Some(Error::NotConnected),
            _ => None,
        },
        Op::Close { .. } => None,
        Op::Open { .. }
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
        | Op::Wait { .. }
        | Op::Signal { .. }
        | Op::Usage
        | Op::Window { .. }
        | Op::ReadSignal { .. }
        | Op::Cancel { .. } => unreachable!("an operation on files' errors, and a Cancel's, are mapped apart"),
    };
    named.unwrap_or(Error::Other(errno))
}

/// The errors of an operation on files: each names those the contract
/// tables for it (`skein_io::kernel`), and anything else is `Other`.
pub(crate) fn file_error(kind: &Op, errno: i32) -> Error {
    let named = match kind {
        Op::Open { .. } => match errno {
            libc::ENOENT => Some(Error::NotFound),
            libc::EEXIST => Some(Error::Exists),
            libc::ENOTDIR => Some(Error::NotADirectory),
            libc::EISDIR => Some(Error::IsADirectory),
            libc::EACCES | libc::EPERM => Some(Error::Permission),
            libc::ENOSPC | libc::EDQUOT => Some(Error::NoSpace),
            libc::EROFS => Some(Error::ReadOnly),
            libc::ELOOP => Some(Error::TooManyLinks),
            libc::ENAMETOOLONG => Some(Error::NameTooLong),
            // RESOLVE_BENEATH's answer to a path out of its root.
            libc::EXDEV => Some(Error::Escape),
            // A socket, or a device with no driver.
            libc::ENXIO => Some(Error::NotAFile),
            libc::EMFILE | libc::ENFILE => Some(Error::TooManyOpenFiles),
            _ => None,
        },
        Op::Read { .. } => match errno {
            libc::EISDIR => Some(Error::IsADirectory),
            _ => None,
        },
        Op::Write { .. } | Op::Append { .. } => match errno {
            libc::ENOSPC | libc::EDQUOT => Some(Error::NoSpace),
            libc::EROFS => Some(Error::ReadOnly),
            _ => None,
        },
        Op::Truncate { .. } => match errno {
            libc::EACCES | libc::EPERM => Some(Error::Permission),
            libc::ENOSPC | libc::EDQUOT => Some(Error::NoSpace),
            libc::EROFS => Some(Error::ReadOnly),
            _ => None,
        },
        Op::Sync { .. } => match errno {
            libc::ENOSPC | libc::EDQUOT => Some(Error::NoSpace),
            _ => None,
        },
        Op::Lock { .. } => match errno {
            libc::EWOULDBLOCK => Some(Error::Busy),
            _ => None,
        },
        Op::Stat { .. } => None,
        // EXDEV here is two filesystems, not an escape: Other.
        Op::Rename { no_replace, .. } => match errno {
            libc::ENOENT => Some(Error::NotFound),
            libc::ENOTDIR => Some(Error::NotADirectory),
            libc::EISDIR => Some(Error::IsADirectory),
            libc::EEXIST if *no_replace => Some(Error::Exists),
            libc::ENOTEMPTY | libc::EEXIST => Some(Error::NotEmpty),
            libc::EACCES | libc::EPERM => Some(Error::Permission),
            libc::ENOSPC | libc::EDQUOT => Some(Error::NoSpace),
            libc::EROFS => Some(Error::ReadOnly),
            libc::EMLINK => Some(Error::TooManyLinks),
            libc::ENAMETOOLONG => Some(Error::NameTooLong),
            _ => None,
        },
        Op::Remove { .. } => match errno {
            libc::ENOENT => Some(Error::NotFound),
            libc::ENOTDIR => Some(Error::NotADirectory),
            libc::EISDIR => Some(Error::IsADirectory),
            libc::ENOTEMPTY | libc::EEXIST => Some(Error::NotEmpty),
            libc::EACCES | libc::EPERM => Some(Error::Permission),
            libc::EROFS => Some(Error::ReadOnly),
            libc::ENAMETOOLONG => Some(Error::NameTooLong),
            _ => None,
        },
        Op::MakeDirectory { .. } => match errno {
            libc::EEXIST => Some(Error::Exists),
            libc::ENOENT => Some(Error::NotFound),
            libc::ENOTDIR => Some(Error::NotADirectory),
            libc::EACCES | libc::EPERM => Some(Error::Permission),
            libc::ENOSPC | libc::EDQUOT => Some(Error::NoSpace),
            libc::EROFS => Some(Error::ReadOnly),
            libc::EMLINK => Some(Error::TooManyLinks),
            libc::ENAMETOOLONG => Some(Error::NameTooLong),
            _ => None,
        },
        Op::List { .. } => match errno {
            libc::ENOENT => Some(Error::NotFound),
            libc::ENOTDIR => Some(Error::NotADirectory),
            libc::ENAMETOOLONG => Some(Error::NameTooLong),
            _ => None,
        },
        Op::Socket { .. }
        | Op::Bind { .. }
        | Op::Listen { .. }
        | Op::Accept { .. }
        | Op::Connect { .. }
        | Op::Recv { .. }
        | Op::Send { .. }
        | Op::Shutdown { .. }
        | Op::Close { .. }
        | Op::PipeRead { .. }
        | Op::PipeWrite { .. }
        | Op::Spawn { .. }
        | Op::Wait { .. }
        | Op::Signal { .. }
        | Op::Usage
        | Op::Window { .. }
        | Op::ReadSignal { .. }
        | Op::Cancel { .. } => unreachable!("only an operation on files' errors are mapped here"),
    };
    named.unwrap_or(Error::Other(errno))
}

/// Path-only descriptors never enter an operation other than Stat or Close (kernel.md, 6.1).
pub(crate) fn check_path_access(operation: &Op) {
    let descriptors = match operation {
        Op::Stat { .. } | Op::Close { .. } | Op::Socket { .. } | Op::Usage | Op::Window { .. } | Op::Cancel { .. } => {
            return;
        }
        Op::Bind { fd, .. }
        | Op::Listen { fd, .. }
        | Op::Accept { fd }
        | Op::Connect { fd, .. }
        | Op::Recv { fd, .. }
        | Op::Send { fd, .. }
        | Op::Shutdown { fd }
        | Op::Read { fd, .. }
        | Op::Write { fd, .. }
        | Op::Append { fd, .. }
        | Op::Truncate { fd, .. }
        | Op::Sync { fd }
        | Op::Lock { fd }
        | Op::List { fd, .. }
        | Op::ReadSignal { fd }
        | Op::PipeRead { fd, .. }
        | Op::PipeWrite { fd, .. } => [Some(*fd), None],
        Op::Open { root, .. } => [Some(*root), None],
        Op::Rename { from_dir, to_dir, .. } => [Some(*from_dir), Some(*to_dir)],
        Op::Remove { dir, .. } | Op::MakeDirectory { dir, .. } => [Some(*dir), None],
        Op::Spawn { spawn } => [Some(spawn.root), None],
        Op::Wait { pidfd, .. } | Op::Signal { pidfd, .. } => [Some(*pidfd), None],
    };
    for descriptor in descriptors.into_iter().flatten() {
        if let Ok(flags) = status_flags(descriptor.raw()) {
            assert_eq!(flags & system::PATH_ONLY, 0, "only Stat and Close may use a path-only descriptor");
        }
    }
}

/// The errors of a connection, on `Connect`, `Recv` and `Send`.
fn stream_error(errno: i32) -> Option<Error> {
    match errno {
        libc::ECONNRESET => Some(Error::Reset),
        libc::ENETUNREACH | libc::EHOSTUNREACH => Some(Error::Unreachable),
        libc::ETIMEDOUT => Some(Error::TimedOut),
        _ => None,
    }
}

pub(crate) fn release_at_floor(release: &[u8], floor: (u32, u32)) -> Result<bool, i32> {
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
    Ok((major, minor) >= floor)
}

#[cfg(test)]
mod tests {
    use super::release_at_floor;

    #[test]
    fn darwin_releases() {
        for (release, expected) in [
            (b"24.0.0".as_slice(), Ok(true)),
            (b"24.6.0", Ok(true)),
            (b"25.1.0", Ok(true)),
            (b"23.6.0", Ok(false)),
            (b"24", Err(libc::EINVAL)),
        ] {
            assert_eq!(release_at_floor(release, (24, 0)), expected);
        }
    }

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
            assert_eq!(release_at_floor(release, (6, 12)), *expected, "{release:?}");
        }
    }
}
