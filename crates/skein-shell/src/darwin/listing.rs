//! Directory-stream positions owned by the readiness backend (shell.md, 7.5).
//!
//! # The `unsafe`, and why it is sound
//!
//! `F_DUPFD_CLOEXEC` duplicates the caller's live descriptor. Fdopendir takes
//! that duplicate only on success; each stream is closed exactly once by
//! forget or drop. Readdir's borrowed entry is copied before the next call;
//! `d_namlen` bounds its name within `d_name`. Telldir and seekdir use the same
//! stream and a position obtained before that read. Fstatat borrows a local
//! terminated name and exclusive stat output until return. Errno is cleared
//! through this thread's __error pointer to distinguish end from failure.

use super::{files::kind_of, last_errno};
use skein_io::kernel::{Entry, Fd, Kind};
use std::{
    collections::BTreeMap,
    ffi::CString,
    mem,
    ptr::{self, NonNull},
};

#[derive(Debug)]
struct Directory(NonNull<libc::DIR>);
impl Drop for Directory {
    fn drop(&mut self) {
        // SAFETY: this stream owns its duplicate and has not been closed.
        let _closed = unsafe { libc::closedir(self.0.as_ptr()) };
    }
}
impl Directory {
    fn open(fd: Fd) -> Result<Self, i32> {
        // SAFETY: fcntl duplicates the live descriptor with close-on-exec.
        let duplicate = unsafe { libc::fcntl(fd.raw(), libc::F_DUPFD_CLOEXEC, 0) };
        if duplicate < 0 {
            return Err(last_errno());
        }
        // SAFETY: fdopendir takes the fresh directory descriptor on success.
        let stream = unsafe { libc::fdopendir(duplicate) };
        if let Some(stream) = NonNull::new(stream) {
            Ok(Self(stream))
        } else {
            let errno = last_errno();
            let _closed = super::close(Fd::new(duplicate));
            Err(errno)
        }
    }
    fn list(&mut self, fd: Fd, entries: &mut [Entry], names: &mut [u8]) -> Result<u32, i32> {
        let mut count = 0_usize;
        let mut used = 0_usize;
        while count < entries.len() {
            // SAFETY: __error returns this thread's writable errno slot.
            let errno_slot = unsafe { libc::__error() };
            // SAFETY: the returned thread-local errno slot is writable.
            unsafe { errno_slot.write(0) };
            // SAFETY: the stream is owned and open; positions stay with this stream.
            let before = unsafe { libc::telldir(self.0.as_ptr()) };
            if before < 0 {
                return if count == 0 {
                    Err(last_errno())
                } else {
                    Ok(u32::try_from(count).expect("valid List count"))
                };
            }
            // SAFETY: __error returns this thread's writable errno slot.
            let errno_slot = unsafe { libc::__error() };
            // SAFETY: the returned thread-local errno slot is writable.
            unsafe { errno_slot.write(0) };
            // SAFETY: the exclusively borrowed stream is live; the entry is used before the next read.
            let entry = unsafe { libc::readdir(self.0.as_ptr()) };
            if entry.is_null() {
                let errno = last_errno();
                if errno != 0 && count == 0 {
                    return Err(errno);
                }
                break;
            }
            // SAFETY: successful readdir returns a live dirent until the next stream operation.
            let raw = unsafe { &*entry };
            let length = usize::from(raw.d_namlen);
            let bytes = raw.d_name.get(..length).ok_or(libc::EIO)?;
            let name: Vec<u8> = bytes.iter().map(|byte| u8::from_ne_bytes(byte.to_ne_bytes())).collect();
            if name == b"." || name == b".." {
                continue;
            }
            let end = used.checked_add(length).ok_or(libc::EOVERFLOW)?;
            let Some(room) = names.get_mut(used..end) else {
                // SAFETY: before was obtained from this same still-open stream.
                unsafe { libc::seekdir(self.0.as_ptr(), before) };
                if count == 0 {
                    return Err(libc::ENAMETOOLONG);
                }
                break;
            };
            room.copy_from_slice(&name);
            let kind = match raw.d_type {
                libc::DT_REG => Kind::File,
                libc::DT_DIR => Kind::Directory,
                libc::DT_LNK => Kind::Symlink,
                libc::DT_UNKNOWN => kind_at(fd, &name),
                _ => Kind::Other,
            };
            *entries.get_mut(count).expect("count is within entries") = Entry {
                kind,
                start: u32::try_from(used).expect("valid List names"),
                len: u32::try_from(length).expect("dirent length fits u32"),
            };
            count = count.checked_add(1).expect("bounded by entries");
            used = end;
        }
        Ok(u32::try_from(count).expect("valid List count"))
    }
}
fn kind_at(fd: Fd, name: &[u8]) -> Kind {
    let Ok(name) = CString::new(name) else {
        return Kind::Other;
    };
    // SAFETY: stat is an integer C output record, valid at zero.
    let mut raw: libc::stat = unsafe { mem::zeroed() };
    // SAFETY: the name and exclusive stat storage live until return.
    let rc = unsafe { libc::fstatat(fd.raw(), name.as_ptr(), ptr::from_mut(&mut raw), libc::AT_SYMLINK_NOFOLLOW) };
    if rc == 0 { kind_of(raw.st_mode) } else { Kind::Other }
}
#[derive(Debug)]
pub(crate) struct Listings(BTreeMap<Fd, Directory>);
impl Listings {
    pub(crate) fn new() -> Self {
        Self(BTreeMap::new())
    }
    pub(crate) fn list(&mut self, fd: Fd, entries: &mut [Entry], names: &mut [u8]) -> Result<u32, i32> {
        if let std::collections::btree_map::Entry::Vacant(entry) = self.0.entry(fd) {
            entry.insert(Directory::open(fd)?);
        }
        self.0.get_mut(&fd).expect("the directory stream is open").list(fd, entries, names)
    }
    pub(crate) fn forget(&mut self, fd: Fd) {
        let _removed = self.0.remove(&fd);
    }
}
