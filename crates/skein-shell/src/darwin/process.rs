//! Darwin child creation, observation and harness calls (shell.md, 7.5).
//!
//! # The `unsafe`, and why it is sound
//!
//! Spawn actions, attributes, strings and pointer arrays are initialized and
//! live until `posix_spawn` returns; no borrowed memory is retained. Temporary
//! descriptors have one owner and transfer once on success. Signal sets,
//! usage and wait records are exclusive local C output storage. Kqueue changes
//! borrow local records; its integer handle is kept in the process-wide table.
//!
//! Terminal children fork in a process that may have other threads. Before
//! fork, the parent prepares all strings, pointer arrays, descriptor plans,
//! signal sets, the close bound and the error pipe. The child takes no locks,
//! allocates nothing and runs no destructors. It uses setsid, open, dup2,
//! fchdir, close, sigprocmask, signal, execve, write and _exit (POSIX's
//! async-signal-safe functions), plus Darwin's TIOCSCTTY ioctl on its own
//! freshly opened terminal and the thread-local errno slot. No Rust library
//! formatting, allocation or synchronization runs between fork and execve.

#![expect(
    clippy::disallowed_types,
    reason = "OwnedFd closes temporary spawn descriptors on every failure path inside the system boundary"
)]

use super::{kqueue, last_errno, retry, signals};
use skein_io::kernel::{Done, Error, Exit, Fd, Resources, Signal, Spawn, Target, Usage, Way};
use std::{
    collections::BTreeMap,
    ffi::{CStr, CString},
    mem::MaybeUninit,
    os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd},
    ptr,
    sync::Mutex,
};

// XNU's bsd/sys/ttycom.h value: _IO('t', 97); libc does not expose it.
const TIOCSCTTY: libc::c_ulong = 0x2000_7461;

#[derive(Clone, Copy)]
struct Child {
    pid: libc::pid_t,
    reaped: bool,
}
// Descriptors are process-wide. Hold the lock across wait/signal so a reap
// cannot race a signal of a recycled pid, even with separately hosted loops.
static CHILDREN: Mutex<BTreeMap<Fd, Child>> = Mutex::new(BTreeMap::new());

fn fd(raw: i32) -> Result<OwnedFd, i32> {
    if raw < 0 {
        Err(last_errno())
    } else {
        // SAFETY: this syscall just made `raw`, uniquely owned here.
        Ok(unsafe { OwnedFd::from_raw_fd(raw) })
    }
}

fn cstring(bytes: &[u8]) -> Result<CString, i32> {
    CString::new(bytes).map_err(|_| libc::EINVAL)
}

fn action(rc: i32) -> Result<(), i32> {
    if rc == 0 { Ok(()) } else { Err(rc) }
}

/// A pipe write after the child closes its read end returns EPIPE instead of
/// terminating the service with SIGPIPE. The kernel is tied to this thread.
pub(crate) fn block_sigpipe() -> Result<(), i32> {
    // SAFETY: sigset_t is a C signal set that sigemptyset initializes.
    let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
    // SAFETY: `set` is a valid writable signal set for both calls.
    action(unsafe { libc::sigemptyset(ptr::from_mut(&mut set)) })?;
    // SAFETY: `set` remains live and writable.
    action(unsafe { libc::sigaddset(ptr::from_mut(&mut set), libc::SIGPIPE) })?;
    // SAFETY: only this kernel thread's mask is changed; a child gets a
    // cleared mask through its spawn attributes below.
    action(unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, ptr::from_ref(&set), ptr::null_mut()) })
}

pub(crate) fn open_termination_signals() -> Result<Fd, i32> {
    signals::open_termination_signals()
}
pub(crate) fn open_signals(watch: crate::Watch) -> Result<Fd, i32> {
    signals::open_signals(watch)
}

fn directory(command: &Spawn) -> Result<OwnedFd, i32> {
    let name = if command.dir.is_empty() { b".".as_slice() } else { command.dir.as_ref() };
    fd(super::files::open_flags(
        command.root,
        name,
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | super::files::O_RESOLVE_BENEATH,
        0,
    )?)
}

fn pipe_pair() -> Result<(OwnedFd, OwnedFd), i32> {
    let mut pair = [0; 2];
    // SAFETY: pair is writable storage for two new descriptors.
    super::status(unsafe { libc::pipe(pair.as_mut_ptr()) })?;
    // SAFETY: pipe returned a newly owned read descriptor.
    let read = unsafe { OwnedFd::from_raw_fd(*pair.first().expect("two pipe ends")) };
    // SAFETY: pipe returned a separate newly owned write descriptor.
    let write = unsafe { OwnedFd::from_raw_fd(*pair.get(1).expect("two pipe ends")) };
    super::cloexec(Fd::new(read.as_raw_fd()))?;
    super::cloexec(Fd::new(write.as_raw_fd()))?;
    Ok((read, write))
}

type PreparedPipes = (OwnedFd, Vec<OwnedFd>, Vec<OwnedFd>, Vec<Option<OwnedFd>>);

fn prepare(command: &Spawn) -> Result<PreparedPipes, i32> {
    let dir = directory(command)?;
    let maximum = command.pipes.iter().map(|pipe| pipe.child).max().unwrap_or(2).max(2);
    let start = i32::try_from(maximum).map_err(|_| libc::EINVAL)?.checked_add(1).ok_or(libc::EINVAL)?;
    let mut parents = Vec::with_capacity(command.pipes.len());
    let mut children = Vec::with_capacity(command.pipes.len());
    for pipe in &command.pipes {
        let (read, write) = pipe_pair()?;
        let (parent, child) = match pipe.way {
            Way::In => (write, read),
            Way::Out => (read, write),
        };
        // Move the source above every child target: ordered dup2 actions can
        // then never overwrite the source of a later action.
        // SAFETY: `child` is open, and fcntl duplicates it with CLOEXEC.
        let high = fd(unsafe { libc::fcntl(child.as_raw_fd(), libc::F_DUPFD_CLOEXEC, start) })?;
        parents.push(parent);
        children.push(high);
    }
    let mut nulls = Vec::with_capacity(3);
    for standard in 0..3_u32 {
        if command.pipes.iter().any(|pipe| pipe.child == standard) {
            nulls.push(None);
            continue;
        }
        let flags = if standard == 0 { libc::O_RDONLY } else { libc::O_WRONLY };
        let name = c"/dev/null";
        // SAFETY: static NUL-terminated path.
        let null = fd(unsafe { libc::open(name.as_ptr(), flags | libc::O_CLOEXEC) })?;
        // Keep every source above every child target, including /dev/null.
        // SAFETY: `null` is open and fcntl makes a separate CLOEXEC source.
        let high = fd(unsafe { libc::fcntl(null.as_raw_fd(), libc::F_DUPFD_CLOEXEC, start) })?;
        nulls.push(Some(high));
    }
    Ok((dir, parents, children, nulls))
}

pub(crate) fn spawn(command: &mut Spawn) -> Result<Done, Error> {
    spawn_prepared(command, None).map_err(map_error)
}

fn spawn_prepared(command: &mut Spawn, terminal: Option<&CString>) -> Result<Done, i32> {
    let path = cstring(&command.program)?;
    let (dir, parents, children, nulls) = prepare(command)?;
    let argv = std::iter::once(&command.program)
        .chain(command.args.iter())
        .map(|v| cstring(v))
        .collect::<Result<Vec<_>, _>>()?;
    let env = command.env.iter().map(|v| cstring(v)).collect::<Result<Vec<_>, _>>()?;
    let mut argv_ptrs: Vec<*mut libc::c_char> = argv.iter().map(|v| v.as_ptr().cast_mut()).collect();
    argv_ptrs.push(ptr::null_mut());
    let mut env_ptrs: Vec<*mut libc::c_char> = env.iter().map(|v| v.as_ptr().cast_mut()).collect();
    env_ptrs.push(ptr::null_mut());
    if let Some(name) = terminal {
        let pid = fork_terminal(command, &path, name, &dir, &children, &nulls, &argv_ptrs, &env_ptrs)?;
        return finish_spawn(command, pid, parents);
    }
    let mut actions = MaybeUninit::<libc::posix_spawn_file_actions_t>::uninit();
    // SAFETY: initialization writes the action structure and returns 0 when valid.
    action(unsafe { libc::posix_spawn_file_actions_init(actions.as_mut_ptr()) })?;
    // SAFETY: initialized above; destroy after all additions and spawn.
    let mut actions = unsafe { actions.assume_init() };
    let result = (|| {
        // SAFETY: the actions pointer stays live through posix_spawn.
        action(unsafe { libc::posix_spawn_file_actions_addfchdir_np(ptr::from_mut(&mut actions), dir.as_raw_fd()) })?;
        for (pipe, child) in command.pipes.iter().zip(&children) {
            let to = i32::try_from(pipe.child).map_err(|_| libc::EINVAL)?;
            // SAFETY: both descriptors are valid, and actions is initialized.
            action(unsafe {
                libc::posix_spawn_file_actions_adddup2(ptr::from_mut(&mut actions), child.as_raw_fd(), to)
            })?;
        }
        for (to, maybe_null) in nulls.iter().enumerate() {
            if let Some(null) = maybe_null {
                // SAFETY: the /dev/null descriptor is valid.
                action(unsafe {
                    libc::posix_spawn_file_actions_adddup2(
                        ptr::from_mut(&mut actions),
                        null.as_raw_fd(),
                        i32::try_from(to).expect("0..3"),
                    )
                })?;
            }
        }
        let mut attributes = MaybeUninit::<libc::posix_spawnattr_t>::uninit();
        // SAFETY: init writes the structure, which is used only on success.
        action(unsafe { libc::posix_spawnattr_init(attributes.as_mut_ptr()) })?;
        // SAFETY: initialized above; destroyed after the spawn attempt.
        let mut attributes = unsafe { attributes.assume_init() };
        let spawned: Result<libc::pid_t, i32> = (|| {
            // SAFETY: sigemptyset initializes this plain C signal set.
            let mut empty: libc::sigset_t = unsafe { std::mem::zeroed() };
            // SAFETY: empty is writable and lives through the attribute copy.
            action(unsafe { libc::sigemptyset(ptr::from_mut(&mut empty)) })?;
            // SAFETY: the initialized attributes receive a copy of the mask.
            action(unsafe { libc::posix_spawnattr_setsigmask(ptr::from_mut(&mut attributes), ptr::from_ref(&empty)) })?;
            // SAFETY: defaults is initialized before being copied to the attributes.
            let mut defaults: libc::sigset_t = unsafe { std::mem::zeroed() };
            // SAFETY: defaults is live writable signal-set storage.
            super::status(unsafe { libc::sigemptyset(ptr::from_mut(&mut defaults)) })?;
            for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGWINCH, libc::SIGPIPE] {
                // SAFETY: defaults remains live initialized storage.
                super::status(unsafe { libc::sigaddset(ptr::from_mut(&mut defaults), signal) })?;
            }
            // SAFETY: initialized attributes copy the live default signal set.
            action(unsafe {
                libc::posix_spawnattr_setsigdefault(ptr::from_mut(&mut attributes), ptr::from_ref(&defaults))
            })?;
            // SAFETY: pgroup zero makes this child its own group leader.
            action(unsafe { libc::posix_spawnattr_setpgroup(ptr::from_mut(&mut attributes), 0) })?;
            // SAFETY: the initialized attributes accept the flags.
            action(unsafe {
                libc::posix_spawnattr_setflags(
                    ptr::from_mut(&mut attributes),
                    i16::try_from(
                        libc::POSIX_SPAWN_SETSIGMASK
                            | libc::POSIX_SPAWN_SETSIGDEF
                            | libc::POSIX_SPAWN_SETPGROUP
                            | libc::POSIX_SPAWN_CLOEXEC_DEFAULT,
                    )
                    .expect("spawn flag fits i16"),
                )
            })?;
            let mut pid = 0;
            // SAFETY: all arrays are NUL-terminated and stable for the call;
            // the initialized actions and attributes live through it.
            let rc = unsafe {
                libc::posix_spawn(
                    ptr::from_mut(&mut pid),
                    path.as_ptr(),
                    ptr::from_ref(&actions),
                    ptr::from_ref(&attributes),
                    argv_ptrs.as_ptr(),
                    env_ptrs.as_ptr(),
                )
            };
            action(rc)?;
            Ok(pid)
        })();
        // SAFETY: no spawn call can still read the attributes.
        let _destroyed = unsafe { libc::posix_spawnattr_destroy(ptr::from_mut(&mut attributes)) };
        finish_spawn(command, spawned?, parents)
    })();
    // SAFETY: the actions object was initialized and is no longer used.
    let _destroyed = unsafe { libc::posix_spawn_file_actions_destroy(ptr::from_mut(&mut actions)) };
    result
}

pub(super) fn map_error(errno: i32) -> Error {
    match errno {
        libc::ENOENT => Error::NotFound,
        libc::EACCES | libc::EPERM => Error::Permission,
        libc::ENOTDIR => Error::NotADirectory,
        libc::ELOOP => Error::TooManyLinks,
        libc::ENAMETOOLONG => Error::NameTooLong,
        libc::EXDEV => Error::Escape,
        libc::EMFILE | libc::ENFILE => Error::TooManyOpenFiles,
        libc::ENOMEM | libc::ENOBUFS => Error::NoBufferSpace,
        libc::EINVAL => Error::InvalidArgument,
        other => Error::Other(other),
    }
}

/// Reads the process and its reaped children at one submission (kernel.md, section 6.3).
pub(crate) fn usage() -> Result<Usage, Error> {
    Ok(Usage { own: resources(libc::RUSAGE_SELF)?, children: resources(libc::RUSAGE_CHILDREN)? })
}

fn resources(who: i32) -> Result<Resources, Error> {
    // SAFETY: rusage is a C output record initialized before its fields are read.
    let mut value: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: value is writable for the duration of this synchronous call.
    if unsafe { libc::getrusage(who, ptr::from_mut(&mut value)) } != 0 {
        return Err(Error::Other(last_errno()));
    }
    Ok(Resources {
        user: cpu(value.ru_utime)?,
        system: cpu(value.ru_stime)?,
        peak_rss_bytes: u64::try_from(value.ru_maxrss).map_err(|_| Error::Other(libc::EOVERFLOW))?,
    })
}

fn cpu(value: libc::timeval) -> Result<skein_lib::Duration, Error> {
    let seconds = u64::try_from(value.tv_sec).map_err(|_| Error::Other(libc::EOVERFLOW))?;
    let micros = u64::try_from(value.tv_usec).map_err(|_| Error::Other(libc::EOVERFLOW))?;
    let nanos = seconds
        .checked_mul(1_000_000_000)
        .and_then(|base| micros.checked_mul(1000).and_then(|tail| base.checked_add(tail)))
        .ok_or(Error::Other(libc::EOVERFLOW))?;
    Ok(skein_lib::Duration::from_nanos(nanos))
}

fn finish_spawn(command: &mut Spawn, pid: libc::pid_t, parents: Vec<OwnedFd>) -> Result<Done, i32> {
    let handle = match child_handle(pid) {
        Ok(handle) => handle,
        Err(errno) => {
            // SAFETY: this is our newly created, unreaped child's group.
            let _killed = unsafe { libc::killpg(pid, libc::SIGKILL) };
            reap_pid(pid);
            return Err(errno);
        }
    };
    for (pipe, parent) in command.pipes.iter_mut().zip(parents) {
        pipe.parent = Some(Fd::new(parent.into_raw_fd()));
    }
    Ok(Done::Spawned { pidfd: handle })
}

fn child_handle(pid: libc::pid_t) -> Result<Fd, i32> {
    let queue = kqueue::open()?;
    let change = kqueue::event(
        usize::try_from(pid).map_err(|_| libc::EINVAL)?,
        libc::EVFILT_PROC,
        libc::EV_ADD,
        libc::NOTE_EXIT,
    );
    match kqueue::change(queue.as_raw_fd(), change) {
        Ok(()) => {}
        Err(libc::ESRCH) => {
            // An already exited child is still ours and unreaped. Keep its
            // handle readable even though EVFILT_PROC can no longer attach.
            kqueue::change(queue.as_raw_fd(), kqueue::event(1, libc::EVFILT_USER, libc::EV_ADD, 0))?;
            kqueue::change(queue.as_raw_fd(), kqueue::event(1, libc::EVFILT_USER, 0, libc::NOTE_TRIGGER))?;
        }
        Err(errno) => return Err(errno),
    }
    let handle = Fd::new(queue.into_raw_fd());
    let prior = CHILDREN.lock().expect("child table is not poisoned").insert(handle, Child { pid, reaped: false });
    assert!(prior.is_none(), "a new descriptor has no child table entry");
    Ok(handle)
}

pub(crate) fn forget(handle: Fd) {
    let _removed = CHILDREN.lock().expect("child table is not poisoned").remove(&handle);
    signals::forget(handle);
}

pub(super) fn child_id(handle: Fd) -> Result<u32, Error> {
    let children = CHILDREN.lock().expect("child table is not poisoned");
    let child = children.get(&handle).ok_or(Error::Other(libc::EBADF))?;
    u32::try_from(child.pid).map_err(|_| Error::Other(libc::EOVERFLOW))
}

pub(crate) fn poll_child(handle: Fd, reap: bool) -> Result<Option<Exit>, Error> {
    let mut children = CHILDREN.lock().expect("child table is not poisoned");
    let child = children.get_mut(&handle).ok_or(Error::Other(libc::EBADF))?;
    if child.reaped {
        return Err(Error::Other(libc::ECHILD));
    }
    // SAFETY: siginfo_t is local C output storage, valid when zeroed.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    retry(|| {
        // SAFETY: pid names this table's unreaped child; info is exclusive
        // writable storage. Holding the lock excludes signal/reap races.
        super::status(unsafe {
            libc::waitid(
                libc::P_PID,
                u32::try_from(child.pid).expect("positive pid"),
                ptr::from_mut(&mut info),
                libc::WEXITED | libc::WNOHANG | if reap { 0 } else { libc::WNOWAIT },
            )
        })
    })
    .map_err(Error::Other)?;
    if info.si_pid == 0 {
        return Ok(None);
    }
    if reap {
        child.reaped = true;
    }
    // SAFETY: waitid filled the child-status union when si_pid is nonzero.
    let status = unsafe { info.si_status() };
    let exit = match info.si_code {
        libc::CLD_EXITED => Exit::Code(u8::try_from(status).expect("a child exit code fits in a byte")),
        libc::CLD_KILLED | libc::CLD_DUMPED => Exit::Signal(u32::try_from(status).expect("a signal is positive")),
        _ => return Err(Error::Other(libc::ECHILD)),
    };
    Ok(Some(exit))
}

pub(crate) fn signal_child(handle: Fd, signal: Signal, to: Target) -> Result<Done, Error> {
    let children = CHILDREN.lock().expect("child table is not poisoned");
    let child = children.get(&handle).ok_or(Error::Other(libc::EBADF))?;
    if child.reaped {
        return Err(Error::Other(libc::ESRCH));
    }
    let number = signal_number(signal);
    let rc = match to {
        // SAFETY: the lock keeps this pid unreaped throughout kill.
        Target::Child => unsafe { libc::kill(child.pid, number) },
        // SAFETY: this unreaped child leads its own group; killpg takes integers.
        Target::Group => unsafe { libc::killpg(child.pid, number) },
    };
    super::status(rc).map_err(map_error)?;
    Ok(Done::Nothing)
}

pub(super) fn signal_number(signal: Signal) -> i32 {
    match signal {
        Signal::Interrupt => libc::SIGINT,
        Signal::Terminate => libc::SIGTERM,
        Signal::Kill => libc::SIGKILL,
        Signal::Stop => libc::SIGSTOP,
        Signal::Continue => libc::SIGCONT,
    }
}

/// Descriptors made for a service hosted by a real-world harness.
#[derive(Debug)]
pub struct HostedPipes {
    pub pidfd: Fd,
    pub pipes: Vec<(u32, Fd)>,
    pub signal: Fd,
    pub signal_writer: Fd,
}

pub(crate) fn hosted_pipes(command: &mut Spawn) -> Result<HostedPipes, Error> {
    let (_dir, parents, children, _nulls) = prepare(command).map_err(map_error)?;
    let (signal, signal_writer) = signal_pipe()?;
    let placeholder = match kqueue::open() {
        Ok(queue) => queue,
        Err(errno) => {
            close_keeper_fd(signal);
            close_keeper_fd(signal_writer);
            return Err(map_error(errno));
        }
    };
    let pipes =
        command.pipes.iter().zip(children).map(|(pipe, child)| (pipe.child, Fd::new(child.into_raw_fd()))).collect();
    for (pipe, parent) in command.pipes.iter_mut().zip(parents) {
        pipe.parent = Some(Fd::new(parent.into_raw_fd()));
    }
    Ok(HostedPipes { pidfd: Fd::new(placeholder.into_raw_fd()), pipes, signal, signal_writer })
}

pub(crate) fn signal_pipe() -> Result<(Fd, Fd), Error> {
    let (reader, writer) = pipe_pair().map_err(map_error)?;
    super::nonblocking(Fd::new(writer.as_raw_fd())).map_err(map_error)?;
    let reader = Fd::new(reader.into_raw_fd());
    signals::remember_pipe(reader);
    Ok((reader, Fd::new(writer.into_raw_fd())))
}

pub(crate) fn write_signal(writer: Fd, signal: skein_io::kernel::ServiceSignal) -> Result<(), Error> {
    let number = match signal {
        skein_io::kernel::ServiceSignal::Interrupt => libc::SIGINT,
        skein_io::kernel::ServiceSignal::Terminate => libc::SIGTERM,
    };
    let bytes = number.to_ne_bytes();
    let count = super::write(writer, &bytes).map_err(map_error)?;
    if usize::try_from(count).expect("u32 fits usize") == bytes.len() { Ok(()) } else { Err(Error::Other(libc::EIO)) }
}

pub(crate) fn signal_current_thread(signal: skein_io::kernel::ServiceSignal) -> Result<(), Error> {
    let number = match signal {
        skein_io::kernel::ServiceSignal::Interrupt => libc::SIGINT,
        skein_io::kernel::ServiceSignal::Terminate => libc::SIGTERM,
    };
    // The signal goes to the process: XNU reports a signal to EVFILT_SIGNAL
    // only when it is sent to the process, never one sent to a thread
    // (bsd/kern/kern_sig.c, psignal_internal: `PSIG_THREAD` skips
    // `proc_knote`). Ignored, it then reaches no thread.
    let process = libc::pid_t::try_from(std::process::id()).expect("a process id fits pid_t");
    // SAFETY: kill names this process, whose caller has already installed
    // its signal kqueue and dispositions.
    action(unsafe { libc::kill(process, number) }).map_err(map_error)
}

pub(crate) fn close_keeper_fd(descriptor: Fd) {
    forget(descriptor);
    let _closed = super::close(descriptor);
}

pub(crate) fn abandon_binary(handle: Option<Fd>, descriptors: &[Fd]) {
    if let Some(handle) = handle {
        let mut children = CHILDREN.lock().expect("child table is not poisoned");
        if let Some(child) = children.get_mut(&handle)
            && !child.reaped
        {
            // SAFETY: this table entry still owns the unreaped child identity.
            let _killed = unsafe { libc::killpg(child.pid, libc::SIGKILL) };
            reap_pid(child.pid);
            child.reaped = true;
        }
    }
    for descriptor in descriptors {
        close_keeper_fd(*descriptor);
    }
    if let Some(handle) = handle {
        forget(handle);
    }
}

fn reap_pid(pid: libc::pid_t) {
    let _waited = retry(|| {
        // SAFETY: waitpid targets only this parent's child; status is not requested.
        let rc = unsafe { libc::waitpid(pid, ptr::null_mut(), 0) };
        if rc < 0 { Err(last_errno()) } else { Ok(()) }
    });
}

pub(crate) fn start_binary(command: &mut Spawn, terminal: bool, cgroup: Option<Fd>) -> Result<(Fd, Option<Fd>), Error> {
    if cgroup.is_some() {
        return Err(Error::InvalidArgument);
    }
    let terminal = if terminal { Some(open_terminal().map_err(map_error)?) } else { None };
    if terminal.is_some() && command.pipes.iter().any(|pipe| pipe.child < 2) {
        return Err(Error::InvalidArgument);
    }
    let done = spawn_prepared(command, terminal.as_ref().map(|(_, name)| name)).map_err(map_error)?;
    let Done::Spawned { pidfd } = done else { unreachable!("a spawn returns a child handle") };
    Ok((pidfd, terminal.map(|(master, _)| Fd::new(master.into_raw_fd()))))
}

fn open_terminal() -> Result<(OwnedFd, CString), i32> {
    // SAFETY: posix_openpt returns a fresh, uniquely owned terminal master.
    let master = fd(unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY) })?;
    super::cloexec(Fd::new(master.as_raw_fd()))?;
    // SAFETY: the live descriptor is a pseudo-terminal master.
    super::status(unsafe { libc::grantpt(master.as_raw_fd()) })?;
    // SAFETY: grantpt succeeded on the live master.
    super::status(unsafe { libc::unlockpt(master.as_raw_fd()) })?;
    // SAFETY: ptsname returns a terminated name on success, copied before
    // any other ptsname call; the shell serializes terminal creation.
    let name = unsafe { libc::ptsname(master.as_raw_fd()) };
    if name.is_null() {
        return Err(last_errno());
    }
    // SAFETY: successful ptsname returned a live NUL-terminated name.
    let name = unsafe { CStr::from_ptr(name) }.to_owned();
    Ok((master, name))
}

fn high_fd(descriptor: &OwnedFd, from: i32) -> Result<OwnedFd, i32> {
    // SAFETY: descriptor is live; fcntl creates a separate close-on-exec fd.
    fd(unsafe { libc::fcntl(descriptor.as_raw_fd(), libc::F_DUPFD_CLOEXEC, from) })
}

// All borrowed data and every descriptor in this plan exist before fork.
struct TerminalPlan<'a> {
    path: &'a CString,
    slave: &'a CString,
    argv: &'a [*mut libc::c_char],
    env: &'a [*mut libc::c_char],
    dup: &'a [(i32, i32)],
    cwd: i32,
    error: i32,
    from: i32,
    until: i32,
    empty: libc::sigset_t,
}

#[expect(clippy::too_many_arguments, reason = "all spawn storage is prepared before fork and borrowed through exec")]
fn fork_terminal(
    command: &Spawn,
    path: &CString,
    slave: &CString,
    dir: &OwnedFd,
    children: &[OwnedFd],
    nulls: &[Option<OwnedFd>],
    argv: &[*mut libc::c_char],
    env: &[*mut libc::c_char],
) -> Result<libc::pid_t, i32> {
    let maximum = command.pipes.iter().map(|pipe| pipe.child).max().unwrap_or(2).max(2);
    let from = i32::try_from(maximum).map_err(|_| libc::EINVAL)?.checked_add(1).ok_or(libc::EINVAL)?;
    let cwd = high_fd(dir, from)?;
    let mut dup = Vec::new();
    for (pipe, source) in command.pipes.iter().zip(children) {
        dup.push((source.as_raw_fd(), i32::try_from(pipe.child).map_err(|_| libc::EINVAL)?));
    }
    for (to, source) in nulls.iter().enumerate().skip(2) {
        if let Some(source) = source {
            dup.push((source.as_raw_fd(), i32::try_from(to).map_err(|_| libc::EINVAL)?));
        }
    }
    let (reader, writer) = pipe_pair()?;
    let high_writer = high_fd(&writer, from)?;
    drop(writer);
    let writer = high_writer;
    // SAFETY: rlimit is a C integer record initialized by getrlimit.
    let mut limit: libc::rlimit = unsafe { std::mem::zeroed() };
    // SAFETY: limit is exclusive writable output storage.
    super::status(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, ptr::from_mut(&mut limit)) })?;
    let until = i32::try_from(limit.rlim_cur.min(65_536)).map_err(|_| libc::EINVAL)?;
    // SAFETY: sigset_t is writable C storage initialized by sigemptyset.
    let mut empty: libc::sigset_t = unsafe { std::mem::zeroed() };
    // SAFETY: empty remains exclusive until this initialization returns.
    super::status(unsafe { libc::sigemptyset(ptr::from_mut(&mut empty)) })?;
    let plan = TerminalPlan {
        path,
        slave,
        argv,
        env,
        dup: &dup,
        cwd: cwd.as_raw_fd(),
        error: writer.as_raw_fd(),
        from,
        until,
        empty,
    };
    // SAFETY: the child uses only the prepared plan and async-signal-safe
    // calls, plus the terminal ioctl; it never returns into Rust destructors.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(last_errno());
    }
    if pid == 0 {
        terminal_child(&plan);
    }
    drop(writer);
    let mut bytes = [0; size_of::<i32>()];
    let mut used = 0;
    let result = loop {
        let room = bytes.get_mut(used..).expect("error record has remaining capacity");
        match super::read(Fd::new(reader.as_raw_fd()), room) {
            Ok(0) if used == 0 => break Ok(pid),
            Ok(0) => break Err(libc::EIO),
            Ok(count) => {
                used = used
                    .checked_add(usize::try_from(count).expect("u32 fits usize"))
                    .expect("read fits the remaining buffer");
                if used == bytes.len() {
                    break Err(i32::from_ne_bytes(bytes));
                }
            }
            Err(errno) => break Err(errno),
        }
    };
    if result.is_err() {
        // SAFETY: pid is the newly forked child, still ours until reap_pid.
        let _killed = unsafe { libc::kill(pid, libc::SIGKILL) };
        reap_pid(pid);
    }
    result
}

fn terminal_child(plan: &TerminalPlan<'_>) -> ! {
    // SAFETY: setsid uses no pointers and runs before any terminal open.
    if unsafe { libc::setsid() } < 0 {
        child_failed(plan.error);
    }
    // Close-on-exec, so that this descriptor, below the closing range when a
    // target is high, is not inherited: its duplicates as 0 and 1 are.
    // SAFETY: slave is prepared, terminated and live across the fork.
    let terminal = unsafe { libc::open(plan.slave.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
    if terminal < 0 {
        child_failed(plan.error);
    }
    // SAFETY: the child owns this terminal and is a new session leader.
    if unsafe { libc::ioctl(terminal, TIOCSCTTY, 0) } < 0 {
        child_failed(plan.error);
    }
    for to in [0, 1] {
        // SAFETY: terminal is live; these integer targets are stdin/stdout.
        if unsafe { libc::dup2(terminal, to) } < 0 {
            child_failed(plan.error);
        }
    }
    for &(source, to) in plan.dup {
        // SAFETY: prepared sources lie above every target and remain open.
        if unsafe { libc::dup2(source, to) } < 0 {
            child_failed(plan.error);
        }
    }
    // SAFETY: cwd was duplicated above all child targets before fork.
    if unsafe { libc::fchdir(plan.cwd) } < 0 {
        child_failed(plan.error);
    }
    for descriptor in plan.from..plan.until {
        if descriptor != plan.error {
            // SAFETY: close only takes an integer; EBADF for a hole is harmless.
            let _closed = unsafe { libc::close(descriptor) };
        }
    }
    // SAFETY: empty is a preinitialized empty signal mask, live through the call.
    if unsafe { libc::sigprocmask(libc::SIG_SETMASK, ptr::from_ref(&plan.empty), ptr::null_mut()) } < 0 {
        child_failed(plan.error);
    }
    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGWINCH, libc::SIGPIPE] {
        // SAFETY: SIG_DFL is a fixed disposition, no callback or allocation.
        if unsafe { libc::signal(signal, libc::SIG_DFL) } == libc::SIG_ERR {
            child_failed(plan.error);
        }
    }
    // SAFETY: all strings and terminated arrays were prepared before fork.
    let _failed = unsafe { libc::execve(plan.path.as_ptr(), plan.argv.as_ptr().cast(), plan.env.as_ptr().cast()) };
    child_failed(plan.error)
}

fn child_failed(writer: i32) -> ! {
    // SAFETY: __error is this thread's errno slot; read it before another call.
    let errno_slot = unsafe { libc::__error() };
    // SAFETY: errno_slot names the live thread-local errno integer.
    let errno = unsafe { *errno_slot };
    let bytes = errno.to_ne_bytes();
    // SAFETY: four initialized bytes are below PIPE_BUF and the descriptor
    // is the child's dedicated blocking error-pipe writer.
    while unsafe { libc::write(writer, bytes.as_ptr().cast(), bytes.len()) } < 0 {
        // SAFETY: errno is thread-local; retry only an interrupted write.
        if unsafe { *errno_slot } != libc::EINTR {
            break;
        }
    }
    // SAFETY: _exit terminates the child without destructors or libc cleanup.
    unsafe { libc::_exit(127) }
}
