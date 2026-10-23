//! A binary's tree keeper (examples.md, section 6). It keeps delegated cgroup
//! paths, discovered pidfds, or a process group ID, and the pre-spawn usage
//! baseline, never service state. `prepare` chooses containment, `refresh` finds adopted children,
//! and `finish` kills leftovers, settles every owned pidfd and returns counts.
//! An abandoned keeper performs the same settlement before releasing its paths.
//! The last Linux keeper restores the observer's prior subreaper setting, so later
//! fixture descendants are not adopted on that binary's behalf.
//! Supervised scenarios keep identities before adoption, survey live members,
//! and choose their own TERM and KILL deadlines (benchmarks.md, section 10).

#[cfg(target_os = "linux")]
use alloc::collections::{BTreeMap, BTreeSet};
use core::cell::Cell;
#[cfg(target_os = "linux")]
use core::cell::RefCell;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::io::{Read, Seek};
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};

use skein_io::kernel::{Complete, Done, Error, Fd, Op, Resources, Signal, Submit, Target, Usage};
use skein_lib::{Duration, Queue, Time, Token};
use skein_shell::{Clock, Config, Kernel, Wait};

// The harness runs on one thread. A walk uses the process-wide reaped-child
// counter, so another observer cannot share that accounting interval.
std::thread_local! {
    static ACTIVE: Cell<u32> = const { Cell::new(0) };
    #[cfg(target_os = "linux")]
    static WALK_ACTIVE: Cell<bool> = const { Cell::new(false) };
    #[cfg(target_os = "linux")]
    static SUBREAPER_BEFORE: Cell<bool> = const { Cell::new(false) };
    #[cfg(target_os = "linux")]
    static SUBREAPER_OWNED: Cell<bool> = const { Cell::new(false) };
    #[cfg(target_os = "linux")]
    static CGROUP_BASE: RefCell<Option<CgroupBase>> = const { RefCell::new(None) };
}

/// The settlement a test asks for (examples.md, 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Containment {
    /// The best the machine has: on Linux a delegated cgroup, else the walk; on macOS the group.
    Best,
    /// The subreaper's walk of pidfds (Linux).
    Walk,
    /// The binary's process group (both systems).
    Group,
}

/// The tree settlement method chosen at startup, reported to the scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// A delegated cgroup v2 contains every descendant from the spawn.
    Cgroup,
    /// The subreaper discovers children and waits on their pidfds.
    Walk,
    /// The binary's process group: settled once the binary is reaped and the group has no member.
    Group,
}

/// The resident-size scope of a settled tree's result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeakScope {
    /// All members resident at the same time, measured by the cgroup.
    Tree,
    /// The largest single process; prior reaped children may establish this baseline.
    LargestProcess { baseline_bytes: u64 },
}

/// The keeper's terminal counts, read only after the whole tree settles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counts {
    pub method: Method,
    pub user: Duration,
    pub system: Duration,
    pub peak_rss_bytes: u64,
    pub peak_scope: PeakScope,
}

/// What the scenario requires at the leader's exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expectation {
    /// An owned service must have ended every descendant; leftovers fail by PID.
    EndsWithBinary,
    /// A measured command may leave children, which the keeper kills and counts.
    Measure,
    /// A referee owns the deadlines and signals; natural settlement never kills.
    Supervise,
}

/// The strongest cleanup signal delivered to a process observed alive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cleanup {
    /// Every process ended without a cleanup signal.
    None,
    /// At least one live process received TERM, and none received KILL.
    Terminate,
    /// At least one live process still needed KILL.
    Kill,
}

/// Outside evidence retained by a supervised observer, including after settlement.
#[derive(Debug)]
pub struct TreeStatus {
    /// The leader and descendants alive at the most recent keeper poll.
    pub live_pids: Vec<u32>,
    /// Live descendants when the leader's exit was first observed; absent before it.
    pub live_at_leader_exit: Option<Vec<u32>>,
    /// Identities observed alive and successfully sent INT by the keeper.
    pub interrupted_pids: Vec<u32>,
    /// Identities observed alive and successfully sent TERM by the keeper.
    pub terminated_pids: Vec<u32>,
    /// Identities observed alive and successfully sent KILL by the keeper.
    pub killed_pids: Vec<u32>,
    /// Identities observed alive and successfully sent STOP by the keeper.
    pub stopped_pids: Vec<u32>,
    /// Identities observed alive and successfully sent CONT by the keeper.
    pub continued_pids: Vec<u32>,
    /// A live member needed KILL, including a cgroup member racing the pidfd scan.
    pub forced_cleanup: bool,
}

impl TreeStatus {
    pub(crate) fn new() -> Self {
        Self {
            live_pids: Vec::new(),
            live_at_leader_exit: None,
            interrupted_pids: Vec::new(),
            terminated_pids: Vec::new(),
            killed_pids: Vec::new(),
            stopped_pids: Vec::new(),
            continued_pids: Vec::new(),
            forced_cleanup: false,
        }
    }

    /// The strongest successful cleanup action, rather than a requested signal.
    #[must_use]
    pub fn cleanup(&self) -> Cleanup {
        if self.forced_cleanup {
            Cleanup::Kill
        } else if !self.terminated_pids.is_empty() {
            Cleanup::Terminate
        } else {
            Cleanup::None
        }
    }
}

// skein-world uses the shell's Error vocabulary without depending on libc.
// EBUSY is 16 on both systems; ENOTSUP is 95 on Linux and 45 on Darwin.
const BUSY: Error = Error::Other(16);
#[cfg(target_os = "linux")]
const NOT_SUPPORTED: Error = Error::Other(95);
#[cfg(not(target_os = "linux"))]
const NOT_SUPPORTED: Error = Error::Other(45);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    Best,
    Walk,
    Group,
}

fn choose(
    containment: Containment,
    linux: bool,
    subreaper: bool,
    walk_active: bool,
    active: u32,
) -> Result<Choice, Error> {
    let choice = match containment {
        Containment::Best => {
            if linux {
                Choice::Best
            } else {
                Choice::Group
            }
        }
        Containment::Walk => {
            if !linux {
                return Err(NOT_SUPPORTED);
            }
            Choice::Walk
        }
        Containment::Group => Choice::Group,
    };
    if walk_active {
        return Err(BUSY);
    }
    match choice {
        Choice::Group if subreaper => Err(BUSY),
        Choice::Walk if active != 0 => Err(BUSY),
        Choice::Best | Choice::Walk | Choice::Group => Ok(choice),
    }
}

#[cfg(target_os = "linux")]
fn prepare_choice(containment: Containment) -> Result<Choice, Error> {
    choose(containment, true, skein_shell::subreaper()?, WALK_ACTIVE.get(), ACTIVE.get())
}

#[cfg(not(target_os = "linux"))]
fn prepare_choice(containment: Containment) -> Result<Choice, Error> {
    choose(containment, false, false, false, ACTIVE.get())
}

#[derive(Debug)]
pub(crate) enum Tree {
    #[cfg(target_os = "linux")]
    Linux(LinuxTree),
    Group(GroupTree),
}

impl Tree {
    pub(crate) fn prepare(containment: Containment, baseline: Usage) -> Result<Self, Error> {
        match prepare_choice(containment)? {
            Choice::Group => {
                ACTIVE.set(ACTIVE.get().checked_add(1).expect("observer count fits"));
                Ok(Self::Group(GroupTree { baseline: baseline.children, leader: None, finished: false }))
            }
            #[cfg(target_os = "linux")]
            Choice::Best => LinuxTree::prepare(false, baseline).map(Self::Linux),
            #[cfg(target_os = "linux")]
            Choice::Walk => LinuxTree::prepare(true, baseline).map(Self::Linux),
            #[cfg(not(target_os = "linux"))]
            Choice::Best | Choice::Walk => Err(NOT_SUPPORTED),
        }
    }

    // Cgroups are Linux's (shell.md, 7.5).
    #[cfg(target_os = "linux")]
    pub(crate) fn cgroup(&self) -> Option<Fd> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(tree) => tree.cgroup(),
            Self::Group(_) => None,
        }
    }

    pub(crate) fn started(&mut self, handle: Fd) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(tree) => tree.started(handle),
            Self::Group(tree) => {
                tree.leader = Some(skein_shell::child_id(handle).expect("a live child handle names its leader"));
            }
        }
    }

    pub(crate) fn refresh(&mut self) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(tree) => tree.refresh(),
            Self::Group(_) => {}
        }
    }

    pub(crate) fn survey(&mut self, leader: Fd) -> Vec<u32> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(tree) => tree.survey(leader),
            Self::Group(tree) => {
                if skein_shell::poll_child(leader, false).is_ok_and(|exit| exit.is_none()) {
                    tree.leader.into_iter().collect()
                } else {
                    Vec::new()
                }
            }
        }
    }

    pub(crate) fn leader_exited(&self, live: &[u32]) -> Vec<u32> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(tree) => tree.leader_exited(live),
            Self::Group(tree) => live.iter().copied().filter(|pid| Some(*pid) != tree.leader).collect(),
        }
    }

    pub(crate) fn unpopulated(&mut self) -> bool {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(tree) => tree.unpopulated(),
            Self::Group(_) => true,
        }
    }

    pub(crate) fn signal(&mut self, leader: Fd, signal: Signal) -> (Vec<u32>, bool) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(tree) => tree.signal(leader, signal),
            Self::Group(tree) => {
                let id = tree.leader.expect("a started group has its leader");
                let mut delivered = Vec::new();
                if group_live(id).unwrap_or_else(|why| crate::fail(&why)) {
                    match skein_shell::signal_group(id, signal) {
                        Ok(()) => delivered.push(id),
                        Err(Error::Other(3_i32)) => {} // The group may end between probe and signal.
                        Err(error) => {
                            crate::fail(&format!("signalling process group {id} (leader {leader:?}): {error:?}"))
                        }
                    }
                }
                let forced = signal == Signal::Kill && !delivered.is_empty();
                (delivered, forced)
            }
        }
    }

    pub(crate) fn finish(&mut self, leader: Fd, expectation: Expectation, observed: bool) -> Result<Counts, String> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(tree) => tree.finish(leader, expectation, observed),
            Self::Group(tree) => tree.finish(leader, expectation, observed),
        }
    }

    pub(crate) fn abandon(&mut self, leader: Fd) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(tree) => tree.abandon(leader),
            Self::Group(tree) => {
                if tree.finished {
                    return;
                }
                let _killed = skein_shell::signal_kept_child(leader, Signal::Kill, Target::Group);
                if let Some(id) = tree.leader {
                    let _killed = skein_shell::signal_group(id, Signal::Kill);
                }
                let _settled = tree.finish(leader, Expectation::Measure, false);
            }
        }
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        match self {
            #[cfg(target_os = "linux")]
            Self::Linux(_) => {} // LinuxTree releases its registration and subreaper setting.
            Self::Group(_) => ACTIVE.set(ACTIVE.get().checked_sub(1).expect("a registered observer is released")),
        }
    }
}

#[derive(Debug)]
pub(crate) struct GroupTree {
    baseline: Resources,
    leader: Option<u32>,
    finished: bool,
}

impl GroupTree {
    fn finish(&mut self, leader: Fd, expectation: Expectation, observed: bool) -> Result<Counts, String> {
        let mut kernel = Kernel::open(Config { operations: 4 }).map_err(|error| format!("tree ring: {error}"))?;
        let clock = Clock::new();
        let until = clock.now().now.saturating_add(Duration::from_secs(2));
        if !observed {
            call(&mut kernel, Op::Wait { pidfd: leader, reap: false }, until)?;
        }
        call(&mut kernel, Op::Wait { pidfd: leader, reap: true }, until)?;
        let id = self.leader.expect("a started group has its leader");
        let lingering = expectation == Expectation::EndsWithBinary && group_live(id)?;
        loop {
            if !group_live(id)? {
                break;
            }
            if expectation != Expectation::Supervise {
                match skein_shell::signal_group(id, Signal::Kill) {
                    Ok(()) | Err(Error::Other(3_i32)) => {}
                    Err(error) => return Err(format!("killing process group {id}: {error:?}")),
                }
            }
            if clock.now().now >= until {
                return Err("tree did not settle within its kill deadline".into());
            }
            std::thread::sleep(core::time::Duration::from_millis(10));
        }
        let Done::Usage(usage) = call(&mut kernel, Op::Usage, until)? else { unreachable!("Usage returns usage") };
        self.finished = true;
        let counts = Counts {
            method: Method::Group,
            user: difference(usage.children.user, self.baseline.user),
            system: difference(usage.children.system, self.baseline.system),
            peak_rss_bytes: usage.children.peak_rss_bytes,
            peak_scope: PeakScope::LargestProcess { baseline_bytes: self.baseline.peak_rss_bytes },
        };
        if lingering {
            return Err(format!("binary exited while its process group {id} still had members"));
        }
        Ok(counts)
    }
}

fn group_live(id: u32) -> Result<bool, String> {
    skein_shell::group_live(id).map_err(|error| format!("probing process group {id}: {error:?}"))
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
pub(crate) struct LinuxTree {
    cgroup: Option<Cgroup>,
    baseline: Resources,
    ignored: BTreeSet<u32>,
    children: BTreeMap<u32, Fd>,
    // Descendants still parented beneath the leader cannot be waited on yet.
    members: BTreeMap<u32, Fd>,
    leader: Option<u32>,
    finished: bool,
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct CgroupBase {
    original: PathBuf,
    parent: PathBuf,
    observer: PathBuf,
    users: u32,
    next: u64,
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct Cgroup {
    binary: PathBuf,
    descriptor: Fd,
    events: fs::File,
}

#[cfg(target_os = "linux")]
impl LinuxTree {
    pub(crate) fn prepare(walk: bool, baseline: Usage) -> Result<Self, Error> {
        if WALK_ACTIVE.get() {
            return Err(Error::Other(16));
        }
        if ACTIVE.get() == 0 {
            let previous = skein_shell::subreaper()?;
            skein_shell::make_subreaper()?;
            SUBREAPER_BEFORE.set(previous);
            SUBREAPER_OWNED.set(true);
        }
        let ignored = children();
        let cgroup = if walk { None } else { Cgroup::prepare() };
        if cgroup.is_none() && ACTIVE.get() != 0 {
            return Err(Error::Other(16));
        }
        ACTIVE.set(ACTIVE.get().checked_add(1).expect("observer count fits"));
        WALK_ACTIVE.set(cgroup.is_none());
        Ok(Self {
            cgroup,
            baseline: baseline.children,
            ignored,
            children: BTreeMap::new(),
            members: BTreeMap::new(),
            leader: None,
            finished: false,
        })
    }

    pub(crate) fn cgroup(&self) -> Option<Fd> {
        self.cgroup.as_ref().map(|group| group.descriptor)
    }

    pub(crate) fn started(&mut self, pidfd: Fd) {
        let leader = skein_shell::child_id(pidfd).expect("a live child handle names its leader");
        self.leader = Some(leader);
    }

    pub(crate) fn refresh(&mut self) {
        for pid in children() {
            if Some(pid) == self.leader || self.ignored.contains(&pid) || self.children.contains_key(&pid) {
                continue;
            }
            if let Some(group) = &self.cgroup {
                let Ok(membership) = fs::read_to_string(format!("/proc/{pid}/cgroup")) else {
                    continue;
                };
                let path = group.binary.strip_prefix("/sys/fs/cgroup").expect("a cgroup beneath its mount");
                if !membership
                    .lines()
                    .filter_map(|line| line.strip_prefix("0::"))
                    .any(|member| Path::new(member.trim_start_matches('/')).starts_with(path))
                {
                    continue;
                }
            }
            if let Some(descriptor) = self.members.remove(&pid) {
                self.children.insert(pid, descriptor);
                continue;
            }
            match skein_shell::open_pidfd(pid) {
                Ok(descriptor) => {
                    self.children.insert(pid, descriptor);
                }
                Err(Error::Other(3_i32)) => {} // The child can disappear between listing and opening.
                Err(error) => crate::fail(&format!("opening tree member {pid}: {error:?}")),
            }
        }
    }

    /// Keeps identities below the leader before they are orphaned, including
    /// other sessions. A cgroup additionally finds every contained generation.
    fn discover(&mut self) {
        self.refresh();
        self.members.retain(|_, descriptor| {
            if exited(*descriptor) {
                skein_shell::close_keeper_fd(*descriptor);
                false
            } else {
                true
            }
        });
        let mut pending: Vec<_> =
            self.leader.into_iter().chain(self.children.keys().copied()).chain(self.members.keys().copied()).collect();
        if let Some(group) = &self.cgroup {
            pending.extend(group.members());
        }
        let mut visited = BTreeSet::new();
        while let Some(pid) = pending.pop() {
            if !visited.insert(pid) || self.ignored.contains(&pid) {
                continue;
            }
            if Some(pid) != self.leader && !self.children.contains_key(&pid) && !self.members.contains_key(&pid) {
                match skein_shell::open_pidfd(pid) {
                    Ok(descriptor) => {
                        self.members.insert(pid, descriptor);
                    }
                    Err(Error::Other(3_i32 | 22_i32)) => continue,
                    Err(error) => crate::fail(&format!("opening supervised tree member {pid}: {error:?}")),
                }
            }
            let found = process_children(pid);
            // A non-child may be reaped by its parent during proc traversal;
            // never attach a reused PID's descendants to the old identity.
            if self.members.get(&pid).is_none_or(|descriptor| !exited(*descriptor)) {
                pending.extend(found);
            }
        }
        self.refresh();
    }

    pub(crate) fn survey(&mut self, leader: Fd) -> Vec<u32> {
        self.discover();
        let mut live = Vec::new();
        if !exited(leader) {
            live.extend(self.leader);
        }
        for (&pid, &descriptor) in &self.children {
            if !exited(descriptor) {
                live.push(pid);
            }
        }
        for (&pid, &descriptor) in &self.members {
            if !exited(descriptor) {
                live.push(pid);
            }
        }
        live.sort_unstable();
        live
    }

    pub(crate) fn leader_exited(&self, live: &[u32]) -> Vec<u32> {
        live.iter().copied().filter(|pid| Some(*pid) != self.leader).collect()
    }

    pub(crate) fn unpopulated(&mut self) -> bool {
        !self.cgroup.as_mut().is_some_and(Cgroup::populated)
    }

    pub(crate) fn signal(&mut self, leader: Fd, signal: Signal) -> (Vec<u32>, bool) {
        self.discover();
        let mut delivered = Vec::new();
        // Signal descendants first: a leader's TERM handler may exit immediately.
        for (&pid, &descriptor) in self.children.iter().chain(&self.members) {
            if deliver(descriptor, signal) {
                delivered.push(pid);
            }
        }
        if deliver(leader, signal) {
            delivered.extend(self.leader);
        }
        let mut forced = signal == Signal::Kill && !delivered.is_empty();
        if signal == Signal::Kill
            && let Some(group) = &mut self.cgroup
        {
            forced |= group.populated();
        }
        if signal == Signal::Kill
            && let Some(group) = &self.cgroup
        {
            // Cover forks racing the pidfd scan; cgroup.kill is recursive.
            fs::write(group.binary.join("cgroup.kill"), b"1")
                .unwrap_or_else(|error| crate::fail(&format!("killing supervised tree: {error}")));
        }
        delivered.sort_unstable();
        (delivered, forced)
    }

    pub(crate) fn finish(&mut self, leader: Fd, expectation: Expectation, observed: bool) -> Result<Counts, String> {
        let mut kernel = Kernel::open(Config { operations: 4 }).map_err(|error| format!("tree ring: {error}"))?;
        let clock = Clock::new();
        let until = clock.now().now.saturating_add(Duration::from_secs(2));
        self.refresh();
        let mut lingering = Vec::new();
        for (&pid, &descriptor) in &self.children {
            if skein_shell::poll_child(descriptor, false).is_ok_and(|status| status.is_none()) {
                lingering.push(pid);
            }
        }
        if let Some(group) = &self.cgroup {
            if let Ok(pids) = fs::read_to_string(group.binary.join("cgroup.procs")) {
                for pid in pids.split_whitespace().filter_map(|pid| pid.parse::<u32>().ok()) {
                    if Some(pid) != self.leader {
                        lingering.push(pid);
                    }
                }
            }
            if expectation != Expectation::Supervise {
                fs::write(group.binary.join("cgroup.kill"), b"1").map_err(|error| format!("killing tree: {error}"))?;
            }
        }
        if expectation != Expectation::Supervise {
            let _signal =
                call(&mut kernel, Op::Signal { pidfd: leader, signal: Signal::Kill, to: Target::Group }, until);
        }
        // Reaping adopted children may expose another generation. Scan after every
        // batch, including an empty batch while cgroup.events remains populated.
        loop {
            self.refresh();
            let discovered: Vec<_> = self.children.iter().map(|(&pid, &fd)| (pid, fd)).collect();
            for (pid, descriptor) in discovered {
                if skein_shell::poll_child(descriptor, false).is_ok_and(|status| status.is_none()) {
                    lingering.push(pid);
                }
                if expectation != Expectation::Supervise {
                    let _signal = call(
                        &mut kernel,
                        Op::Signal { pidfd: descriptor, signal: Signal::Kill, to: Target::Child },
                        until,
                    );
                }
                call(&mut kernel, Op::Wait { pidfd: descriptor, reap: false }, until)?;
                call(&mut kernel, Op::Wait { pidfd: descriptor, reap: true }, until)?;
                call(&mut kernel, Op::Close { fd: descriptor }, until)?;
                self.children.remove(&pid);
            }
            if !observed && self.leader.is_some() {
                call(&mut kernel, Op::Wait { pidfd: leader, reap: false }, until)?;
            }
            self.refresh();
            let populated = self.cgroup.as_mut().is_some_and(Cgroup::populated);
            if self.children.is_empty() && !populated {
                break;
            }
            if let Some(group) = &self.cgroup {
                skein_shell::wait_cgroup_change(Fd::new(group.events.as_raw_fd()), 10)
                    .map_err(|error| format!("cgroup notification: {error:?}"))?;
            }
            if clock.now().now >= until {
                return Err("tree did not settle within its kill deadline".into());
            }
        }
        call(&mut kernel, Op::Wait { pidfd: leader, reap: true }, until)?;
        for descriptor in core::mem::take(&mut self.members).into_values() {
            call(&mut kernel, Op::Close { fd: descriptor }, until)?;
        }
        let counts = match &self.cgroup {
            Some(group) => group.counts()?,
            None => {
                let Done::Usage(usage) = call(&mut kernel, Op::Usage, until)? else {
                    unreachable!("Usage returns usage")
                };
                Counts {
                    method: Method::Walk,
                    user: difference(usage.children.user, self.baseline.user),
                    system: difference(usage.children.system, self.baseline.system),
                    peak_rss_bytes: usage.children.peak_rss_bytes,
                    peak_scope: PeakScope::LargestProcess { baseline_bytes: self.baseline.peak_rss_bytes },
                }
            }
        };
        self.finished = true;
        if let Some(group) = self.cgroup.take() {
            group.remove();
        }
        if expectation == Expectation::EndsWithBinary && !lingering.is_empty() {
            lingering.sort_unstable();
            lingering.dedup();
            return Err(format!("binary exited while tree processes still ran: {lingering:?}"));
        }
        Ok(counts)
    }

    pub(crate) fn abandon(&mut self, leader: Fd) {
        if self.finished {
            return;
        }
        let _settled = self.finish(leader, Expectation::Measure, false);
        // A failed ring cleanup still settles each known identity through the
        // adapter; this path is test-only and may block after SIGKILL.
        if !self.finished {
            if let Some(group) = &self.cgroup {
                let _kill = fs::write(group.binary.join("cgroup.kill"), b"1");
            }
            let _killed = skein_shell::signal_kept_child(leader, Signal::Kill, Target::Group);
            skein_shell::abandon_binary(Some(leader), &[]);
            loop {
                self.refresh();
                if self.children.is_empty() {
                    break;
                }
                for &descriptor in self.children.values() {
                    let _killed = skein_shell::signal_kept_child(descriptor, Signal::Kill, Target::Child);
                    skein_shell::abandon_binary(Some(descriptor), &[descriptor]);
                }
                self.children.clear();
            }
            if let Some(group) = self.cgroup.take() {
                group.remove();
            }
            self.finished = true;
        }
    }
}

fn difference(after: Duration, before: Duration) -> Duration {
    Duration::from_nanos(after.as_nanos().checked_sub(before.as_nanos()).expect("resource CPU never falls"))
}

#[cfg(target_os = "linux")]
fn exited(descriptor: Fd) -> bool {
    skein_shell::pidfd_exited(descriptor)
        .unwrap_or_else(|error| crate::fail(&format!("polling tree identity: {error:?}")))
}

#[cfg(target_os = "linux")]
fn deliver(descriptor: Fd, signal: Signal) -> bool {
    if exited(descriptor) {
        return false;
    }
    match skein_shell::signal_kept_child(descriptor, signal, Target::Child) {
        Ok(()) => true,
        Err(Error::NotFound | Error::Other(3_i32)) => false,
        Err(error) => crate::fail(&format!("signalling tree identity: {error:?}")),
    }
}

#[cfg(target_os = "linux")]
fn process_children(pid: u32) -> BTreeSet<u32> {
    let mut result = BTreeSet::new();
    if let Ok(tasks) = fs::read_dir(format!("/proc/{pid}/task")) {
        for task in tasks.flatten() {
            if let Ok(list) = fs::read_to_string(task.path().join("children")) {
                result.extend(list.split_whitespace().filter_map(|pid| pid.parse::<u32>().ok()));
            }
        }
    }
    result
}

#[cfg(target_os = "linux")]
fn children() -> BTreeSet<u32> {
    let mut result = BTreeSet::new();
    for task in fs::read_dir("/proc/self/task").expect("the observer has a proc task directory").flatten() {
        if let Ok(list) = fs::read_to_string(task.path().join("children")) {
            result.extend(list.split_whitespace().filter_map(|pid| pid.parse::<u32>().ok()));
        }
    }
    result
}

pub(crate) fn call(kernel: &mut Kernel, operation: Op, until: Time) -> Result<Done, String> {
    let mut submissions = Queue::with_capacity(1);
    let mut completions = Queue::<Complete>::with_capacity(1);
    submissions.push(Submit { op: Token::new(1), kind: operation });
    loop {
        kernel.submit(&mut submissions, Wait::No);
        kernel.reap(&mut completions);
        if let Some(complete) = completions.pop() {
            return complete.result.map_err(|error| format!("tree operation {:?}: {error:?}", complete.kind));
        }
        kernel.submit(&mut submissions, Wait::Until(until));
        if Clock::new().now().now >= until {
            return Err("tree operation exceeded its deadline".into());
        }
    }
}

#[cfg(target_os = "linux")]
impl Cgroup {
    fn members(&self) -> BTreeSet<u32> {
        let mut result = BTreeSet::new();
        let mut pending = vec![self.binary.clone()];
        while let Some(directory) = pending.pop() {
            if let Ok(list) = fs::read_to_string(directory.join("cgroup.procs")) {
                result.extend(list.split_whitespace().filter_map(|pid| pid.parse::<u32>().ok()));
            }
            if let Ok(entries) = fs::read_dir(directory) {
                for entry in entries.flatten() {
                    if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                        pending.push(entry.path());
                    }
                }
            }
        }
        result
    }

    fn prepare() -> Option<Self> {
        if CGROUP_BASE.with(|base| base.borrow().is_some()) {
            return CGROUP_BASE.with(|base| {
                let mut base = base.borrow_mut();
                let base = base.as_mut().expect("a prepared cgroup base");
                let binary = base.parent.join(format!("binary-{}", base.next));
                base.next = base.next.checked_add(1).expect("binary names fit");
                fs::create_dir(&binary).ok()?;
                let Ok(descriptor) = skein_shell::open_cgroup(&binary) else {
                    let _removed = fs::remove_dir(&binary);
                    return None;
                };
                let events = fs::File::open(binary.join("cgroup.events")).expect("a cgroup has its event file");
                base.users = base.users.checked_add(1).expect("observer count fits");
                Some(Self { binary, descriptor, events })
            });
        }
        let proc = fs::read_to_string("/proc/self/cgroup").ok()?;
        let relative = proc.lines().find_map(|line| line.strip_prefix("0::"))?;
        let original = Path::new("/sys/fs/cgroup").join(relative.trim_start_matches('/'));
        for ancestor in original.ancestors() {
            if !ancestor.starts_with("/sys/fs/cgroup") {
                break;
            }
            let parent = ancestor.join(format!("skein-test-{}", std::process::id()));
            if fs::create_dir(&parent).is_err() {
                continue;
            }
            let observer = parent.join("observer");
            let binary = parent.join("binary-0");
            let setup = (|| {
                fs::create_dir(&observer)?;
                fs::write(observer.join("cgroup.procs"), std::process::id().to_string())?;
                fs::write(parent.join("cgroup.subtree_control"), b"+cpu +memory")?;
                fs::create_dir(&binary)?;
                if !binary.join("memory.peak").exists() {
                    return Err(std::io::Error::other("memory controller is not delegated"));
                }
                Ok(())
            })();
            if setup.is_err() {
                let _restored = fs::write(original.join("cgroup.procs"), std::process::id().to_string());
                let _removed = fs::remove_dir(&binary);
                let _removed = fs::remove_dir(&observer);
                let _removed = fs::remove_dir(&parent);
                continue;
            }
            match skein_shell::open_cgroup(&binary) {
                Ok(descriptor) => {
                    let events = fs::File::open(binary.join("cgroup.events")).expect("a new cgroup has its event file");
                    CGROUP_BASE.with(|base| {
                        *base.borrow_mut() = Some(CgroupBase { original, parent, observer, users: 1, next: 1 });
                    });
                    return Some(Self { binary, descriptor, events });
                }
                Err(_) => {
                    let _restored = fs::write(original.join("cgroup.procs"), std::process::id().to_string());
                    let _removed = fs::remove_dir(&binary);
                    let _removed = fs::remove_dir(&observer);
                    let _removed = fs::remove_dir(&parent);
                }
            }
        }
        None
    }

    fn populated(&mut self) -> bool {
        self.events.rewind().expect("the cgroup event file rewinds");
        let mut text = String::new();
        self.events.read_to_string(&mut text).expect("the cgroup event file reads");
        text.lines().any(|line| line == "populated 1")
    }

    fn counts(&self) -> Result<Counts, String> {
        let cpu = fs::read_to_string(self.binary.join("cpu.stat")).map_err(|error| error.to_string())?;
        let value = |name: &str| -> Result<u64, String> {
            cpu.lines()
                .find_map(|line| line.strip_prefix(name).and_then(|value| value.trim().parse().ok()))
                .ok_or_else(|| format!("cpu.stat has no {name}"))
        };
        let nanos = |micros: u64| -> Result<Duration, String> {
            micros.checked_mul(1000).map(Duration::from_nanos).ok_or_else(|| "cgroup CPU overflow".into())
        };
        let peak_rss_bytes = fs::read_to_string(self.binary.join("memory.peak"))
            .map_err(|error| error.to_string())?
            .trim()
            .parse::<u64>()
            .map_err(|error| error.to_string())?;
        Ok(Counts {
            method: Method::Cgroup,
            user: nanos(value("user_usec")?)?,
            system: nanos(value("system_usec")?)?,
            peak_rss_bytes,
            peak_scope: PeakScope::Tree,
        })
    }

    fn remove(self) {
        skein_shell::close_keeper_fd(self.descriptor);
        drop(self.events);
        fs::remove_dir(&self.binary).expect("the binary's empty cgroup is removed");
        let last = CGROUP_BASE.with(|base| {
            let mut base = base.borrow_mut();
            let shared = base.as_mut().expect("a prepared cgroup base");
            shared.users = shared.users.checked_sub(1).expect("an admitted binary is removed");
            if shared.users == 0 { base.take() } else { None }
        });
        if let Some(base) = last {
            fs::write(base.original.join("cgroup.procs"), std::process::id().to_string())
                .expect("the observer returns to its original cgroup");
            fs::remove_dir(&base.observer).expect("the observer's empty cgroup is removed");
            fs::remove_dir(&base.parent).expect("the test's empty cgroup is removed");
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for LinuxTree {
    fn drop(&mut self) {
        ACTIVE.set(ACTIVE.get().checked_sub(1).expect("a registered observer is released"));
        if self.cgroup.is_none() {
            WALK_ACTIVE.set(false);
        }
        if let Some(group) = self.cgroup.take() {
            group.remove();
        }
        if ACTIVE.get() == 0 && SUBREAPER_OWNED.replace(false) {
            skein_shell::set_subreaper(SUBREAPER_BEFORE.get()).expect("the observer restores its subreaper setting");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BUSY, Choice, Containment, NOT_SUPPORTED, choose};

    #[test]
    fn containment_choice_respects_platform_and_accounting() {
        use Containment::{Best, Group, Walk};
        let cases = [
            (Group, true, false, false, 0, Ok(Choice::Group)),
            (Group, true, true, false, 0, Err(BUSY)),
            (Group, true, false, true, 1, Err(BUSY)),
            (Group, true, false, false, 1, Ok(Choice::Group)),
            (Best, false, false, false, 0, Ok(Choice::Group)),
            (Walk, false, false, false, 0, Err(NOT_SUPPORTED)),
            (Walk, true, false, true, 1, Err(BUSY)),
            (Best, true, true, true, 1, Err(BUSY)),
            (Walk, true, false, false, 1, Err(BUSY)),
            (Walk, true, true, false, 0, Ok(Choice::Walk)),
            (Best, true, false, false, 0, Ok(Choice::Best)),
            (Best, true, true, false, 1, Ok(Choice::Best)),
        ];
        for (containment, linux, subreaper, walk_active, active, expected) in cases {
            assert_eq!(
                choose(containment, linux, subreaper, walk_active, active),
                expected,
                "{containment:?}, linux={linux}, subreaper={subreaper}, walk={walk_active}, active={active}"
            );
        }
    }
}
