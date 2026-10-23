//! Linux calls and decoders shared by its backends (shell.md, 7.5).
//!
//! # The `unsafe`, and why it is sound
//!
//! - **Socket addresses** are cast between `sockaddr_storage` and the
//!   `sockaddr_in` or `sockaddr_in6` its family names: the storage is larger
//!   and at least as aligned as either.
//! - **The synchronous calls** (`setsockopt`, `getsockname`, `close`,
//!   `open`, `flock`, `getdents64`, `lseek`, `fstatat`, `fstat`, `fcntl`,
//!   `faccessat2`, `clock_gettime`, `getrandom`) are given pointers to locals, to strings
//!   or to the
//!   table's listing buffer, each of which lives across the call, with their
//!   true sizes. `getdents64` is not a ring operation, so a `List` runs at
//!   its submit and completes at the next reap.
//! - Zeroed socket, stat and time structures contain only integer fields.
//!   Child-exit decoding reads the status union only after successful waitid.

use std::ffi::CString;
use std::io;
use std::mem::{self, size_of};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;

use skein_io::kernel::{
    Addr, Done, Entry, Error, Exit, Family, Fd, Kind, Op, OpenHow, PERMISSIONS, ServiceSignal, Stat,
};

use crate::calls::file_error;
use crate::startup::{DirectoryAccess, OutputKind};

const NANOS_PER_SEC: u64 = 1_000_000_000;

/// Finds an inherited output's kind at startup and enables `O_APPEND` on a
/// regular file. Ownership stays with the caller until io adopts the descriptor.
/// Pipes and terminals retain their flags (shell.md, section 6; io.md, 5.1).
pub fn prepare_output(descriptor: Fd) -> Result<OutputKind, Error> {
    // SAFETY: stat is initialized, and remains writable throughout fstat.
    let mut stat: libc::stat = unsafe { mem::zeroed() };
    // SAFETY: the caller owns descriptor, and stat is writable for this call.
    let result = unsafe { libc::fstat(descriptor.raw(), ptr::from_mut(&mut stat)) };
    if result < 0 {
        return Err(Error::Other(last_errno()));
    }
    if stat.st_mode & libc::S_IFMT != libc::S_IFREG {
        return Ok(OutputKind::Pipe);
    }
    let flags = status_flags(descriptor.raw()).map_err(Error::Other)?;
    if flags & libc::O_ACCMODE == libc::O_RDONLY {
        return Err(Error::Permission);
    }
    // SAFETY: F_SETFL takes only integer flags for the caller's live descriptor.
    let changed = unsafe { libc::fcntl(descriptor.raw(), libc::F_SETFL, flags | libc::O_APPEND) };
    if changed < 0 { Err(Error::Other(last_errno())) } else { Ok(OutputKind::Append) }
}

pub(crate) const STAT_MASK: u32 =
    libc::STATX_TYPE | libc::STATX_SIZE | libc::STATX_MODE | libc::STATX_UID | libc::STATX_NLINK;

/// Decodes the status union filled by a successful waitid.
pub(crate) fn child_exit(info: &libc::siginfo_t) -> Result<Exit, Error> {
    // SAFETY: callers pass a child-status record filled by successful waitid.
    let status = unsafe { info.si_status() };
    match info.si_code {
        libc::CLD_EXITED => Ok(Exit::Code(u8::try_from(status).expect("a child exit code fits in a byte"))),
        libc::CLD_KILLED | libc::CLD_DUMPED => Ok(Exit::Signal(u32::try_from(status).expect("a signal is positive"))),
        _ => Err(Error::Other(libc::ECHILD)),
    }
}

/// A signal read must contain exactly one signalfd record.
pub(crate) fn read_signal(info: &libc::signalfd_siginfo, count: usize) -> Result<Done, Error> {
    if count != size_of::<libc::signalfd_siginfo>() {
        return Err(Error::Other(libc::EIO));
    }
    let signal = match info.ssi_signo {
        value if value == u32::try_from(libc::SIGINT).expect("positive signal") => {
            skein_io::kernel::Signalled::Termination(ServiceSignal::Interrupt)
        }
        value if value == u32::try_from(libc::SIGTERM).expect("positive signal") => {
            skein_io::kernel::Signalled::Termination(ServiceSignal::Terminate)
        }
        value if value == u32::try_from(libc::SIGWINCH).expect("positive signal") => {
            skein_io::kernel::Signalled::WindowChanged
        }
        _ => return Err(Error::Other(libc::EINVAL)),
    };
    Ok(Done::Signalled(signal))
}

pub(crate) fn lock(fd: Fd) -> Result<(), i32> {
    crate::linux::retry(|| {
        // SAFETY: flock takes only the descriptor and integer flags, and never waits.
        let result = unsafe { libc::flock(fd.raw(), libc::LOCK_EX | libc::LOCK_NB) };
        if result == 0 { Ok(()) } else { Err(last_errno()) }
    })
}

/// `bytes` and a NUL after them: the string the kernel reads for a path or
/// a name, in which a valid record holds no NUL.
pub(crate) fn c_path(bytes: &[u8]) -> Box<[u8]> {
    let mut path = Vec::with_capacity(bytes.len().checked_add(1).expect("a path's length and one more fit a usize"));
    path.extend_from_slice(bytes);
    path.push(0);
    path.into_boxed_slice()
}

/// What an `Open` that the kernel answered with the descriptor `fd` comes
/// to: a file or a directory, or `NotAFile`, the descriptor closed. A file
/// blocks again, so that io_uring sends a read it cannot do at once to its
/// worker rather than answer `EAGAIN`.
pub(crate) fn opened(fd: i32) -> Result<Done, Error> {
    // SAFETY: `libc::stat` is a plain C struct of integers, for which all
    // zeroes is a valid value.
    let mut raw: libc::stat = unsafe { mem::zeroed() };
    // SAFETY: `raw` is a live, exclusive borrow of a `stat`, which the
    // kernel writes during the call only; `fd` was just opened for this
    // adapter.
    let stated = unsafe { libc::fstat(fd, ptr::from_mut(&mut raw)) };
    let checked = if stated != 0 {
        Err(Error::Other(last_errno()))
    } else {
        match kind_of(raw.st_mode) {
            Kind::File => blocking(fd),
            Kind::Directory => Ok(()),
            Kind::Symlink | Kind::Other => Err(Error::NotAFile),
        }
    };
    match checked {
        Ok(()) => Ok(Done::Fd(Fd::new(fd))),
        Err(error) => {
            close(fd);
            Err(error)
        }
    }
}

/// The status flags of what is open on `fd` (`F_GETFL`), or the error
/// number.
pub(crate) fn status_flags(fd: i32) -> Result<i32, i32> {
    // SAFETY: integers only.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 { Err(last_errno()) } else { Ok(flags) }
}

/// Clears `O_NONBLOCK` from the file open on `fd`.
fn blocking(fd: i32) -> Result<(), Error> {
    let flags = match status_flags(fd) {
        Ok(flags) => flags,
        Err(errno) => return Err(Error::Other(errno)),
    };
    // SAFETY: integers only.
    let set = unsafe { libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK) };
    if set < 0 { Err(Error::Other(last_errno())) } else { Ok(()) }
}

/// What a `Stat`'s `statx` found.
pub(crate) fn stat(raw: &libc::statx) -> Stat {
    let kind = if raw.stx_mask & libc::STATX_TYPE == 0 { Kind::Other } else { kind_of(u32::from(raw.stx_mode)) };
    let mode = if raw.stx_mask & libc::STATX_MODE == 0 { 0 } else { u32::from(raw.stx_mode) & PERMISSIONS };
    Stat { kind, size: raw.stx_size, mode, owner: raw.stx_uid, links: raw.stx_nlink }
}

/// The kind of a file whose mode is `mode`.
pub(crate) fn kind_of(mode: libc::mode_t) -> Kind {
    match mode & libc::S_IFMT {
        libc::S_IFREG => Kind::File,
        libc::S_IFDIR => Kind::Directory,
        libc::S_IFLNK => Kind::Symlink,
        _ => Kind::Other,
    }
}

/// The kind of a `getdents64` entry, or `None` when its filesystem did not
/// say (`DT_UNKNOWN`).
fn kind_of_entry(d_type: u8) -> Option<Kind> {
    match d_type {
        libc::DT_REG => Some(Kind::File),
        libc::DT_DIR => Some(Kind::Directory),
        libc::DT_LNK => Some(Kind::Symlink),
        libc::DT_UNKNOWN => None,
        _ => Some(Kind::Other),
    }
}

/// The offsets of a `linux_dirent64`'s fields: `d_off`, `d_reclen`,
/// `d_type`, and where `d_name` starts.
const D_OFF: usize = 8;

const D_RECLEN: usize = 16;

const D_TYPE: usize = 18;

const D_NAME: usize = 19;

/// A `List` (`skein_io::kernel`): `getdents64` into `listing`, the entries
/// copied out until `entries` or `names` is full, `.` and `..` skipped,
/// and the directory's position set back to just past the last entry taken,
/// so that the next `List` starts there. A filesystem that does not say an
/// entry's kind is asked with `fstatat`. Fails with the error number of a
/// call, or `ENAMETOOLONG` for a next name longer than all of `names`, when
/// no entry was taken.
pub(crate) fn list(fd: i32, entries: &mut [Entry], names: &mut [u8], listing: &mut [u8]) -> Result<u32, i32> {
    let mut count = 0_usize;
    let mut used = 0_usize;
    // Where the next List starts: past the last entry taken.
    let mut resume = seek(fd, 0, libc::SEEK_CUR)?;
    while count < entries.len() {
        let read = match getdents(fd, listing) {
            Ok(0) => break,
            Ok(read) => read,
            Err(errno) if count == 0 => return Err(errno),
            // Those taken are handed back; the next List meets the error.
            Err(_) => break,
        };
        let mut at = 0_usize;
        while at < read {
            let record = listing.get(at..read).expect("a record starts within what was read");
            let next = i64::from_ne_bytes(field(record, D_OFF));
            let len = usize::from(u16::from_ne_bytes(field(record, D_RECLEN)));
            let d_type = u8::from_ne_bytes(field(record, D_TYPE));
            let tail = record.get(D_NAME..len).expect("a record holds its name");
            let nul = tail.iter().position(|&byte| byte == 0).expect("a name ends in a NUL");
            let name = tail.get(..nul).expect("a name lies within its record");
            at = at.checked_add(len).expect("records lie within what was read");
            if name == b"." || name == b".." {
                resume = next;
                continue;
            }
            let end = used.checked_add(name.len()).expect("a name's end fits a usize");
            let (Some(entry), Some(room)) = (entries.get_mut(count), names.get_mut(used..end)) else {
                return match (count, seek(fd, resume, libc::SEEK_SET)) {
                    (0, Err(errno)) => Err(errno),
                    // A name longer than all of names: nothing can be taken.
                    (0, Ok(_)) => Err(libc::ENAMETOOLONG),
                    // Those taken are handed back, whether or not the
                    // position could be set back: they were read.
                    (_, Ok(_) | Err(_)) => Ok(u32::try_from(count).expect("no more entries than a valid List's count")),
                };
            };
            room.copy_from_slice(name);
            let kind = match kind_of_entry(d_type) {
                Some(kind) => kind,
                None => kind_at(fd, name),
            };
            let start = u32::try_from(used).expect("a valid List's names fit a count");
            let len = u32::try_from(name.len()).expect("a name's length fits a count");
            *entry = Entry { kind, start, len };
            count = count.checked_add(1).expect("no more entries than fit a usize");
            used = end;
            resume = next;
        }
    }
    Ok(u32::try_from(count).expect("no more entries than a valid List's count"))
}

/// The `N` bytes of a record from `at`.
fn field<const N: usize>(record: &[u8], at: usize) -> [u8; N] {
    let end = at.checked_add(N).expect("a field's end fits a usize");
    let bytes = record.get(at..end).expect("a record holds its fields");
    bytes.try_into().expect("N bytes")
}

/// The kind of the entry `name` of the directory open on `fd`, not
/// following it: `Other` if it is gone.
fn kind_at(fd: i32, name: &[u8]) -> Kind {
    let Ok(name) = CString::new(name) else {
        return Kind::Other;
    };
    // SAFETY: `libc::stat` is a plain C struct of integers, for which all
    // zeroes is a valid value.
    let mut raw: libc::stat = unsafe { mem::zeroed() };
    // SAFETY: `name` is a NUL-terminated string, and `raw` a live,
    // exclusive borrow of a `stat`, which the kernel writes during the call
    // only; both live across it.
    let done = unsafe { libc::fstatat(fd, name.as_ptr(), ptr::from_mut(&mut raw), libc::AT_SYMLINK_NOFOLLOW) };
    if done == 0 { kind_of(raw.st_mode) } else { Kind::Other }
}

pub(crate) fn domain(family: Family) -> i32 {
    match family {
        Family::Ipv4 => libc::AF_INET,
        Family::Ipv6 => libc::AF_INET6,
    }
}

pub(crate) fn socklen_of<T>() -> libc::socklen_t {
    libc::socklen_t::try_from(size_of::<T>()).expect("a socket structure's size fits a socklen_t")
}

pub(crate) fn errno_of(error: &io::Error) -> i32 {
    error.raw_os_error().unwrap_or(libc::EIO)
}

pub(crate) fn last_errno() -> i32 {
    errno_of(&io::Error::last_os_error())
}

fn family_of(family: i32) -> libc::sa_family_t {
    libc::sa_family_t::try_from(family).expect("an address family fits a sa_family_t")
}

pub(crate) fn zeroed_storage() -> libc::sockaddr_storage {
    // SAFETY: `sockaddr_storage` is a plain C struct of integers, for which
    // all zeroes is a valid value (an unspecified family).
    unsafe { mem::zeroed() }
}

pub(crate) fn zeroed_statx() -> libc::statx {
    // SAFETY: `statx` is a plain C struct of integers, for which all zeroes
    // is a valid value (nothing in its mask).
    unsafe { mem::zeroed() }
}

/// Writes `addr` into `storage` as the kernel lays it out, and returns its
/// length.
pub(crate) fn encode(addr: &Addr, storage: &mut libc::sockaddr_storage) -> libc::socklen_t {
    match addr {
        SocketAddr::V4(v4) => {
            let raw = libc::sockaddr_in {
                sin_family: family_of(libc::AF_INET),
                sin_port: v4.port().to_be(),
                // The octets are in network order, which is how s_addr is
                // laid out in memory.
                sin_addr: libc::in_addr { s_addr: u32::from_ne_bytes(v4.ip().octets()) },
                sin_zero: [0; 8],
            };
            // SAFETY: `sockaddr_storage` is larger than `sockaddr_in` and at
            // least as aligned, and `storage` is a live, exclusive borrow.
            unsafe { ptr::from_mut(storage).cast::<libc::sockaddr_in>().write(raw) };
            socklen_of::<libc::sockaddr_in>()
        }
        SocketAddr::V6(v6) => {
            let raw = libc::sockaddr_in6 {
                sin6_family: family_of(libc::AF_INET6),
                sin6_port: v6.port().to_be(),
                sin6_flowinfo: v6.flowinfo(),
                sin6_addr: libc::in6_addr { s6_addr: v6.ip().octets() },
                sin6_scope_id: v6.scope_id(),
            };
            // SAFETY: `sockaddr_storage` is larger than `sockaddr_in6` and at
            // least as aligned, and `storage` is a live, exclusive borrow.
            unsafe { ptr::from_mut(storage).cast::<libc::sockaddr_in6>().write(raw) };
            socklen_of::<libc::sockaddr_in6>()
        }
    }
}

/// The address the kernel wrote into `storage`, `len` bytes of it, or `None`
/// when it is neither IPv4 nor IPv6.
pub(crate) fn decode_addr(storage: &libc::sockaddr_storage, len: libc::socklen_t) -> Option<Addr> {
    let family = i32::from(storage.ss_family);
    if family == libc::AF_INET && len >= socklen_of::<libc::sockaddr_in>() {
        // SAFETY: the kernel wrote a `sockaddr_in`, as its family says, and
        // the storage is larger and at least as aligned.
        let raw = unsafe { ptr::from_ref(storage).cast::<libc::sockaddr_in>().read() };
        let ip = Ipv4Addr::from(raw.sin_addr.s_addr.to_ne_bytes());
        return Some(SocketAddr::V4(SocketAddrV4::new(ip, u16::from_be(raw.sin_port))));
    }
    if family == libc::AF_INET6 && len >= socklen_of::<libc::sockaddr_in6>() {
        // SAFETY: the kernel wrote a `sockaddr_in6`, as its family says, and
        // the storage is larger and at least as aligned.
        let raw = unsafe { ptr::from_ref(storage).cast::<libc::sockaddr_in6>().read() };
        let ip = Ipv6Addr::from(raw.sin6_addr.s6_addr);
        let port = u16::from_be(raw.sin6_port);
        return Some(SocketAddr::V6(SocketAddrV6::new(ip, port, raw.sin6_flowinfo, raw.sin6_scope_id)));
    }
    None
}

/// Sets the integer socket option `name` at `level` to 1.
pub(crate) fn set_option(fd: i32, level: i32, name: i32) -> Result<(), i32> {
    let one: libc::c_int = 1;
    let len = socklen_of::<libc::c_int>();
    // SAFETY: the value points at a live `c_int` of `len` bytes, which the
    // kernel reads during the call only.
    let set = unsafe { libc::setsockopt(fd, level, name, ptr::from_ref(&one).cast(), len) };
    if set == 0 { Ok(()) } else { Err(last_errno()) }
}

/// Reads the address `fd` is bound to into `storage`, and its length into
/// `len`.
pub(crate) fn local_addr(fd: i32, storage: &mut libc::sockaddr_storage, len: &mut libc::socklen_t) -> Result<(), i32> {
    *len = socklen_of::<libc::sockaddr_storage>();
    // SAFETY: `storage` is a live, exclusive borrow of `*len` bytes, and the
    // kernel writes at most that many, and the length, during the call only.
    let read = unsafe { libc::getsockname(fd, ptr::from_mut(storage).cast(), ptr::from_mut(len)) };
    if read == 0 { Ok(()) } else { Err(last_errno()) }
}

/// `getdents64` of the directory open on `fd` into `listing`: the bytes it
/// wrote, 0 at the end, or the error number.
fn getdents(fd: i32, listing: &mut [u8]) -> Result<usize, i32> {
    let len = libc::c_uint::try_from(listing.len()).expect("the listing buffer's length fits a c_uint");
    // SAFETY: `listing` is a live, exclusive borrow of `len` bytes, which the
    // kernel writes during the call only.
    let read = unsafe { libc::syscall(libc::SYS_getdents64, fd, listing.as_mut_ptr(), len) };
    match usize::try_from(read) {
        Ok(read) => Ok(read),
        Err(_) => Err(last_errno()),
    }
}

/// Moves the position of `fd`, as `lseek` does: where it is now.
fn seek(fd: i32, to: libc::off_t, whence: i32) -> Result<libc::off_t, i32> {
    // SAFETY: integers only.
    let at = unsafe { libc::lseek(fd, to, whence) };
    if at < 0 { Err(last_errno()) } else { Ok(at) }
}

/// Opens the directory at `path` as a root for io's files (kernel.md, 6.1),
/// as a service's `main` does at startup, from its configuration (shell.md,
/// 6): close-on-exec, and released only by a `Close` record. Fails with the
/// error number: `ENOTDIR` when `path` is not a directory, `EINVAL` when
/// it holds a NUL.
pub fn open_root(path: &Path) -> Result<Fd, i32> {
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return Err(libc::EINVAL);
    };
    // SAFETY: `path` is a NUL-terminated string that lives across the call.
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC) };
    if fd < 0 { Err(last_errno()) } else { Ok(Fd::new(fd)) }
}

/// Opens a regular file beneath `root` for append, creating it with `mode` if absent
/// (shell.md, 6). Existing contents and mode remain; links cannot escape the root.
pub fn open_append(root: Fd, path: &[u8], mode: u32) -> Result<Fd, Error> {
    if mode & !PERMISSIONS != 0 || path.is_empty() {
        return Err(Error::InvalidArgument);
    }
    let path_c = CString::new(path).map_err(|_| Error::InvalidArgument)?;
    let flags = libc::O_WRONLY | libc::O_APPEND | libc::O_CREAT | libc::O_CLOEXEC | libc::O_NONBLOCK | libc::O_NOCTTY;
    // The syscall layout is three u64 fields, as Linux's struct open_how.
    let how = [
        u64::try_from(flags).expect("positive flags"),
        u64::from(mode),
        libc::RESOLVE_BENEATH | libc::RESOLVE_NO_MAGICLINKS,
    ];
    // SAFETY: both pointers remain live for the call; the kernel reads the
    // NUL-terminated path and exactly these three initialized u64 fields.
    let fd = unsafe { libc::syscall(libc::SYS_openat2, root.raw(), path_c.as_ptr(), how.as_ptr(), size_of_val(&how)) };
    if fd < 0 {
        let kind = Op::Open { root, path: Box::from(path), how: OpenHow::Create { mode: Some(mode) } };
        return Err(file_error(&kind, last_errno()));
    }
    let Done::Fd(fd) = opened(i32::try_from(fd).expect("a descriptor fits i32"))? else {
        unreachable!("opened returns a descriptor");
    };
    Ok(fd)
}

/// Releases a descriptor the adapter made but cannot hand up. The descriptor
/// is released whatever `close` answers, so there is nothing to do with it.
pub(crate) fn close(fd: i32) {
    // SAFETY: `fd` is a descriptor the kernel just made for this adapter,
    // which nothing else has seen.
    let _closed: i32 = unsafe { libc::close(fd) };
}

/// Nanoseconds on clock `id`, or `None` before its origin.
pub(crate) fn clock_nanos(id: libc::clockid_t) -> Option<u64> {
    let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `time` is a live, exclusive borrow of a `timespec`, which the
    // kernel writes during the call only.
    let read = unsafe { libc::clock_gettime(id, ptr::from_mut(&mut time)) };
    assert!(read == 0, "the shell's clocks exist and the timespec is valid");
    let secs = u64::try_from(time.tv_sec).ok()?;
    let nanos = u64::try_from(time.tv_nsec).ok()?;
    secs.checked_mul(NANOS_PER_SEC)?.checked_add(nanos)
}

/// Fills startup's caller buffer from one `getrandom` call.
pub(crate) fn random(bytes: &mut [u8]) -> Result<(), i32> {
    // SAFETY: `bytes` is a live, exclusive borrow of its length, which the
    // kernel writes during the call only.
    let read = unsafe { libc::getrandom(bytes.as_mut_ptr().cast(), bytes.len(), 0) };
    match usize::try_from(read) {
        Ok(read) if read == bytes.len() => Ok(()),
        // A read of up to 256 bytes is never short once it starts.
        Ok(_) => Err(libc::EIO),
        Err(_) => Err(last_errno()),
    }
}

/// The process effective user, read once by startup for io's private files (shell.md, section 6).
#[must_use]
pub fn effective_user() -> u32 {
    // SAFETY: geteuid takes no pointers and has no preconditions.
    unsafe { libc::geteuid() }
}

/// Checks startup's directory with `faccessat2(W_OK | X_OK, AT_EACCESS)`;
/// creates nothing, opens nothing for writing and changes nothing (shell.md, 6.1).
/// The answer describes this moment; a later write must find out for itself.
pub fn directory_access(path: &Path) -> DirectoryAccess {
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return DirectoryAccess::Denied(libc::EINVAL);
    };
    // SAFETY: `libc::stat` is a plain C struct of integers, for which all
    // zeroes is a valid value.
    let mut raw: libc::stat = unsafe { mem::zeroed() };
    // SAFETY: the NUL-terminated path and the exclusive stat buffer live
    // across the call. Following links checks the directory the path names.
    let stat = unsafe { libc::fstatat(libc::AT_FDCWD, path.as_ptr(), ptr::from_mut(&mut raw), 0) };
    if stat == 0 {
        if raw.st_mode & libc::S_IFMT != libc::S_IFDIR {
            return DirectoryAccess::NotDirectory;
        }
        // SAFETY: the NUL-terminated path lives across the call; the other
        // arguments are the syscall's directory, access mode and flags.
        let access = unsafe {
            libc::syscall(
                libc::SYS_faccessat2,
                libc::AT_FDCWD,
                path.as_ptr(),
                libc::W_OK | libc::X_OK,
                libc::AT_EACCESS,
            )
        };
        if access == 0 {
            return DirectoryAccess::Writable;
        }
    }
    match last_errno() {
        libc::ENOENT => DirectoryAccess::Absent,
        libc::ENOTDIR => DirectoryAccess::NotDirectory,
        errno => DirectoryAccess::Denied(errno),
    }
}

/// The process ID, read once by startup for the service's report (shell.md, section 6.1).
#[must_use]
pub fn process_id() -> u32 {
    // SAFETY: getpid takes no pointers and has no preconditions.
    let process = unsafe { libc::getpid() };
    u32::try_from(process).expect("getpid returns a positive process ID")
}
