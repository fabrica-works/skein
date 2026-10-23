//! File calls for the readiness backend (shell.md, 7.3).
//!
//! # The `unsafe`, and why it is sound
//!
//! - Openat2 reads a NUL-terminated local path and a zero-initialized `open_how`
//!   with the shared flags, mode and resolution fields. Neither pointer is kept.
//! - Pread/pwrite borrow the supplied slice, exclusively for reads, for its
//!   stated length until the call returns. Valid records' offsets fit `off_t`.
//! - Ftruncate and fsync take only integer descriptors and lengths.
//! - Statx reads a static empty path and writes one initialized local statx;
//!   its output and pointers never outlive the call.
//! - Renameat2, unlinkat and mkdirat borrow NUL-terminated local names during
//!   each call, with integer flags and descriptors. No pointer is retained.

use super::sync;
use super::{count, retry, status};
use skein_io::kernel::{Fd, OpenHow};
use std::{mem, ptr};

pub(crate) fn open(root: Fd, path: &[u8], how: OpenHow) -> Result<i32, i32> {
    let path = sync::c_path(path);
    let fields = super::opens::fields(how);
    // SAFETY: open_how is a plain integer C record, valid when zeroed.
    let mut how: libc::open_how = unsafe { mem::zeroed() };
    how.flags = fields.flags;
    how.mode = fields.mode;
    how.resolve = fields.resolve;
    retry(|| {
        // SAFETY: path is NUL-terminated; how and its exact size remain live.
        let result = unsafe {
            libc::syscall(
                libc::SYS_openat2,
                root.raw(),
                path.as_ptr(),
                ptr::from_ref(&how),
                size_of::<libc::open_how>(),
            )
        };
        if result < 0 { Err(sync::last_errno()) } else { Ok(i32::try_from(result).expect("a descriptor fits i32")) }
    })
}

pub(crate) fn read(fd: Fd, buf: &mut [u8], at: u64) -> Result<u32, i32> {
    let at = libc::off_t::try_from(at).expect("a valid Read's offset fits off_t");
    retry(|| {
        // SAFETY: buf is exclusively writable for its stated length during pread.
        count(unsafe { libc::pread(fd.raw(), buf.as_mut_ptr().cast(), buf.len(), at) })
    })
}

pub(crate) fn write(fd: Fd, bytes: &[u8], at: u64) -> Result<u32, i32> {
    let at = libc::off_t::try_from(at).expect("a valid Write's offset fits off_t");
    retry(|| {
        // SAFETY: bytes is readable for its stated length during pwrite.
        count(unsafe { libc::pwrite(fd.raw(), bytes.as_ptr().cast(), bytes.len(), at) })
    })
}

pub(crate) fn truncate(fd: Fd, length: u64) -> Result<(), i32> {
    let length = libc::off_t::try_from(length).expect("a valid Truncate's length fits off_t");
    retry(|| {
        // SAFETY: ftruncate takes only a descriptor and integer length.
        status(unsafe { libc::ftruncate(fd.raw(), length) })
    })
}

pub(crate) fn sync(fd: Fd) -> Result<(), i32> {
    retry(|| {
        // SAFETY: fsync takes only a descriptor.
        status(unsafe { libc::fsync(fd.raw()) })
    })
}

pub(crate) fn stat(fd: Fd) -> Result<skein_io::kernel::Stat, i32> {
    // SAFETY: statx is a plain integer C record, valid when zeroed.
    let mut raw: libc::statx = unsafe { mem::zeroed() };
    retry(|| {
        // SAFETY: the empty path is NUL-terminated, and raw is exclusively
        // writable for one statx until the call returns.
        status(unsafe {
            libc::statx(fd.raw(), c"".as_ptr(), libc::AT_EMPTY_PATH, sync::STAT_MASK, ptr::from_mut(&mut raw))
        })
    })?;
    Ok(sync::stat(&raw))
}

pub(crate) fn rename(from_dir: Fd, from: &[u8], to_dir: Fd, to: &[u8], no_replace: bool) -> Result<(), i32> {
    let from = sync::c_path(from);
    let to = sync::c_path(to);
    let flags = if no_replace { libc::RENAME_NOREPLACE } else { 0 };
    retry(|| {
        // SAFETY: both names are NUL-terminated locals borrowed only by renameat2.
        status(unsafe {
            libc::renameat2(from_dir.raw(), from.as_ptr().cast(), to_dir.raw(), to.as_ptr().cast(), flags)
        })
    })
}

pub(crate) fn remove(dir: Fd, name: &[u8], directory: bool) -> Result<(), i32> {
    let name = sync::c_path(name);
    let flags = if directory { libc::AT_REMOVEDIR } else { 0 };
    retry(|| {
        // SAFETY: name is NUL-terminated and remains live throughout unlinkat.
        status(unsafe { libc::unlinkat(dir.raw(), name.as_ptr().cast(), flags) })
    })
}

pub(crate) fn mkdir(dir: Fd, name: &[u8], mode: u32) -> Result<(), i32> {
    let name = sync::c_path(name);
    retry(|| {
        // SAFETY: name is NUL-terminated and remains live throughout mkdirat.
        status(unsafe { libc::mkdirat(dir.raw(), name.as_ptr().cast(), mode) })
    })
}
