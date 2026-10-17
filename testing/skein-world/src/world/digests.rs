use super::{Host, Machine, Outcome};

/// The state digests a run ended with, for replay (testing.md, 5).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StateDigests {
    /// Each surviving process's, in admission order, as `Outcome::procs`.
    pub procs: Vec<Option<[u8; 32]>>,
    /// The fake machine's.
    pub machine: Option<[u8; 32]>,
}

impl<P: Host, M: Machine> Outcome<P, M> {
    /// Reads each surviving process's digest, then the machine's.
    #[must_use]
    pub fn state_digests(&self) -> StateDigests {
        StateDigests {
            procs: self.procs.iter().map(Host::state_digest).collect(),
            machine: self.machine.state_digest(),
        }
    }
}
