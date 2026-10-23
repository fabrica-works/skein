//! Darwin files and startup reads (shell.md, 7.5; kernel.md, 6.1).
//!
//! # The `unsafe`, and why it is sound
//!
//! Open, rename, unlink, mkdir and access borrow NUL-terminated local paths
//! only until return. Stat output is an initialized exclusive C record.
//! Pread/pwrite/read/write borrow slices of exactly their supplied lengths;
//! io validates offsets before submission. Fcntl, flock and truncate take
//! integers only. Every startup descriptor is closed before return or
//! transferred exactly once. Link refusal and beneath-root resolution are
//! enforced by openat, before the opened kind is inspected.

use super::{close, count, last_errno, retry, status, status_flags};
use crate::{
    calls::file_error,
    startup::{DirectoryAccess, OutputKind},
};
use skein_io::kernel::{Done, Error, Fd, Kind, Op, OpenHow, PERMISSIONS, Stat};
use std::{ffi::CString, mem, os::unix::ffi::OsStrExt, path::Path, ptr};

// XNU bsd/sys/fcntl.h's O_RESOLVE_BENEATH value; confirmed on the M4 runner.
pub(super) const O_RESOLVE_BENEATH: i32 = 0x0000_1000;
fn c_path(bytes: &[u8]) -> Result<CString, i32> {
    CString::new(bytes).map_err(|_| libc::EINVAL)
}
fn path_name(path: &Path) -> Result<CString, i32> {
    c_path(path.as_os_str().as_bytes())
}
fn escape(errno: i32) -> i32 {
    if errno == libc::ENOTCAPABLE { libc::EXDEV } else { errno }
}
pub(crate) fn open(root: Fd, path: &[u8], how: OpenHow) -> Result<i32, i32> {
    let (flags, mode) = match how {
        OpenHow::PathNoFollow => (libc::O_SYMLINK | libc::O_EVTONLY | libc::O_NOFOLLOW_ANY, 0),
        OpenHow::Read => (libc::O_RDONLY, 0),
        OpenHow::ReadNoFollow => (libc::O_RDONLY | libc::O_NOFOLLOW_ANY, 0),
        OpenHow::Write => (libc::O_RDWR, 0),
        OpenHow::WriteNoFollow => (libc::O_RDWR | libc::O_NOFOLLOW_ANY, 0),
        OpenHow::Directory => (libc::O_RDONLY | libc::O_DIRECTORY, 0),
        OpenHow::DirectoryNoFollow => (libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW_ANY, 0),
        OpenHow::Create { mode } => (libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL, mode.unwrap_or(0o666)),
        OpenHow::CreateNoFollow { mode } => {
            (libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW_ANY, mode.unwrap_or(0o666))
        }
    };
    // O_NONBLOCK on every open, a path-only one too: O_EVTONLY's access is
    // O_RDONLY's, so without it XNU's fifo_open sleeps until a FIFO has a
    // writer, where Linux's O_PATH opens the entry without opening the FIFO.
    let flags = flags | O_RESOLVE_BENEATH | libc::O_CLOEXEC | libc::O_NONBLOCK;
    let flags = if matches!(how, OpenHow::PathNoFollow) { flags } else { flags | libc::O_NOCTTY };
    open_flags(root, path, flags, mode)
}
pub(super) fn open_flags(root: Fd, path: &[u8], flags: i32, mode: u32) -> Result<i32, i32> {
    let path = c_path(path)?;
    let mode = libc::mode_t::try_from(mode).map_err(|_| libc::EINVAL)?;
    retry(|| {
        // SAFETY: path is terminated and live; mode is promoted to int for the variadic call.
        let raw = unsafe { libc::openat(root.raw(), path.as_ptr(), flags, i32::from(mode)) };
        if raw < 0 { Err(escape(last_errno())) } else { Ok(raw) }
    })
}
pub(crate) fn read(fd: Fd, buf: &mut [u8], at: u64) -> Result<u32, i32> {
    let at = libc::off_t::try_from(at).expect("io validates offsets");
    retry(|| {
        // SAFETY: buf is exclusive for its length; at is a valid file offset.
        count(unsafe { libc::pread(fd.raw(), buf.as_mut_ptr().cast(), buf.len(), at) })
    })
}
pub(crate) fn write(fd: Fd, bytes: &[u8], at: u64) -> Result<u32, i32> {
    let at = libc::off_t::try_from(at).expect("io validates offsets");
    retry(|| {
        // SAFETY: bytes is readable for its length and at is a valid offset.
        count(unsafe { libc::pwrite(fd.raw(), bytes.as_ptr().cast(), bytes.len(), at) })
    })
}
pub(crate) fn truncate(fd: Fd, length: u64) -> Result<(), i32> {
    let length = libc::off_t::try_from(length).expect("io validates lengths");
    retry(|| {
        // SAFETY: ftruncate takes only a descriptor and integer length.
        status(unsafe { libc::ftruncate(fd.raw(), length) })
    })
}
pub(crate) fn sync(fd: Fd) -> Result<(), i32> {
    retry(|| {
        // SAFETY: F_FULLFSYNC takes only the live descriptor and integer command.
        status(unsafe { libc::fcntl(fd.raw(), libc::F_FULLFSYNC) })
    })
}
fn raw_stat(fd: i32) -> Result<libc::stat, i32> {
    // SAFETY: stat contains only integers and timespecs, valid when zeroed.
    let mut raw: libc::stat = unsafe { mem::zeroed() };
    retry(|| {
        // SAFETY: raw is exclusive output storage alive throughout fstat.
        status(unsafe { libc::fstat(fd, ptr::from_mut(&mut raw)) })
    })?;
    Ok(raw)
}
pub(super) fn kind_of(mode: libc::mode_t) -> Kind {
    match mode & libc::S_IFMT {
        libc::S_IFREG => Kind::File,
        libc::S_IFDIR => Kind::Directory,
        libc::S_IFLNK => Kind::Symlink,
        _ => Kind::Other,
    }
}
pub(crate) fn stat(fd: Fd) -> Result<Stat, i32> {
    let raw = raw_stat(fd.raw())?;
    Ok(Stat {
        kind: kind_of(raw.st_mode),
        size: u64::try_from(raw.st_size).map_err(|_| libc::EOVERFLOW)?,
        mode: u32::from(raw.st_mode) & PERMISSIONS,
        owner: raw.st_uid,
        links: u32::from(raw.st_nlink),
    })
}
pub(crate) fn opened(raw: i32) -> Result<Done, Error> {
    let checked = raw_stat(raw).map_err(Error::Other).and_then(|stat| match kind_of(stat.st_mode) {
        Kind::File => {
            let flags = status_flags(raw).map_err(Error::Other)?;
            // SAFETY: F_SETFL takes integer flags on the just-opened file.
            status(unsafe { libc::fcntl(raw, libc::F_SETFL, flags & !libc::O_NONBLOCK) }).map_err(Error::Other)
        }
        Kind::Directory => Ok(()),
        Kind::Symlink | Kind::Other => Err(Error::NotAFile),
    });
    match checked {
        Ok(()) => Ok(Done::Fd(Fd::new(raw))),
        Err(error) => {
            let _closed = close(Fd::new(raw));
            Err(error)
        }
    }
}
pub(crate) fn rename(from_dir: Fd, from: &[u8], to_dir: Fd, to: &[u8], no_replace: bool) -> Result<(), i32> {
    let from = c_path(from)?;
    let to = c_path(to)?;
    retry(|| {
        if no_replace {
            // SAFETY: both terminated names remain live; RENAME_EXCL refuses replacement atomically.
            status(unsafe {
                libc::renameatx_np(from_dir.raw(), from.as_ptr(), to_dir.raw(), to.as_ptr(), libc::RENAME_EXCL)
            })
        } else {
            // SAFETY: both terminated names remain live throughout renameat.
            status(unsafe { libc::renameat(from_dir.raw(), from.as_ptr(), to_dir.raw(), to.as_ptr()) })
        }
    })
}
pub(crate) fn remove(dir: Fd, name: &[u8], directory: bool) -> Result<(), i32> {
    let name = c_path(name)?;
    let result = retry(|| {
        // SAFETY: name is terminated and lives across unlinkat.
        status(unsafe { libc::unlinkat(dir.raw(), name.as_ptr(), if directory { libc::AT_REMOVEDIR } else { 0 }) })
    });
    if result == Err(libc::EPERM) && !directory {
        // XNU bsd_vfs_vfs_subr.c:8292-8294 answers EPERM for unlink of a
        // directory. Inspect the entry itself on the same parent, so a link
        // to a directory never turns another EPERM into EISDIR.
        // SAFETY: stat is initialized C output storage, valid at zero.
        let mut raw: libc::stat = unsafe { mem::zeroed() };
        let found = retry(|| {
            // SAFETY: name and exclusive raw storage live throughout fstatat.
            status(unsafe {
                libc::fstatat(dir.raw(), name.as_ptr(), ptr::from_mut(&mut raw), libc::AT_SYMLINK_NOFOLLOW)
            })
        });
        if found.is_ok() && kind_of(raw.st_mode) == Kind::Directory {
            return Err(libc::EISDIR);
        }
    }
    result
}
pub(crate) fn mkdir(dir: Fd, name: &[u8], mode: u32) -> Result<(), i32> {
    let name = c_path(name)?;
    let mode = libc::mode_t::try_from(mode).map_err(|_| libc::EINVAL)?;
    retry(|| {
        // SAFETY: name remains live and terminated throughout mkdirat.
        status(unsafe { libc::mkdirat(dir.raw(), name.as_ptr(), mode) })
    })
}
pub(crate) fn lock(fd: Fd) -> Result<(), i32> {
    retry(|| {
        // SAFETY: flock takes integers and LOCK_NB prevents waiting.
        status(unsafe { libc::flock(fd.raw(), libc::LOCK_EX | libc::LOCK_NB) })
    })
}
pub fn open_root(path: &Path) -> Result<Fd, i32> {
    let path = path_name(path)?;
    // SAFETY: the terminated path lives across open.
    let raw = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
    if raw < 0 { Err(last_errno()) } else { Ok(Fd::new(raw)) }
}
pub fn open_append(root: Fd, path: &[u8], mode: u32) -> Result<Fd, Error> {
    if mode & !PERMISSIONS != 0 || path.is_empty() {
        return Err(Error::InvalidArgument);
    }
    let flags = libc::O_WRONLY
        | libc::O_APPEND
        | libc::O_CREAT
        | libc::O_CLOEXEC
        | libc::O_NONBLOCK
        | libc::O_NOCTTY
        | O_RESOLVE_BENEATH;
    let raw = open_flags(root, path, flags, mode).map_err(|errno| {
        file_error(&Op::Open { root, path: Box::from(path), how: OpenHow::Create { mode: Some(mode) } }, errno)
    })?;
    let Done::Fd(fd) = opened(raw)? else { unreachable!("opened returns a descriptor") };
    Ok(fd)
}
pub fn prepare_output(descriptor: Fd) -> Result<OutputKind, Error> {
    let raw = raw_stat(descriptor.raw()).map_err(Error::Other)?;
    if kind_of(raw.st_mode) != Kind::File {
        return Ok(OutputKind::Pipe);
    }
    let flags = status_flags(descriptor.raw()).map_err(Error::Other)?;
    if flags & libc::O_ACCMODE == libc::O_RDONLY {
        return Err(Error::Permission);
    }
    // SAFETY: the caller owns this live descriptor; F_SETFL takes integer flags only.
    status(unsafe { libc::fcntl(descriptor.raw(), libc::F_SETFL, flags | libc::O_APPEND) }).map_err(Error::Other)?;
    Ok(OutputKind::Append)
}
pub fn directory_access(path: &Path) -> DirectoryAccess {
    let Ok(path) = path_name(path) else {
        return DirectoryAccess::Denied(libc::EINVAL);
    };
    // SAFETY: stat is a C integer record valid at zero.
    let mut raw: libc::stat = unsafe { mem::zeroed() };
    // SAFETY: path is terminated and raw is exclusive local storage.
    if unsafe { libc::fstatat(libc::AT_FDCWD, path.as_ptr(), ptr::from_mut(&mut raw), 0) } == 0 {
        if kind_of(raw.st_mode) != Kind::Directory {
            return DirectoryAccess::NotDirectory;
        }
        // SAFETY: path remains live; access checks effective credentials without changing the file.
        if unsafe { libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::W_OK | libc::X_OK, libc::AT_EACCESS) } == 0 {
            return DirectoryAccess::Writable;
        }
    }
    match last_errno() {
        libc::ENOENT => DirectoryAccess::Absent,
        libc::ENOTDIR => DirectoryAccess::NotDirectory,
        errno => DirectoryAccess::Denied(errno),
    }
}
#[must_use]
pub fn effective_user() -> u32 {
    // SAFETY: geteuid takes no arguments.
    unsafe { libc::geteuid() }
}
#[must_use]
pub fn process_id() -> u32 {
    // SAFETY: getpid takes no arguments and returns a positive process ID.
    u32::try_from(unsafe { libc::getpid() }).expect("a positive pid")
}

use crate::startup::Unread;
use skein_io::file::Unsafe;
use skein_lib::Secret;
pub(crate) fn read_private_file(path: &Path, max: u32, user: u32) -> Result<Secret, Unread> {
    let path = CString::new(path.as_os_str().as_bytes()).map_err(|_| Unread::Kernel(Error::InvalidArgument))?;
    let flags = libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_CLOEXEC;
    let fd = match open_private(&path, flags) {
        Ok(fd) => fd,
        Err(libc::EACCES) => return Err(denied_private(&path, user)),
        Err(errno) => return Err(private_error(errno)),
    };
    let result = check_private(fd, user).and_then(|()| read_opened(fd, max)).map(Secret::new);
    let _closed = close(Fd::new(fd));
    result
}

fn open_private(path: &CString, flags: i32) -> Result<i32, i32> {
    // SAFETY: the terminated path lives across open; O_NOFOLLOW_ANY refuses every link.
    let fd = unsafe { libc::open(path.as_ptr(), flags | libc::O_NOFOLLOW_ANY) };
    if fd < 0 { Err(last_errno()) } else { Ok(fd) }
}

fn private_error(errno: i32) -> Unread {
    match errno {
        libc::ELOOP => Unread::Unsafe(Unsafe::Link),
        libc::ENOENT => Unread::Absent,
        other => Unread::Kernel(kernel_error(other)),
    }
}

fn denied_private(path: &CString, user: u32) -> Unread {
    let Ok(fd) = open_private(
        path,
        libc::O_SYMLINK | libc::O_EVTONLY | libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK | libc::O_CLOEXEC,
    ) else {
        return Unread::Kernel(Error::Permission);
    };
    let checked = check_private(fd, user);
    let _closed = close(Fd::new(fd));
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
    let mut stat: libc::stat = unsafe { mem::zeroed() };
    // SAFETY: stat remains exclusive and live across fstat.
    if unsafe { libc::fstat(fd, ptr::from_mut(&mut stat)) } < 0 {
        return Err(Unread::Kernel(kernel_error(last_errno())));
    }
    let found = if stat.st_uid != user {
        Some(Unsafe::Owner { found: stat.st_uid, expected: user })
    } else if kind_of(stat.st_mode) == Kind::Symlink {
        Some(Unsafe::Link)
    } else if kind_of(stat.st_mode) != Kind::File {
        Some(Unsafe::Kind(kind_of(stat.st_mode)))
    } else if stat.st_mode & 0o077 != 0 {
        Some(Unsafe::Mode(u32::from(stat.st_mode & 0o077)))
    } else if stat.st_nlink != 1 {
        Some(Unsafe::Links(u32::from(stat.st_nlink)))
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
    let _closed = close(Fd::new(fd));
    result
}

fn read_opened(fd: i32, max: u32) -> Result<Box<[u8]>, Unread> {
    // SAFETY: all-zero stat storage is valid; fstat initializes this live
    // exclusive local during the call and retains no pointer.
    let mut stat: libc::stat = unsafe { mem::zeroed() };
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
        let _closed = close(parent);
        parent = result?;
    }
    Ok(parent)
}

fn make_directory_root(parent: Fd, path: &Path, mode: u32) -> Result<Fd, i32> {
    let component = path.components().next_back().ok_or(libc::EINVAL)?;
    let path_c = CString::new(component.as_os_str().as_bytes()).map_err(|_| libc::EINVAL)?;
    // SAFETY: parent is held open, the leaf name is terminated and live,
    // and mode holds permission bits.
    let created = unsafe {
        libc::mkdirat(parent.raw(), path_c.as_ptr(), libc::mode_t::try_from(mode).expect("permission bits fit mode_t"))
    };
    if created < 0 {
        let error = last_errno();
        if error != libc::EEXIST {
            return Err(error);
        }
    } else {
        // SAFETY: parent is an open directory owned by this startup call.
        let synced = unsafe { libc::fcntl(parent.raw(), libc::F_FULLFSYNC) };
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
#[cfg(test)]
mod tests {
    use super::{close, open_root, path_name, remove, retry, status};
    use skein_scratch::Scratch;

    // A directory's unlink answers EPERM on Darwin, as does a refusal that is
    // not a directory's: an immutable file's (chflags), and so a sticky
    // directory's other users' entries, which one user cannot make.
    #[test]
    fn only_a_directory_turns_an_unlinks_eperm_into_eisdir() {
        let scratch = Scratch::new("darwin-remove");
        std::fs::create_dir(scratch.path().join("dir")).expect("make the directory");
        std::fs::write(scratch.path().join("fixed"), b"x").expect("make the file");
        let fixed = path_name(&scratch.path().join("fixed")).expect("a path");
        let change = |flags: u32| {
            // SAFETY: fixed is terminated and lives across chflags.
            retry(|| status(unsafe { libc::chflags(fixed.as_ptr(), flags) })).expect("chflags the file");
        };
        change(libc::UF_IMMUTABLE);
        let parent = open_root(scratch.path()).expect("open the scratch directory");
        assert_eq!(remove(parent, b"dir", false), Err(libc::EISDIR), "a directory's unlink is EISDIR");
        assert_eq!(remove(parent, b"fixed", false), Err(libc::EPERM), "an immutable file stays EPERM");
        change(0);
        assert_eq!(remove(parent, b"fixed", false), Ok(()), "the file goes once it may");
        assert_eq!(remove(parent, b"dir", true), Ok(()), "the directory goes as a directory");
        assert_eq!(close(parent), Ok(()), "close the scratch directory");
    }
}
