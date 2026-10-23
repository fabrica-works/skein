//! The public kernel and its startup choice (shell.md, 7.1).

use std::{fmt, io};

use skein_io::kernel::{Complete, Submit};
use skein_lib::{Queue, Time};

#[cfg(target_os = "linux")]
use crate::linux::ring;
use crate::{readiness, system};

/// How large a [`crate::Kernel`] is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Config {
    /// The most operations in flight at once, cancels included, as io's
    /// operation slab counts them (kernel.md, 5). Both rings are
    /// sized from it, so the completion queue cannot overflow, and
    /// [`crate::Kernel::submit`] takes no record past it.
    pub operations: u32,
}

/// Whether [`crate::Kernel::submit`] blocks for a completion, and until when.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wait {
    /// Submit and return: work is pending above.
    No,
    /// Block until a completion arrives or the deadline passes, a monotonic
    /// [`Time`] read from the [`Clock`](crate::Clock): the earliest deadline
    /// over every layer (programming-model.md, section 9).
    Until(Time),
    /// Block until a completion arrives: no deadline is armed. Asserts that
    /// something is in flight or ready, since otherwise it never returns.
    Forever,
}

/// Why [`crate::Kernel::open`] refused: the shell does not start (shell.md, 4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpenError {
    /// A config of no operations, which could never submit one.
    NoOperations,
    /// The ring could not be set up, with the error number: io_uring is
    /// missing, disabled (`io_uring_disabled`) or refused by a seccomp
    /// profile (`ENOSYS`, `EPERM`), or `EINVAL`: the config is past the
    /// ring's sizes, or the kernel is below the floor and does not know
    /// `SINGLE_ISSUER`, `DEFER_TASKRUN` or `SUBMIT_ALL`.
    Setup(i32),
    /// The kernel is below the floor, 6.12: its ring lacks this operation.
    Missing(&'static str),
    /// The kernel's release is below this system's floor (shell.md, 4 and 7.6).
    BelowFloor,
    /// This system has no completion ring.
    Unsupported,
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::NoOperations => write!(f, "the kernel is configured for no operations"),
            OpenError::Setup(errno) => {
                write!(f, "io_uring could not be set up: {}", io::Error::from_raw_os_error(*errno))
            }
            OpenError::Missing(name) => write!(f, "the kernel is below 6.12: io_uring has no {name}"),
            OpenError::BelowFloor => {
                write!(f, "the kernel is below the floor, {}.{}", system::FLOOR.0, system::FLOOR.1)
            }
            OpenError::Unsupported => f.write_str("the ring is not available on this system"),
        }
    }
}

impl std::error::Error for OpenError {}

/// The implementation to run, forced by tests at startup.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// Linux's completion ring.
    Ring,
    /// Non-blocking calls and descriptor readiness.
    Readiness,
}

/// What runs, and when the ring was refused, its error number.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Chosen {
    /// The running implementation.
    pub backend: Backend,
    /// The ring's setup refusal, when startup fell back to readiness.
    pub ring_refused: Option<i32>,
}

impl fmt::Display for Chosen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.backend {
            Backend::Ring => f.write_str("io_uring"),
            Backend::Readiness => {
                #[cfg(target_os = "linux")]
                f.write_str("readiness (epoll)")?;
                #[cfg(target_os = "macos")]
                f.write_str("readiness (kqueue)")?;
                if let Some(errno) = self.ring_refused {
                    f.write_str(": io_uring refused (")?;
                    match errno {
                        libc::EPERM => f.write_str("EPERM")?,
                        libc::ENOSYS => f.write_str("ENOSYS")?,
                        other => write!(f, "{other}")?,
                    }
                    f.write_str(")")?;
                }
                Ok(())
            }
        }
    }
}

/// io's kernel, opened once on one thread (shell.md, 3 and 7).
#[derive(Debug)]
pub struct Kernel {
    running: Running,
    ring_refused: Option<i32>,
}

#[derive(Debug)]
#[cfg_attr(
    target_os = "linux",
    expect(
        clippy::large_enum_variant,
        reason = "the startup choice holds the ring inline so its path adds no allocation or indirection (shell.md, 7.1)"
    )
)]
enum Running {
    #[cfg(target_os = "linux")]
    Ring(ring::Ring),
    Readiness(readiness::Readiness),
}

/// The ring's refusal that falls back to readiness (shell.md, 7.1):
/// `Setup(ENOSYS)` or `Setup(EPERM)`, its errno; anything else, None.
#[cfg(any(target_os = "linux", test))]
fn falls_back(refused: &OpenError) -> Option<i32> {
    match refused {
        OpenError::Setup(errno) => match *errno {
            libc::ENOSYS | libc::EPERM => Some(*errno),
            _ => None,
        },
        OpenError::Missing(_) | OpenError::BelowFloor | OpenError::NoOperations | OpenError::Unsupported => None,
    }
}

impl Kernel {
    /// The host's backend (shell.md, 7.1).
    #[cfg(target_os = "linux")]
    pub fn open(config: Config) -> Result<Self, OpenError> {
        match ring::Ring::open(config) {
            Ok(ring) => Ok(Self { running: Running::Ring(ring), ring_refused: None }),
            Err(refused) => match falls_back(&refused) {
                Some(errno) => Ok(Self {
                    running: Running::Readiness(readiness::Readiness::open(config)?),
                    ring_refused: Some(errno),
                }),
                None => Err(refused),
            },
        }
    }

    /// The host's readiness backend (shell.md, 7.1).
    #[cfg(target_os = "macos")]
    pub fn open(config: Config) -> Result<Self, OpenError> {
        Self::open_on(config, Backend::Readiness)
    }

    /// Forces a backend, so its conformance cannot silently use another.
    pub fn open_on(config: Config, backend: Backend) -> Result<Self, OpenError> {
        let running = match backend {
            #[cfg(target_os = "linux")]
            Backend::Ring => Running::Ring(ring::Ring::open(config)?),
            #[cfg(target_os = "macos")]
            Backend::Ring => return Err(OpenError::Unsupported),
            Backend::Readiness => Running::Readiness(readiness::Readiness::open(config)?),
        };
        Ok(Self { running, ring_refused: None })
    }

    /// The backend selected at startup.
    #[must_use]
    pub fn chosen(&self) -> Chosen {
        let backend = match &self.running {
            #[cfg(target_os = "linux")]
            Running::Ring(_) => Backend::Ring,
            Running::Readiness(_) => Backend::Readiness,
        };
        Chosen { backend, ring_refused: self.ring_refused }
    }

    /// Operations in flight, including completions not yet reaped.
    #[must_use]
    pub fn in_flight(&self) -> u32 {
        match &self.running {
            #[cfg(target_os = "linux")]
            Running::Ring(ring) => ring.in_flight(),
            Running::Readiness(readiness) => readiness.in_flight(),
        }
    }

    /// The number of records submit can take now.
    #[must_use]
    pub fn room(&self) -> u32 {
        match &self.running {
            #[cfg(target_os = "linux")]
            Running::Ring(ring) => ring.room(),
            Running::Readiness(readiness) => readiness.room(),
        }
    }

    /// Takes records while there is room, then waits as requested.
    pub fn submit(&mut self, records: &mut Queue<Submit>, wait: Wait) {
        match &mut self.running {
            #[cfg(target_os = "linux")]
            Running::Ring(ring) => ring.submit(records, wait),
            Running::Readiness(readiness) => readiness.submit(records, wait),
        }
    }

    /// Hands back completions and their records while `out` has room.
    pub fn reap(&mut self, out: &mut Queue<Complete>) {
        match &mut self.running {
            #[cfg(target_os = "linux")]
            Running::Ring(ring) => ring.reap(out),
            Running::Readiness(readiness) => readiness.reap(out),
        }
    }

    /// Open retries after a path resolution raced a rename or mount.
    #[must_use]
    pub const fn resubmitted(&self) -> u64 {
        match &self.running {
            #[cfg(target_os = "linux")]
            Running::Ring(ring) => ring.resubmitted(),
            Running::Readiness(readiness) => readiness.resubmitted(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::falls_back;
    use super::{Backend, Chosen, Kernel};
    use crate::Config;
    use crate::OpenError;

    #[test]
    fn only_setup_refusals_fall_back() {
        for (refused, expected) in [
            (OpenError::Setup(libc::ENOSYS), Some(libc::ENOSYS)),
            (OpenError::Setup(libc::EPERM), Some(libc::EPERM)),
            (OpenError::Setup(libc::EINVAL), None),
            (OpenError::Setup(libc::ENOMEM), None),
            (OpenError::Missing("x"), None),
            (OpenError::BelowFloor, None),
            (OpenError::NoOperations, None),
            (OpenError::Unsupported, None),
        ] {
            assert_eq!(falls_back(&refused), expected, "{refused:?}");
        }
    }

    // This opens Linux's completion ring.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_host_chooses_the_ring() {
        let kernel = Kernel::open(Config { operations: 2 }).expect("the host's ring is usable");
        assert_eq!(kernel.chosen(), Chosen { backend: Backend::Ring, ring_refused: None });
        assert_eq!(kernel.chosen().to_string(), "io_uring");
    }

    #[test]
    fn chosen_names_the_backend_and_the_ring_refusal() {
        #[cfg(target_os = "linux")]
        let readiness = "readiness (epoll)";
        #[cfg(target_os = "macos")]
        let readiness = "readiness (kqueue)";
        for (backend, ring_refused, expected) in [
            (Backend::Ring, None, "io_uring".to_owned()),
            (Backend::Readiness, None, readiness.to_owned()),
            (Backend::Readiness, Some(libc::EPERM), format!("{readiness}: io_uring refused (EPERM)")),
            (Backend::Readiness, Some(libc::ENOSYS), format!("{readiness}: io_uring refused (ENOSYS)")),
            (Backend::Readiness, Some(1234), format!("{readiness}: io_uring refused (1234)")),
        ] {
            assert_eq!(Chosen { backend, ring_refused }.to_string(), expected);
        }
    }

    // This opens Linux's completion ring.
    #[cfg(target_os = "linux")]
    #[test]
    fn forcing_a_backend_reports_it_and_refuses_zero_capacity() {
        for backend in [Backend::Ring, Backend::Readiness] {
            assert_eq!(
                Kernel::open_on(Config { operations: 0 }, backend).expect_err("zero capacity is refused"),
                OpenError::NoOperations
            );
            let kernel = Kernel::open_on(Config { operations: 2 }, backend).expect("the forced backend is usable");
            assert_eq!(kernel.chosen(), Chosen { backend, ring_refused: None });
            assert_eq!(kernel.in_flight(), 0);
            assert_eq!(kernel.room(), 2);
            assert_eq!(kernel.resubmitted(), 0);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_chooses_readiness_and_refuses_the_ring() {
        let kernel = Kernel::open(Config { operations: 2 }).expect("kqueue is usable");
        assert_eq!(kernel.chosen(), Chosen { backend: Backend::Readiness, ring_refused: None });
        assert_eq!(kernel.chosen().to_string(), "readiness (kqueue)");
        assert_eq!(kernel.room(), 2);
        assert_eq!(
            Kernel::open(Config { operations: 0 }).expect_err("zero capacity is refused"),
            OpenError::NoOperations
        );
        for operations in [0, 2] {
            assert_eq!(
                Kernel::open_on(Config { operations }, Backend::Ring).expect_err("Darwin has no ring"),
                OpenError::Unsupported
            );
        }
        assert_eq!(OpenError::Unsupported.to_string(), "the ring is not available on this system");
        assert_eq!(OpenError::BelowFloor.to_string(), "the kernel is below the floor, 24.0");
    }
}
