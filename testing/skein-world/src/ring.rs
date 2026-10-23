//! One kernel shared by host loops, with isolated local operation identities.
//! Bindings survive until completion; returned records retain host capacity
//! until delivered. A cancel is translated in both directions.
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use skein_io::kernel::{Complete, Op, Submit};
use skein_lib::{Queue, Token};
use skein_shell::{Backend, Config, Kernel, OpenError, Wait};

/// One host's original operation identity, including a cancel's target.
#[derive(Debug)]
pub struct Binding {
    /// Host index assigned when the ring is opened or the host is admitted.
    pub host: usize,
    /// The operation token in that host's namespace.
    pub local: Token,
    /// The original host-local target when the operation is a cancellation.
    pub cancel: Option<Token>,
}

/// The shared kernel and the bindings kept until records return to their host.
#[derive(Debug)]
pub struct Ring {
    pub(crate) kernel: Kernel,
    pub(crate) next: u64,
    pub(crate) limits: Vec<u32>,
    pub(crate) counts: Vec<u32>,
    pub(crate) bindings: BTreeMap<Token, Binding>,
    pub(crate) tokens: BTreeMap<(usize, Token), Token>,
    pub(crate) ready: Vec<VecDeque<Complete>>,
    pub(crate) submits: Queue<Submit>,
    pub(crate) arrived: Queue<Complete>,
}

impl Ring {
    /// Opens the shared kernel with a total bound and each host's declared bound.
    /// The total also covers any direct cleanup operations owned by the loop.
    pub fn new(operations: u32, limits: Vec<u32>) -> Result<Self, OpenError> {
        let kernel = Kernel::open(Config { operations })?;
        Ok(Self::with_kernel(operations, limits, kernel))
    }

    /// Opens the shared kernel on the backend a test forces (shell.md, 7.1).
    pub fn new_on(operations: u32, limits: Vec<u32>, backend: Backend) -> Result<Self, OpenError> {
        let kernel = Kernel::open_on(Config { operations }, backend)?;
        Ok(Self::with_kernel(operations, limits, kernel))
    }

    fn with_kernel(operations: u32, limits: Vec<u32>, kernel: Kernel) -> Self {
        Self {
            kernel,
            next: 0,
            counts: vec![0; limits.len()],
            ready: (0..limits.len()).map(|_| VecDeque::new()).collect(),
            limits,
            bindings: BTreeMap::new(),
            tokens: BTreeMap::new(),
            submits: Queue::with_capacity(operations),
            arrived: Queue::with_capacity(operations),
        }
    }

    /// Reaps raw kernel records, for loops that intercept operations themselves.
    pub fn reap(&mut self) -> &mut Queue<Complete> {
        self.kernel.reap(&mut self.arrived);
        &mut self.arrived
    }

    /// Queues a restored record until its owner has completion capacity.
    pub fn returned(&mut self, host: usize, complete: Complete) {
        self.ready.get_mut(host).expect("a queue per host").push_back(complete);
    }

    /// Queues an already registered operation for the next kernel entry.
    pub fn queue(&mut self, record: Submit) {
        self.submits.push(record);
    }

    /// Restores a reaped record to its host and local identity. The caller must
    /// queue it with `returned` until `deliver` hands it back to that host.
    pub fn complete(&mut self, mut complete: Complete) -> (usize, Complete) {
        let binding = self.bindings.remove(&complete.op).expect("a world token has its binding");
        self.tokens.remove(&(binding.host, binding.local)).expect("a local token has its binding");
        complete.op = binding.local;
        if let Some(target) = binding.cancel {
            complete.kind = Op::Cancel { target };
        }
        (binding.host, complete)
    }

    /// Returns queued completions to a host, releasing its operation capacity.
    pub fn deliver(&mut self, host: usize, out: &mut Queue<Complete>) {
        while out.room() > 0 {
            let Some(complete) = self.ready.get_mut(host).expect("a queue per host").pop_front() else { break };
            let count = self.counts.get_mut(host).expect("a count per host");
            *count = count.checked_sub(1).expect("a returned operation was in flight");
            out.push(complete);
        }
    }

    /// Binds a host-local submission, including its cancellation target.
    /// A local token may be reused only after its terminal has been delivered.
    pub fn register(&mut self, host: usize, mut record: Submit) -> Submit {
        assert!(
            self.counts.get(host).expect("a count per host") < self.limits.get(host).expect("a limit per host"),
            "host {host} exceeds its own operation limit"
        );
        assert!(
            !self.tokens.contains_key(&(host, record.op))
                && !self.ready.get(host).expect("a queue per host").iter().any(|complete| complete.op == record.op),
            "a host never reuses a token in flight"
        );
        let token = self.fresh();
        let local = record.op;
        let cancel = if let Op::Cancel { target } = &mut record.kind {
            let original = *target;
            *target = self.tokens.get(&(host, original)).copied().unwrap_or(Token::new(0));
            Some(original)
        } else {
            None
        };
        self.tokens.insert((host, local), token);
        self.bindings.insert(token, Binding { host, local, cancel });
        let count = self.counts.get_mut(host).expect("a count per host");
        *count = count.checked_add(1).expect("within the host limit");
        record.op = token;
        record
    }

    /// Reserves a kernel token for loop-owned cleanup, outside host bindings.
    pub fn fresh(&mut self) -> Token {
        self.next = self.next.checked_add(1).expect("world tokens do not wrap");
        Token::new(self.next)
    }

    /// Adds a host whose declared operations fit the provisioned total.
    pub fn admit(&mut self, limit: u32) -> usize {
        let host = self.limits.len();
        self.limits.push(limit);
        self.counts.push(0);
        self.ready.push(VecDeque::new());
        host
    }

    /// Counts a host's operations, including terminals waiting for delivery.
    #[must_use]
    pub fn count(&self, host: usize) -> u32 {
        *self.counts.get(host).expect("a count per host")
    }

    /// Submits every queued operation and waits as requested.
    pub fn enter(&mut self, wait: Wait) {
        self.kernel.submit(&mut self.submits, wait);
        assert!(self.submits.is_empty(), "the ring is provisioned for every host and cleanup");
    }

    /// Whether any host has a completion waiting for delivery.
    #[must_use]
    pub fn has_ready(&self) -> bool {
        self.ready.iter().any(|ready| !ready.is_empty())
    }

    /// Whether the kernel and all hosts' returned queues are empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.kernel.in_flight() == 0 && !self.has_ready()
    }
}

#[cfg(test)]
mod tests {
    use super::Ring;
    use skein_io::kernel::{Complete, Done, Op, Submit};
    use skein_lib::{Queue, Token};
    use skein_shell::OpenError;

    #[test]
    fn opening_returns_the_kernel_configuration_error() {
        assert!(matches!(Ring::new(0, vec![]), Err(OpenError::NoOperations)));
    }

    #[test]
    fn cancellation_preserves_host_local_targets_and_reuse() {
        // Two hosts, each with one operation and its cancellation in flight.
        const PER_HOST: u32 = 2;
        const TOTAL: u32 = 4;
        let mut ring = Ring::new(TOTAL, vec![PER_HOST, PER_HOST]).expect("test ring");
        let local = Token::new(10);
        let cancel = Token::new(11);
        let first = ring.register(0, Submit { op: local, kind: Op::Close { fd: skein_io::kernel::Fd::new(-1) } });
        let second = ring.register(1, Submit { op: local, kind: Op::Close { fd: skein_io::kernel::Fd::new(-1) } });
        assert_ne!(first.op, second.op);
        for (host, pending) in [(0, first), (1, second)] {
            let submitted = ring.register(host, Submit { op: cancel, kind: Op::Cancel { target: local } });
            assert_eq!(submitted.kind, Op::Cancel { target: pending.op });
            let (owner, completed) =
                ring.complete(Complete { op: submitted.op, kind: submitted.kind, result: Ok(Done::Nothing) });
            assert_eq!(owner, host);
            assert_eq!(completed.op, cancel);
            assert_eq!(completed.kind, Op::Cancel { target: local });
            ring.returned(owner, completed);
            let (owner, completed) =
                ring.complete(Complete { op: pending.op, kind: pending.kind, result: Ok(Done::Nothing) });
            ring.returned(owner, completed);
            assert_eq!(ring.count(host), PER_HOST);
            let mut returned = Queue::with_capacity(PER_HOST);
            ring.deliver(host, &mut returned);
            assert_eq!(ring.count(host), 0);
            let reused = ring.register(host, Submit { op: local, kind: Op::Cancel { target: local } });
            assert_ne!(reused.op, pending.op);
            assert_eq!(reused.kind, Op::Cancel { target: Token::new(0) });
        }
    }
}
