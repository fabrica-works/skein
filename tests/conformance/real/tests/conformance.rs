//! Files, processes and sockets against both backends on the real kernel,
//! on loopback and in scratch directories (kernel.md, 8): every scenario of
//! `skein_conformance` that loopback and a healthy scratch directory can
//! provoke, each process of a scenario a `Kernel` of its own. The same
//! scenarios run against the simulator in `tests/conformance/sim`.
//!
//! Not here: a failed `Accept` or `Open` past the descriptor limit, which
//! needs the process's limit lowered, and so `unsafe` outside the ring
//! adapter (programming-model.md, 2.1); they run on the simulator only.
//!
//! A machine without `io_uring` fails the ring module's tests, saying so;
//! a run as root fails the scenario on permissions, whom no mode stops.

#![cfg_attr(
    target_os = "linux",
    expect(
        clippy::duplicate_mod,
        reason = "one case list for files, processes and sockets runs in each backend's test namespace (shell.md, 9)"
    )
)]

use skein_conformance::Check;
use skein_conformance_real::Real;
use skein_shell::Backend;

fn on<S: Check>(backend: Backend, scenario: fn(&mut Real) -> S) {
    scenario(&mut Real::new(backend)).check();
}

// The conformance on the ring (shell.md, 9).
#[cfg(target_os = "linux")]
#[path = "cases"]
mod ring {
    const BACKEND: skein_shell::Backend = skein_shell::Backend::Ring;
    #[path = "files.rs"]
    mod files;
    #[path = "processes.rs"]
    mod processes;
    #[path = "sockets.rs"]
    mod sockets;
}

#[path = "cases"]
mod readiness {
    const BACKEND: skein_shell::Backend = skein_shell::Backend::Readiness;
    #[path = "files.rs"]
    mod files;
    #[path = "processes.rs"]
    mod processes;
    #[path = "sockets.rs"]
    mod sockets;
}
