//! Calls for the tree keeper (shell.md, 7.5).

use skein_io::kernel::{Error, Fd, Signal};

/// The process ID of the child a kept child handle names: its process group's
/// ID, as the binary leads a group of its own (shell.md, 7.5).
pub fn child_id(handle: Fd) -> Result<u32, Error> {
    crate::system::group::child_id(handle)
}

/// Sends `signal` to every member of process group `group` (shell.md, 7.5).
/// Groups 0 and 1 are refused: the kernel reads them as the caller's group
/// and every process.
pub fn signal_group(group: u32, signal: Signal) -> Result<(), Error> {
    crate::system::group::signal_group(group, signal)
}

/// Whether process group `group` still has a member (shell.md, 7.5): a
/// group ID is not reused while it has one, so after its leader's reap a
/// `false` means the whole group has ended.
pub fn group_live(group: u32) -> Result<bool, Error> {
    crate::system::group::group_live(group)
}
