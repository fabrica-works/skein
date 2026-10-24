//! Startup and keeper process calls (shell.md, 7.5).

#[cfg(target_os = "linux")]
use std::path::Path;

use skein_io::kernel::{Error, Exit, Fd, ServiceSignal};

use crate::system;

/// Pipe descriptors for a spawned service hosted in the real loop (examples.md, section 6).
pub use crate::system::process::HostedPipes;

/// Executes a command with a parent-death signal, returning only on failure.
/// On Linux the signal survives exec of a non-setuid binary and is cleared
/// by a setuid/setgid exec. It fires when the parent's thread that forked
/// the child exits, rather than when the parent process exits. With io,
/// that thread runs the shell's kernel and lives as long as the loop.
/// `parent` must be the PID the caller's parent gave it, for example as an
/// argument, so the re-check after installing the signal detects a parent
/// that already died. macOS and other non-Linux systems return
/// [`std::io::ErrorKind::Unsupported`] without executing the command.
#[expect(clippy::disallowed_types, reason = "the child consumes Command only to exec, without a Child handle")]
#[must_use]
pub fn exec_dying_with_parent(
    command: std::process::Command,
    parent: u32,
    signal: skein_io::kernel::Signal,
) -> std::io::Error {
    #[cfg(target_os = "linux")]
    {
        system::process::exec_dying_with_parent(command, parent, signal)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _unused = (command, parent, signal);
        std::io::Error::new(std::io::ErrorKind::Unsupported, "parent-death exec requires Linux")
    }
}

#[cfg(all(test, not(target_os = "linux")))]
#[test]
#[expect(clippy::disallowed_types, reason = "the refused command must never be executed")]
fn parent_death_exec_is_unsupported() {
    assert_eq!(
        exec_dying_with_parent(std::process::Command::new("/no-such-program"), 0, skein_io::kernel::Signal::Kill)
            .kind(),
        std::io::ErrorKind::Unsupported
    );
}

/// Opens the service's termination signal source for io
/// to adopt before the loop starts (shell.md, section 6; io.md, section 7).
pub fn open_termination_signals() -> Result<Fd, i32> {
    system::process::open_termination_signals()
}

/// Makes a hosted spawn's real pipes and identity placeholder, without exec.
pub fn hosted_pipes(command: &mut skein_io::kernel::Spawn) -> Result<HostedPipes, Error> {
    system::process::hosted_pipes(command)
}

/// Starts a shipped binary on pipes or a controlling pseudo-terminal for
/// an end-to-end world; stderr is always separate (examples.md, section 6).
pub fn start_binary(command: &mut skein_io::kernel::Spawn, terminal: bool) -> Result<(Fd, Option<Fd>), Error> {
    system::process::start_binary(command, terminal, None)
}

/// Polls a child handle during observer cleanup, observing or reaping only that child.
pub fn poll_child(pidfd: Fd, reap: bool) -> Result<Option<Exit>, Error> {
    system::process::poll_child(pidfd, reap)
}

/// Sends a keeper's signal through its owned child handle, including its group.
pub fn signal_kept_child(
    pidfd: Fd,
    signal: skein_io::kernel::Signal,
    to: skein_io::kernel::Target,
) -> Result<(), Error> {
    system::process::signal_child(pidfd, signal, to).map(|_| ())
}

/// Closes an exclusively owned test keeper descriptor during cleanup.
pub fn close_keeper_fd(descriptor: Fd) {
    system::process::close_keeper_fd(descriptor);
}

/// Reaps and releases an external child abandoned by a failing test. Normal
/// end-to-end settlement uses the kernel (testing-strategy.md, section 2.9).
/// Every descriptor must be quiescent: an observer with outstanding records
/// cancels and drains them before closure, or leaves their descriptors open
/// in its emergency destructor (kernel.md, section 7).
pub fn abandon_binary(pidfd: Option<Fd>, descriptors: &[Fd]) {
    system::process::abandon_binary(pidfd, descriptors);
}

/// Sends a signal record to a hosted service's signal pipe (examples.md, section 6).
pub fn write_service_signal(writer: Fd, signal: ServiceSignal) -> Result<(), Error> {
    system::process::write_signal(writer, signal)
}

/// Opens a signal-record pipe, returning its reader and writer (examples.md, section 6).
pub fn open_signal_pipe() -> Result<(Fd, Fd), Error> {
    system::process::signal_pipe()
}

/// Sends to this thread's termination signal source (io.md, section 7).
/// Call only after `open_termination_signals` on the same thread. On macOS
/// it signals the process, which every signal kqueue in it hears (shell.md, 7.5).
pub fn signal_current_thread(signal: ServiceSignal) -> Result<(), Error> {
    system::process::signal_current_thread(signal)
}

// Linux's own (shell.md, 7.5)

/// Starts the binary in an already-open delegated cgroup, before its exec.
#[cfg(target_os = "linux")]
pub fn start_binary_in_cgroup(
    command: &mut skein_io::kernel::Spawn,
    terminal: bool,
    cgroup: Fd,
) -> Result<(Fd, Option<Fd>), Error> {
    system::process::start_binary(command, terminal, Some(cgroup))
}

/// Makes the test process a subreaper for its binary's descendants.
#[cfg(target_os = "linux")]
pub fn make_subreaper() -> Result<(), Error> {
    system::process::make_subreaper()
}

/// Reads whether this test process adopts orphaned descendants.
#[cfg(target_os = "linux")]
pub fn subreaper() -> Result<bool, Error> {
    system::process::subreaper()
}

/// Restores or enables this test process's orphan adoption setting.
#[cfg(target_os = "linux")]
pub fn set_subreaper(enabled: bool) -> Result<(), Error> {
    system::process::set_subreaper(enabled)
}

/// Opens a discovered process's pidfd for the test keeper.
#[cfg(target_os = "linux")]
pub fn open_pidfd(pid: u32) -> Result<Fd, Error> {
    system::process::open_pidfd(pid)
}

/// Opens a cgroup directory for the binary's atomic placement at spawn.
#[cfg(target_os = "linux")]
pub fn open_cgroup(path: &Path) -> Result<Fd, Error> {
    system::process::open_cgroup(path)
}

/// Tests a keeper's pidfd for exit, including descendants not yet adopted.
#[cfg(target_os = "linux")]
pub fn pidfd_exited(pidfd: Fd) -> Result<bool, Error> {
    system::process::pidfd_exited(pidfd)
}

/// Waits for a keeper's cgroup event file to change, within the given milliseconds.
#[cfg(target_os = "linux")]
pub fn wait_cgroup_change(descriptor: Fd, millis: u32) -> Result<(), Error> {
    system::process::wait_cgroup_change(descriptor, millis)
}
