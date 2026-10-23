//! Shipped binaries beside the scripted processes of a real world
//! (testing-strategy.md, section 2.9; examples.md, section 6). This module
//! keeps the child's pidfd, captured stderr and pending kernel operations;
//! it never knows the service's state. `Binary::start` opens its pipes or
//! controlling terminal, `take_streams` transfers the scripted ends for
//! adoption, and the `Host` entry points wait, signal, read and close on
//! the world's shared ring. The referee observes stderr and `exit_status`.
//! A supervised tree retains its keeper after leader exit; `signal_tree` queues
//! member-pidfd delivery by the keeper, while the referee owns the grace.
//! `abandon` cancels observation through the shared kernel before tree cleanup
//! and closure; its host stays in the loop until empty.

use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use skein_io::kernel::{Complete, Done, Error, Exit, Fd, Op, Pipe, Signal, Spawn, Submit, Target, Way};
use skein_lib::{Duration, Queue, Time, Token, Wall, bytes};
use skein_shell::{Config, Kernel, OpenError, Wait};

use crate::Host;
use crate::tree::{self, Tree};
pub use crate::tree::{Cleanup, Containment, Counts, Expectation, Method, PeakScope, TreeStatus};

/// A scenario's binary command, with an exact environment and working directory.
#[derive(Debug)]
pub struct Command {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub environment: Vec<(OsString, OsString)>,
    pub directory: PathBuf,
}

/// How the scenario connects its scripted person to the child.
#[derive(Clone, Copy, Debug)]
pub enum Mode {
    /// Separate stdin/stdout pipes, transferred to the scripted hosts.
    Pipes,
    /// One controlling terminal, transferred to the scripted host.
    Terminal,
}

/// Startup failed before a binary joined the loop; the scenario reports why.
#[derive(Debug)]
pub enum StartError {
    /// The real ring is unusable, so no child was started.
    Ring(OpenError),
    /// Opening the command's working directory failed with this errno.
    Directory(i32),
    /// Preparing tree containment failed before spawn.
    Tree(Error),
    /// Making pipes, the terminal or executing the child failed.
    Child(Error),
}

/// Scripted descriptors transferred by the child to their owning hosts.
#[derive(Debug)]
pub enum Streams {
    /// The person's writing end of stdin and reading end of stdout.
    Pipes { input: Fd, output: Fd },
    /// The person's bidirectional terminal master; the child owns the slave.
    Terminal { stream: Fd },
}

impl Streams {
    /// The descriptors a real world declares when admitting the scripted host.
    #[must_use]
    pub fn descriptors(&self) -> Vec<Fd> {
        match *self {
            Self::Pipes { input, output } => vec![input, output],
            Self::Terminal { stream } => vec![stream],
        }
    }
}

/// An external child's observer, owned by a real world until exit and closure.
#[derive(Debug)]
pub struct Binary {
    pidfd: Option<Fd>,
    stderr: Option<Fd>,
    streams: Option<Streams>,
    capture: Capture,
    status: Option<Exit>,
    wait_started: bool,
    waiting: Option<Token>,
    reading: Option<Token>,
    abandoning: bool,
    cancelling: u32,
    signal: Option<(Signal, Target)>,
    tree_signal: Option<Signal>,
    tree_sweep: Option<Signal>,
    tree: Option<Box<Tree>>,
    tree_status: Box<TreeStatus>,
    counts: Option<Counts>,
    expectation: Expectation,
    poll_at: Option<Time>,
    signalling: bool,
    closing: u32,
    next: u64,
    completions: Queue<Complete>,
    submissions: Queue<Submit>,
}

/// A bounded prefix and sticky overflow evidence, independent of pipe drainage.
#[derive(Debug)]
struct Capture {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}

impl Capture {
    fn new(limit: usize) -> Self {
        Self { bytes: Vec::with_capacity(limit), limit, overflowed: false }
    }

    fn push(&mut self, bytes: &[u8], expectation: Expectation) {
        let room = self.limit.checked_sub(self.bytes.len()).expect("capture stays within its cap");
        if bytes.len() > room {
            match expectation {
                Expectation::Supervise => self.overflowed = true,
                Expectation::EndsWithBinary | Expectation::Measure => {
                    assert!(bytes.len() <= room, "binary stderr exceeds its scenario limit of {} bytes", self.limit);
                }
            }
        }
        let kept = bytes.len().min(room);
        self.bytes.extend_from_slice(bytes.get(..kept).expect("capture fits within the completed read"));
    }
}

impl Binary {
    /// Starts a binary after probing `io_uring`; temporary startup resources
    /// close before returning. `stderr_limit` bounds captured bytes, and an
    /// overflow fails the scenario. Environment entries are not inherited.
    pub fn start(command: Command, mode: Mode, stderr_limit: usize) -> Result<Self, StartError> {
        Self::start_with_tree(command, mode, stderr_limit, Expectation::EndsWithBinary, false)
    }

    /// Starts a binary with an explicit tree expectation; `force_walk` exercises
    /// the subreaper path even on a machine with delegated cgroups. The walk
    /// requires one observer per test process, since its counts are process-wide.
    /// In `Supervise`, stderr overflow retains its bounded prefix and is reported
    /// by `stderr_overflowed`; the referee owns termination while capture drains.
    pub fn start_with_tree(
        command: Command,
        mode: Mode,
        stderr_limit: usize,
        expectation: Expectation,
        force_walk: bool,
    ) -> Result<Self, StartError> {
        Self::start_contained(
            command,
            mode,
            stderr_limit,
            expectation,
            if force_walk { Containment::Walk } else { Containment::Best },
        )
    }

    /// Starts with explicit containment. `Containment::Best` uses a delegated
    /// cgroup on Linux, falling back to the walk, and the group on macOS.
    /// `Containment::Walk` requires Linux's subreaper; `Containment::Group`
    /// uses the binary's process group and adopts no descendants. A containment
    /// the machine cannot run fails the start with `StartError::Tree`.
    pub fn start_contained(
        command: Command,
        mode: Mode,
        stderr_limit: usize,
        expectation: Expectation,
        containment: Containment,
    ) -> Result<Self, StartError> {
        let mut startup = Kernel::open(Config { operations: 1 }).map_err(StartError::Ring)?;
        let until = skein_shell::Clock::new().now().now.saturating_add(Duration::from_secs(2));
        let baseline = tree::call(&mut startup, Op::Usage, until).expect("startup usage succeeds");
        let Done::Usage(baseline) = baseline else { unreachable!("Usage returns usage") };
        let mut tree = Tree::prepare(containment, baseline).map_err(StartError::Tree)?;
        let root = skein_shell::open_root(&command.directory).map_err(StartError::Directory)?;
        let terminal = matches!(mode, Mode::Terminal);
        let mut pipes = Vec::new();
        if !terminal {
            pipes.push(Pipe { child: 0, way: Way::In, parent: None });
            pipes.push(Pipe { child: 1, way: Way::Out, parent: None });
        }
        pipes.push(Pipe { child: 2, way: Way::Out, parent: None });
        let mut spawn = Spawn {
            root,
            dir: Box::from([]),
            program: Box::from(command.program.as_os_str().as_bytes()),
            args: command.arguments.iter().map(|argument| Box::from(argument.as_bytes())).collect(),
            env: command
                .environment
                .iter()
                .map(|(name, value)| {
                    let mut entry = name.as_bytes().to_vec();
                    entry.push(b'=');
                    entry.extend_from_slice(value.as_bytes());
                    entry.into_boxed_slice()
                })
                .collect(),
            pipes: pipes.into_boxed_slice(),
        };
        // Cgroups are Linux's (shell.md, 7.5).
        #[cfg(target_os = "linux")]
        let child = match tree.cgroup() {
            Some(cgroup) => skein_shell::start_binary_in_cgroup(&mut spawn, terminal, cgroup),
            None => skein_shell::start_binary(&mut spawn, terminal),
        };
        #[cfg(not(target_os = "linux"))]
        let child = skein_shell::start_binary(&mut spawn, terminal);
        let mut submits = Queue::with_capacity(1);
        let mut completes = Queue::with_capacity(1);
        submits.push(Submit { op: Token::new(1), kind: Op::Close { fd: root } });
        startup.submit(&mut submits, Wait::No);
        while completes.is_empty() {
            startup.reap(&mut completes);
            if completes.is_empty() {
                startup.submit(&mut submits, Wait::Forever);
            }
        }
        let closed = completes.pop().expect("startup root close completed");
        assert!(matches!(closed.kind, Op::Close { .. }), "startup closes only its root");
        let (pidfd, master) = child.map_err(StartError::Child)?;
        tree.started(pidfd);
        let stderr = spawn.pipes.last().and_then(|pipe| pipe.parent).expect("stderr was requested");
        let streams = match master {
            Some(stream) => Streams::Terminal { stream },
            None => Streams::Pipes {
                input: spawn.pipes.first().and_then(|pipe| pipe.parent).expect("stdin was requested"),
                output: spawn.pipes.get(1).and_then(|pipe| pipe.parent).expect("stdout was requested"),
            },
        };
        Ok(Self {
            pidfd: Some(pidfd),
            stderr: Some(stderr),
            streams: Some(streams),
            capture: Capture::new(stderr_limit),
            status: None,
            wait_started: false,
            waiting: None,
            reading: None,
            abandoning: false,
            cancelling: 0,
            signal: None,
            tree_signal: None,
            tree_sweep: None,
            tree: Some(Box::new(tree)),
            tree_status: Box::new(TreeStatus::new()),
            counts: None,
            expectation,
            poll_at: Some(Time::ZERO),
            signalling: false,
            closing: 0,
            next: 0,
            completions: Queue::with_capacity(8),
            submissions: Queue::with_capacity(8),
        })
    }

    /// Transfers stdin/stdout or the terminal for adoption by scripted hosts.
    /// Unclaimed ends close on the binary observer's first iteration.
    pub fn take_streams(&mut self) -> Option<Streams> {
        self.streams.take()
    }

    /// The descriptors declared by the real world when it admits this observer.
    #[must_use]
    pub fn descriptors(&self) -> Vec<Fd> {
        self.pidfd.into_iter().chain(self.stderr).chain(self.streams.iter().flat_map(Streams::descriptors)).collect()
    }

    /// Queues a referee's pidfd signal for the next shared-ring iteration.
    pub fn signal(&mut self, signal: Signal) {
        assert!(!self.abandoning, "an abandoned observer accepts no signals");
        assert!(self.status.is_none(), "the referee signals a child before its exit");
        assert!(self.signal.is_none(), "one pending referee signal at a time");
        self.signal = Some((signal, Target::Child));
    }

    /// Queues a group signal while the binary's leader is retained.
    pub fn signal_group(&mut self, signal: Signal) {
        assert!(!self.abandoning, "an abandoned observer accepts no signals");
        assert!(self.pidfd.is_some(), "the leader remains owned until tree settlement");
        assert!(self.signal.is_none(), "one pending referee signal at a time");
        self.signal = Some((signal, Target::Group));
    }

    /// Queues a keeper signal to every discovered live tree member, including
    /// detached descendants after leader exit. `Supervise` leaves its grace and
    /// escalation deadlines to the referee. Membership is refreshed at delivery.
    pub fn signal_tree(&mut self, signal: Signal) {
        assert!(!self.abandoning, "an abandoned observer accepts no signals");
        assert_eq!(self.expectation, Expectation::Supervise, "whole-tree policy belongs to a supervised observer");
        assert!(self.tree.is_some(), "a signal needs a retained tree keeper");
        assert!(self.tree_signal.is_none(), "one pending tree signal at a time");
        self.tree_signal = Some(signal);
    }

    /// The supervised tree's last poll and delivered signals. Its
    /// history remains available after `counts` reports settlement.
    #[must_use]
    pub fn tree_status(&self) -> &TreeStatus {
        &self.tree_status
    }

    /// Ends a measured command; final counts arrive after its tree settles.
    pub fn end(&mut self) {
        match self.expectation {
            Expectation::Supervise => self.signal_tree(Signal::Kill),
            Expectation::EndsWithBinary | Expectation::Measure => self.signal_group(Signal::Kill),
        }
    }

    /// Abandons observation and cancels its pending reads and wait. Keep driving
    /// this host until `is_empty`: target and cancel completions precede tree
    /// reaping and descriptor closure, on every kernel backend (kernel.md, 7).
    /// Repeating the request while it settles has no effect.
    pub fn abandon(&mut self) {
        if self.abandoning {
            return;
        }
        self.abandoning = true;
        self.signal = None;
        self.tree_signal = None;
        self.tree_sweep = None;
        self.poll_at = None;
        for target in [self.waiting, self.reading].into_iter().flatten() {
            self.cancelling = self.cancelling.checked_add(1).expect("at most two cancels");
            let _op = self.submit(Op::Cancel { target });
        }
    }

    /// The terminal counts, present only once the binary and every descendant settled.
    #[must_use]
    pub const fn counts(&self) -> Option<Counts> {
        self.counts
    }

    /// What the child wrote to stderr; it is outside observation, not service state.
    #[must_use]
    pub fn stderr(&self) -> &[u8] {
        &self.capture.bytes
    }

    /// Whether supervised stderr exceeded its capture limit. The captured prefix
    /// remains bounded and subsequent bytes drain without retention. This flag
    /// stays set after settlement; the referee chooses the failure and tree ladder.
    #[must_use]
    pub const fn stderr_overflowed(&self) -> bool {
        self.capture.overflowed
    }

    /// The ring's child-exit event, available before final descriptor closure.
    #[must_use]
    pub const fn exit_status(&self) -> Option<Exit> {
        self.status
    }

    fn submit(&mut self, kind: Op) -> Token {
        self.next = self.next.checked_add(1).expect("binary operation tokens do not wrap");
        let op = Token::new(self.next);
        self.submissions.push(Submit { op, kind });
        op
    }

    fn close(&mut self, fd: Fd) {
        self.closing = self.closing.checked_add(1).expect("a small number of descriptors");
        let _op = self.submit(Op::Close { fd });
    }

    fn observation_pending(&self) -> bool {
        self.waiting.is_some() || self.reading.is_some() || self.signalling || self.cancelling != 0 || self.closing != 0
    }

    fn settle_abandoned(&mut self) {
        if self.observation_pending() {
            return;
        }
        if let Some(tree) = &mut self.tree
            && let Some(pidfd) = self.pidfd
        {
            tree.abandon(pidfd, false);
        }
        self.tree = None;
        if let Some(pidfd) = self.pidfd.take() {
            self.close(pidfd);
        }
        if let Some(stderr) = self.stderr.take() {
            self.close(stderr);
        }
        if let Some(streams) = self.streams.take() {
            for fd in streams.descriptors() {
                self.close(fd);
            }
        }
    }

    fn completed(&mut self, complete: Complete) {
        if let Op::Wait { .. } = complete.kind {
            assert_eq!(self.waiting.take(), Some(complete.op), "the pending wait completed");
            if self.abandoning && complete.result == Err(Error::Cancelled) {
                return;
            }
            let Ok(Done::Exit(status)) = complete.result else {
                crate::fail(&format!("binary wait failed: {complete:?}"))
            };
            self.status = Some(status);
        } else if let Op::PipeRead { buf, .. } = complete.kind {
            assert_eq!(self.reading.take(), Some(complete.op), "the pending read completed");
            if self.abandoning {
                assert!(
                    matches!(complete.result, Ok(Done::Count(_)) | Err(Error::Cancelled)),
                    "abandoned stderr read failed: {:?}",
                    complete.result
                );
                return;
            }
            let Ok(Done::Count(count)) = complete.result else {
                crate::fail(&format!("binary stderr read failed: {:?}", complete.result))
            };
            if count == 0 {
                let fd = self.stderr.take().expect("stderr remains open through its read");
                self.close(fd);
            } else {
                let count = usize::try_from(count).expect("a read count fits memory");
                let bytes = buf.get(..count).expect("the kernel returned no more than requested");
                self.capture.push(bytes, self.expectation);
            }
        } else if let Op::Signal { .. } = complete.kind {
            self.signalling = false;
            if let Err(error) = complete.result {
                // Exit and a referee's signal may cross in the kernel.
                assert!(error == Error::NotFound || error == Error::Other(3), "binary signal failed: {error:?}");
            }
        } else if let Op::Cancel { .. } = complete.kind {
            self.cancelling = self.cancelling.checked_sub(1).expect("a submitted cancel completed");
            assert!(
                matches!(complete.result, Ok(Done::Nothing) | Err(Error::TooLate)),
                "binary cancel failed: {:?}",
                complete.result
            );
        } else if let Op::Close { .. } = complete.kind {
            self.closing = self.closing.checked_sub(1).expect("a submitted close completed");
        } else {
            unreachable!("the binary observer submits only wait, read, signal, cancel and close");
        }
    }
}

// Ordinary test code may unwind (programming-model.md, section 10.2).
// A failed scenario must not leave a shipped service running after its
// observer is discarded; a normally settled observer owns no descriptors.
impl Drop for Binary {
    fn drop(&mut self) {
        let retain_leader = self.waiting.is_some() || self.signalling;
        if let Some(tree) = &mut self.tree
            && let Some(pidfd) = self.pidfd
        {
            tree.abandon(pidfd, retain_leader);
        }
        // Drop cannot drain the driver's kernel. Leave descriptors with an
        // outstanding record open; explicit abandonment settles them first.
        let descriptors = self
            .descriptors()
            .into_iter()
            .filter(|fd| {
                !(Some(*fd) == self.pidfd && retain_leader || Some(*fd) == self.stderr && self.reading.is_some())
            })
            .collect::<Vec<_>>();
        let leader = if retain_leader { None } else { self.pidfd.take() };
        if !descriptors.is_empty() {
            skein_shell::abandon_binary(leader, &descriptors);
        }
    }
}

impl Host for Binary {
    fn iterate(&mut self, now: Time, _wall: Wall) {
        if self.abandoning {
            while let Some(complete) = self.completions.pop() {
                self.completed(complete);
            }
            self.settle_abandoned();
            return;
        }
        let poll_tree = self.poll_at.is_some_and(|at| at <= now);
        if poll_tree {
            if let Some(tree) = &mut self.tree {
                tree.refresh();
            }
            self.poll_at = Some(now.saturating_add(Duration::from_millis(10)));
        }
        while let Some(complete) = self.completions.pop() {
            self.completed(complete);
        }
        if self.expectation == Expectation::Supervise
            && (poll_tree || self.status.is_some() && self.tree_status.live_at_leader_exit.is_none())
            && let Some(tree) = &mut self.tree
        {
            self.tree_status.live_pids = tree.survey(self.pidfd.expect("a retained tree has its leader pidfd"));
            if self.status.is_some() && self.tree_status.live_at_leader_exit.is_none() {
                self.tree_status.live_at_leader_exit = Some(tree.leader_exited(&self.tree_status.live_pids));
            }
        }
        if let Some(signal) = self.tree_signal.take().or(if poll_tree { self.tree_sweep } else { None }) {
            if signal == Signal::Kill {
                self.tree_sweep = Some(Signal::Kill);
            }
            let tree = self.tree.as_mut().expect("a pending signal has a tree");
            let (delivered, forced) = tree.signal(self.pidfd.expect("a retained tree has its leader pidfd"), signal);
            self.tree_status.forced_cleanup |= forced;
            let history = match signal {
                Signal::Interrupt => &mut self.tree_status.interrupted_pids,
                Signal::Terminate => &mut self.tree_status.terminated_pids,
                Signal::Kill => &mut self.tree_status.killed_pids,
                Signal::Stop => &mut self.tree_status.stopped_pids,
                Signal::Continue => &mut self.tree_status.continued_pids,
            };
            history.extend(delivered);
            history.sort_unstable();
            history.dedup();
            self.tree_status.live_pids = tree.survey(self.pidfd.expect("a retained tree has its leader pidfd"));
        }
        if let Some(streams) = self.streams.take() {
            for fd in streams.descriptors() {
                self.close(fd);
            }
        }
        if !self.wait_started {
            self.wait_started = true;
            self.waiting =
                Some(self.submit(Op::Wait { pidfd: self.pidfd.expect("a child has its pidfd"), reap: false }));
        }
        if self.reading.is_none()
            && let Some(fd) = self.stderr
        {
            self.reading = Some(self.submit(Op::PipeRead { fd, buf: bytes::zeroed(4096) }));
        }
        if !self.signalling
            && let Some((signal, to)) = self.signal.take()
        {
            self.signalling = true;
            let _op = self.submit(Op::Signal { pidfd: self.pidfd.expect("a running child has its pidfd"), signal, to });
        }
        if self.status.is_some()
            && !self.signalling
            && let Some(pidfd) = self.pidfd
        {
            if self.expectation == Expectation::Supervise {
                let tree = self.tree.as_mut().expect("a retained leader has a tree");
                if !self.tree_status.live_pids.is_empty() || !tree.unpopulated() {
                    return;
                }
            }
            if let Some(tree) = &mut self.tree {
                self.counts = Some(tree.finish(pidfd, self.expectation, true).unwrap_or_else(|why| crate::fail(&why)));
            }
            self.tree = None;
            self.pidfd = None;
            self.poll_at = None;
            self.close(pidfd);
        }
    }

    fn completions(&mut self) -> &mut Queue<Complete> {
        &mut self.completions
    }

    fn submissions(&mut self) -> &mut Queue<Submit> {
        &mut self.submissions
    }

    fn work_pending(&self, now: Time) -> bool {
        if self.abandoning {
            return !self.completions.is_empty()
                || !self.submissions.is_empty()
                || (!self.observation_pending()
                    && (self.pidfd.is_some() || self.stderr.is_some() || self.streams.is_some()));
        }
        self.poll_at.is_some_and(|at| at <= now)
            || !self.completions.is_empty()
            || !self.wait_started
            || self.streams.is_some()
            || (self.signal.is_some() && !self.signalling)
            || self.tree_signal.is_some()
            || (self.stderr.is_some() && self.reading.is_none())
            || (self.expectation != Expectation::Supervise
                && self.status.is_some()
                && self.pidfd.is_some()
                && !self.signalling)
    }

    fn next_deadline(&self) -> Option<Time> {
        self.poll_at
    }

    fn next_policy_deadline(&self) -> Option<Time> {
        None
    }

    fn is_empty(&self) -> bool {
        self.pidfd.is_none()
            && self.stderr.is_none()
            && self.streams.is_none()
            && self.waiting.is_none()
            && self.reading.is_none()
            && self.cancelling == 0
            && self.closing == 0
            && !self.signalling
            && self.signal.is_none()
            && self.tree_signal.is_none()
            && self.completions.is_empty()
            && self.submissions.is_empty()
    }

    fn worst_case(&self) -> u64 {
        u64::try_from(self.capture.limit)
            .expect("stderr cap fits u64")
            .checked_add(32 * 1024)
            .expect("observer memory fits u64")
    }

    fn operations(&self) -> u32 {
        8
    }
}

#[cfg(test)]
mod tests {
    use super::{Capture, Expectation};

    #[test]
    fn supervised_capture_keeps_an_exact_prefix_and_overflow_stays_set() {
        let mut capture = Capture::new(5);
        capture.push(b"ab", Expectation::Supervise);
        capture.push(b"cde", Expectation::Supervise);
        assert!(!capture.overflowed, "exactly the cap is not overflow");
        capture.push(b"fg", Expectation::Supervise);
        capture.push(b"later", Expectation::Supervise);
        capture.push(b"", Expectation::Supervise);
        assert_eq!(capture.bytes, b"abcde");
        assert_eq!(capture.bytes.capacity(), 5, "overflow never grows the capture allocation");
        assert!(capture.overflowed, "overflow remains visible after subsequent reads and EOF");
    }

    #[test]
    fn a_zero_capture_distinguishes_no_bytes_from_discarded_bytes() {
        let mut capture = Capture::new(0);
        capture.push(b"", Expectation::Supervise);
        assert!(!capture.overflowed);
        capture.push(b"x", Expectation::Supervise);
        assert!(capture.overflowed);
        assert!(capture.bytes.is_empty());
        assert_eq!(capture.bytes.capacity(), 0);
    }

    #[test]
    #[should_panic(expected = "binary stderr exceeds its scenario limit of 2 bytes")]
    fn measured_capture_keeps_the_legacy_scenario_assertion() {
        Capture::new(2).push(b"abc", Expectation::Measure);
    }
}
