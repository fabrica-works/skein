//! The shell kit (shell.md): what a service's `main` runs its
//! loop on (programming-model.md, section 2).
//!
//! - [`Kernel`]: the ring or readiness backend of io's kernel records
//!   (`skein_io::kernel`; shell.md, 3), opened once, then submitted to
//!   and reaped from once per iteration.
//! - [`open_append`]: a regular file opened beneath a root for output at startup.
//! - [`open_root`]: a directory opened at startup as a root for io's files.
//! - [`effective_user`]: the startup user ID for io's private-file configuration.
//! - [`directory_access`]: the kernel's startup answer on writing and searching a directory.
//! - [`process_id`]: the startup process ID for the service's report.
//! - [`read_file`] and [`read_trust_roots`]: startup reads within configured bounds.
//! - [`resolve`] and [`help`]: a command's settings, their sources and help (shell.md, 6.5).
//! - [`open_termination_signals`]: the blocked SIGINT/SIGTERM source io adopts.
//! - [`Clock`]: monotonic and wall time, read together once per iteration.
//! - [`seed`]: the random seed, from `getrandom`, once at startup.
//!
//! [`Host`] adapts the service; [`drive`] runs it until settlement (shell.md, 12 and 13).
//!
//! This is ordinary Rust (programming-model.md, 10.2), and the only `unsafe`
//! in skein that a service runs lives here, in the ring adapter and Linux calls:
//! the crate's lints deny it everywhere else. (The counting allocator,
//! test-only, has the other: testing.md, 6.)

mod clock;
mod drive;
mod group;
mod kernel;
mod linux;
mod readiness;
mod ring;
mod seed;
mod settings;
mod startup;
mod terminal;
#[cfg(test)]
mod tests;

pub use clock::{Clock, Now};
pub use drive::{Host, drive};
pub use group::{child_id, group_live, signal_group};
pub use kernel::{Backend, Chosen, Kernel};
pub use ring::{
    Config, DirectoryAccess, HostedPipes, OpenError, OutputKind, Wait, abandon_binary, close_keeper_fd,
    directory_access, effective_user, hosted_pipes, make_root, make_subreaper, open_append, open_cgroup, open_pidfd,
    open_root, open_signal_pipe, open_termination_signals, pidfd_exited, poll_child, prepare_output, process_id,
    set_subreaper, signal_current_thread, signal_kept_child, start_binary, start_binary_in_cgroup, subreaper,
    wait_cgroup_change, write_service_signal,
};
pub use seed::{crypto_seed, seed};
pub use settings::{
    Base, Fallback, Origin, Reason, Refusal, Resolved, Setting, Settings, Value, arguments, environment, help, resolve,
};
pub use startup::{RootBounds, Roots, RootsRefusal, Trust, Unread, read_file, read_private_file, read_trust_roots};

pub use terminal::{Input, Kept, Keys, Modes, Watch, open_signals, raw_input, resize_terminal, terminal_modes};
