//! The public kernel and its startup choice (shell.md, 7.1).

use std::fmt;

use skein_io::kernel::{Complete, Submit};
use skein_lib::Queue;

use crate::{Config, OpenError, Wait, readiness, ring};

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
                f.write_str("readiness (epoll)")?;
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
#[expect(
    clippy::large_enum_variant,
    reason = "the startup choice holds the ring inline so its path adds no allocation or indirection (shell.md, 7.1)"
)]
enum Running {
    Ring(ring::Ring),
    Readiness(readiness::Readiness),
}

/// The ring's refusal that falls back to readiness (shell.md, 7.1):
/// `Setup(ENOSYS)` or `Setup(EPERM)`, its errno; anything else, None.
fn falls_back(refused: &OpenError) -> Option<i32> {
    match refused {
        OpenError::Setup(errno) => match *errno {
            libc::ENOSYS | libc::EPERM => Some(*errno),
            _ => None,
        },
        OpenError::Missing(_) | OpenError::BelowFloor | OpenError::NoOperations => None,
    }
}

impl Kernel {
    /// The host's backend (shell.md, 7.1).
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

    /// Forces a backend, so its conformance cannot silently use another.
    pub fn open_on(config: Config, backend: Backend) -> Result<Self, OpenError> {
        let running = match backend {
            Backend::Ring => Running::Ring(ring::Ring::open(config)?),
            Backend::Readiness => Running::Readiness(readiness::Readiness::open(config)?),
        };
        Ok(Self { running, ring_refused: None })
    }

    /// The backend selected at startup.
    #[must_use]
    pub fn chosen(&self) -> Chosen {
        let backend = match &self.running {
            Running::Ring(_) => Backend::Ring,
            Running::Readiness(_) => Backend::Readiness,
        };
        Chosen { backend, ring_refused: self.ring_refused }
    }

    /// Operations in flight, including completions not yet reaped.
    #[must_use]
    pub fn in_flight(&self) -> u32 {
        match &self.running {
            Running::Ring(ring) => ring.in_flight(),
            Running::Readiness(readiness) => readiness.in_flight(),
        }
    }

    /// The number of records submit can take now.
    #[must_use]
    pub fn room(&self) -> u32 {
        match &self.running {
            Running::Ring(ring) => ring.room(),
            Running::Readiness(readiness) => readiness.room(),
        }
    }

    /// Takes records while there is room, then waits as requested.
    pub fn submit(&mut self, records: &mut Queue<Submit>, wait: Wait) {
        match &mut self.running {
            Running::Ring(ring) => ring.submit(records, wait),
            Running::Readiness(readiness) => readiness.submit(records, wait),
        }
    }

    /// Hands back completions and their records while `out` has room.
    pub fn reap(&mut self, out: &mut Queue<Complete>) {
        match &mut self.running {
            Running::Ring(ring) => ring.reap(out),
            Running::Readiness(readiness) => readiness.reap(out),
        }
    }

    /// Open retries after a path resolution raced a rename or mount.
    #[must_use]
    pub const fn resubmitted(&self) -> u64 {
        match &self.running {
            Running::Ring(ring) => ring.resubmitted(),
            Running::Readiness(readiness) => readiness.resubmitted(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Backend, Chosen, Kernel, falls_back};
    use crate::{Config, OpenError};

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
        ] {
            assert_eq!(falls_back(&refused), expected, "{refused:?}");
        }
    }

    #[test]
    fn the_host_chooses_the_ring() {
        let kernel = Kernel::open(Config { operations: 2 }).expect("the host's ring is usable");
        assert_eq!(kernel.chosen(), Chosen { backend: Backend::Ring, ring_refused: None });
        assert_eq!(kernel.chosen().to_string(), "io_uring");
    }

    #[test]
    fn chosen_names_the_backend_and_the_ring_refusal() {
        for (backend, ring_refused, expected) in [
            (Backend::Ring, None, "io_uring"),
            (Backend::Readiness, None, "readiness (epoll)"),
            (Backend::Readiness, Some(libc::EPERM), "readiness (epoll): io_uring refused (EPERM)"),
            (Backend::Readiness, Some(libc::ENOSYS), "readiness (epoll): io_uring refused (ENOSYS)"),
            (Backend::Readiness, Some(1234), "readiness (epoll): io_uring refused (1234)"),
        ] {
            assert_eq!(Chosen { backend, ring_refused }.to_string(), expected);
        }
    }

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
}
