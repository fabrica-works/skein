//! Durable startup directories (shell.md, section 6).
//!
//! # The `unsafe`, and why it is sound
//!
//! Directory calls borrow local NUL-terminated names until they return.
//! Their parent descriptors stay open across each call; fsync takes only
//! its descriptor. Each opened descriptor is handed up or closed once.

use super::{close_unchecked as close, last_errno, open_root};
use skein_io::kernel::{Fd, PERMISSIONS};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// Makes absent directories along `path` durable, then opens it as a root
/// (shell.md, 6). Existing directories keep their modes; errors are errno.
pub fn make_root(path: &Path, mode: u32) -> Result<Fd, i32> {
    if mode & !PERMISSIONS != 0 {
        return Err(libc::EINVAL);
    }
    if path.as_os_str().is_empty() {
        return Err(libc::ENOENT);
    }
    let mut absent = Vec::new();
    let mut parent = None;
    for ancestor in path.ancestors() {
        let ancestor = if ancestor.as_os_str().is_empty() { Path::new(".") } else { ancestor };
        match open_root(ancestor) {
            Ok(fd) => {
                parent = Some(fd);
                break;
            }
            Err(libc::ENOENT) => absent.push(ancestor),
            Err(error) => return Err(error),
        }
    }
    let mut parent = parent.ok_or(libc::ENOENT)?;
    for directory in absent.into_iter().rev() {
        let result = make_directory_root(parent, directory, mode);
        close(parent.raw());
        parent = result?;
    }
    Ok(parent)
}

fn make_directory_root(parent: Fd, path: &Path, mode: u32) -> Result<Fd, i32> {
    let component = path.components().next_back().ok_or(libc::EINVAL)?;
    let path_c = CString::new(component.as_os_str().as_bytes()).map_err(|_| libc::EINVAL)?;
    // SAFETY: parent is held open, the leaf name is terminated and live,
    // and mode holds permission bits.
    let created = unsafe { libc::mkdirat(parent.raw(), path_c.as_ptr(), mode) };
    if created < 0 {
        let error = last_errno();
        if error != libc::EEXIST {
            return Err(error);
        }
    } else {
        // SAFETY: parent is an open directory owned by this startup call.
        let synced = unsafe { libc::fsync(parent.raw()) };
        if synced < 0 {
            return Err(last_errno());
        }
    }
    // SAFETY: parent remains open and the terminated leaf name is live;
    // these are open_root's directory and close-on-exec flags.
    let fd =
        unsafe { libc::openat(parent.raw(), path_c.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
    if fd < 0 { Err(last_errno()) } else { Ok(Fd::new(fd)) }
}
