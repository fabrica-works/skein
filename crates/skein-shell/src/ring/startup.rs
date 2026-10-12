//! Startup's synchronous bounded regular-file reads (shell.md, 6.1).
//! Every descriptor stays here and is closed before returning bytes or a
//! refusal. No service or ring state is kept.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use skein_io::file::Unsafe;
use skein_io::kernel::Error;
use skein_lib::Secret;

use crate::startup::Unread;

use super::{close, kind_of, last_errno};

pub(crate) fn read_private_file(path: &Path, max: u32, user: u32) -> Result<Secret, Unread> {
    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| Unread::Kernel(Error::InvalidArgument))?;
    let flags = libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_CLOEXEC;
    let fd = match open_private(&path, flags) {
        Ok(fd) => fd,
        Err(libc::EACCES) => return Err(denied_private(&path, user)),
        Err(errno) => return Err(private_error(errno)),
    };
    let result = check_private(fd, user).and_then(|()| read_opened(fd, max)).map(Secret::new);
    close(fd);
    result
}

fn open_private(path: &CString, flags: i32) -> Result<i32, i32> {
    let how = [u64::try_from(flags).expect("positive flags"), 0, libc::RESOLVE_NO_SYMLINKS];
    // SAFETY: the NUL-terminated path and all three initialized open_how
    // fields remain live across the syscall, which retains no pointers.
    let fd =
        unsafe { libc::syscall(libc::SYS_openat2, libc::AT_FDCWD, path.as_ptr(), how.as_ptr(), size_of_val(&how)) };
    if fd < 0 { Err(last_errno()) } else { Ok(i32::try_from(fd).expect("a descriptor fits i32")) }
}

fn private_error(errno: i32) -> Unread {
    match errno {
        libc::ELOOP => Unread::Unsafe(Unsafe::Link),
        libc::ENOENT => Unread::Absent,
        other => Unread::Kernel(kernel_error(other)),
    }
}

fn denied_private(path: &CString, user: u32) -> Unread {
    let Ok(fd) = open_private(path, libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC) else {
        return Unread::Kernel(Error::Permission);
    };
    let checked = check_private(fd, user);
    close(fd);
    match checked {
        Err(Unread::Unsafe(found)) => Unread::Unsafe(found),
        Ok(()) | Err(Unread::Absent | Unread::NotAFile | Unread::TooLarge { .. } | Unread::Kernel(_)) => {
            Unread::Kernel(Error::Permission)
        }
    }
}

fn check_private(fd: i32, user: u32) -> Result<(), Unread> {
    // SAFETY: zero is valid stat storage; fstat initializes this exclusive
    // local during the call and retains no pointer.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: stat remains exclusive and live across fstat.
    if unsafe { libc::fstat(fd, ptr::from_mut(&mut stat)) } < 0 {
        return Err(Unread::Kernel(kernel_error(last_errno())));
    }
    let found = if stat.st_uid != user {
        Some(Unsafe::Owner { found: stat.st_uid, expected: user })
    } else if kind_of(stat.st_mode) == skein_io::kernel::Kind::Symlink {
        Some(Unsafe::Link)
    } else if kind_of(stat.st_mode) != skein_io::kernel::Kind::File {
        Some(Unsafe::Kind(kind_of(stat.st_mode)))
    } else if stat.st_mode & 0o077 != 0 {
        Some(Unsafe::Mode(stat.st_mode & 0o077))
    } else if stat.st_nlink != 1 {
        Some(Unsafe::Links(u32::try_from(stat.st_nlink).unwrap_or(u32::MAX)))
    } else {
        None
    };
    match found {
        Some(found) => Err(Unread::Unsafe(found)),
        None => Ok(()),
    }
}

pub(crate) fn read_file(path: &Path, max: u32) -> Result<Box<[u8]>, Unread> {
    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| Unread::Kernel(Error::InvalidArgument))?;
    // SAFETY: the NUL-terminated path remains live across open.
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_CLOEXEC) };
    if fd < 0 {
        let errno = last_errno();
        return if errno == libc::ENOENT { Err(Unread::Absent) } else { Err(Unread::Kernel(kernel_error(errno))) };
    }
    let result = read_opened(fd, max);
    close(fd);
    result
}

fn read_opened(fd: i32, max: u32) -> Result<Box<[u8]>, Unread> {
    // SAFETY: all-zero stat storage is valid; fstat initializes this live
    // exclusive local during the call and retains no pointer.
    let mut stat: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: stat is exclusive initialized storage that lives across fstat.
    if unsafe { libc::fstat(fd, ptr::from_mut(&mut stat)) } < 0 {
        return Err(Unread::Kernel(kernel_error(last_errno())));
    }
    if stat.st_mode & libc::S_IFMT != libc::S_IFREG {
        return Err(Unread::NotAFile);
    }
    if stat.st_size > i64::from(max) {
        return Err(Unread::TooLarge { max });
    }
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8 * 1_024];
    loop {
        // SAFETY: the kernel writes only within the live exclusive buffer;
        // the descriptor is ours until the outer function closes it.
        let read = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
        if read < 0 {
            let errno = last_errno();
            if errno == libc::EINTR {
                continue;
            }
            return Err(Unread::Kernel(kernel_error(errno)));
        }
        if read == 0 {
            return Ok(bytes.into_boxed_slice());
        }
        let read = usize::try_from(read).expect("a positive read fits usize");
        let length = bytes.len().checked_add(read).ok_or(Unread::TooLarge { max })?;
        if u64::try_from(length).expect("a length fits u64") > u64::from(max) {
            return Err(Unread::TooLarge { max });
        }
        bytes.extend_from_slice(buffer.get(..read).expect("the kernel read within its buffer"));
    }
}

fn kernel_error(errno: i32) -> Error {
    match errno {
        libc::EACCES | libc::EPERM => Error::Permission,
        libc::ENOTDIR => Error::NotADirectory,
        libc::ENAMETOOLONG => Error::NameTooLong,
        libc::EMFILE | libc::ENFILE => Error::TooManyOpenFiles,
        libc::EINVAL => Error::InvalidArgument,
        libc::ENOBUFS | libc::ENOMEM => Error::NoBufferSpace,
        other => Error::Other(other),
    }
}
