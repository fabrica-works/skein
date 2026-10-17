//! Owners retry a busy lock on their own timer; a killed holder frees it.
use skein_fake_machine::{Machine as Files, Opened};
use skein_io::{
    file::{Event, Request},
    file_layer::{self, FileIo},
    kernel::{Complete, Error, Fd, Op, Submit},
};
use skein_lib::{Duration, Queue, Time, Token, Wall};
use skein_sim::{Answer, Call, Config, Handle};
use skein_world::{Cut, Host, Inherited, Machine, Memory, Referee, StartupRoot, World};
const REQUEST: Token = Token::new(1);
const SIGNAL: Token = Token::new(u64::MAX);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heard {
    Locked,
    Busy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Holder,
    Waiter,
    Again,
    Pair,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Dormant,
    Lock,
    Hold,
    Stat,
    CloseFirst,
    CloseRoot,
    CloseSignal,
    Settled,
}
#[derive(Debug)]
pub struct Owner {
    io: FileIo,
    roots: Vec<Token>,
    at: usize,
    signal: Option<Fd>,
    role: Role,
    phase: Phase,
    waiting: bool,
    retry: Option<Time>,
    pub heard: Vec<Heard>,
    completions: Queue<Complete>,
    submissions: Queue<Submit>,
    events: Queue<Event>,
}
impl Owner {
    fn new(fds: &[Fd], signal: Option<Fd>, role: Role) -> Self {
        let mut io = FileIo::new(2, 4, 4, Duration::from_secs(1));
        let roots = fds.iter().map(|fd| io.adopt_root(*fd).expect("two roots at most")).collect();
        Self {
            io,
            roots,
            at: 0,
            signal,
            role,
            phase: Phase::Dormant,
            waiting: false,
            retry: None,
            heard: Vec::with_capacity(8),
            completions: Queue::with_capacity(2),
            submissions: Queue::with_capacity(2),
            events: Queue::with_capacity(2),
        }
    }
    /// One owner takes a lock twice through the same open root.
    #[must_use]
    pub fn again(root: Fd) -> Self {
        let mut owner = Self::new(&[root], None, Role::Again);
        owner.phase = Phase::Lock;
        owner
    }
    /// One owner observes a lock through two independent opens over the ring.
    #[must_use]
    pub fn pair(first: Fd, second: Fd) -> Self {
        let mut owner = Self::new(&[first, second], None, Role::Pair);
        owner.phase = Phase::Lock;
        owner
    }
    fn observe(&mut self, now: Time) {
        while let Some(event) = self.events.pop() {
            assert!(self.waiting);
            self.waiting = false;
            match event {
                Event::Locked { owner: REQUEST } => {
                    assert!(self.heard.len() < 8);
                    self.heard.push(Heard::Locked);
                    self.phase = match self.role {
                        Role::Holder => Phase::Hold,
                        Role::Again if self.heard.len() == 1 => Phase::Lock,
                        Role::Pair if self.at == 0 => {
                            self.at = 1;
                            Phase::Lock
                        }
                        Role::Waiter | Role::Again | Role::Pair => Phase::CloseRoot,
                    };
                }
                Event::Busy { owner: REQUEST } => {
                    assert!(self.heard.len() < 8);
                    self.heard.push(Heard::Busy);
                    assert!(matches!(self.role, Role::Waiter | Role::Pair));
                    self.phase = if self.role == Role::Pair { Phase::CloseFirst } else { Phase::Lock };
                    if self.role == Role::Waiter {
                        self.retry = Some(now.saturating_add(Duration::from_millis(200)));
                    }
                }
                Event::Closed { owner: REQUEST } => {
                    self.phase = if self.phase == Phase::CloseFirst { Phase::Lock } else { Phase::CloseSignal };
                }
                Event::Locked { .. }
                | Event::Busy { .. }
                | Event::Closed { .. }
                | Event::Made { .. }
                | Event::Refused { .. }
                | Event::Opened { .. }
                | Event::Loaded { .. }
                | Event::Scanned { .. }
                | Event::Stored { .. }
                | Event::Conflict { .. }
                | Event::TooLarge { .. }
                | Event::Stated { .. }
                | Event::Written { .. }
                | Event::Read { .. }
                | Event::Cut { .. }
                | Event::Synced { .. }
                | Event::Renamed { .. }
                | Event::Removed { .. }
                | Event::Listed { .. }
                | Event::Failed { .. }
                | Event::Cancelled { .. } => panic!("unexpected lock terminal {event:?}"),
            }
            assert_eq!(self.io.next_deadline(), None, "no terminal retains a deadline");
        }
    }
    fn request(&mut self) -> Option<Request> {
        match self.phase {
            Phase::Lock => Some(Request::Lock { owner: REQUEST, root: self.roots[self.at] }),
            Phase::Stat => Some(Request::Stat { owner: REQUEST, file: self.roots[0] }),
            Phase::CloseFirst => Some(Request::Close { owner: REQUEST, file: self.roots[0] }),
            Phase::CloseRoot => Some(Request::Close { owner: REQUEST, file: self.roots[self.at] }),
            Phase::CloseSignal => {
                if let Some(fd) = self.signal.take() {
                    self.waiting = true;
                    self.submissions.push(Submit { op: SIGNAL, kind: Op::Close { fd } });
                } else {
                    self.phase = Phase::Settled;
                }
                None
            }
            Phase::Dormant | Phase::Hold | Phase::Settled => None,
        }
    }
}
impl Host for Owner {
    fn iterate(&mut self, now: Time, _wall: Wall) {
        while let Some(complete) = self.completions.pop() {
            if complete.op == SIGNAL {
                assert_eq!(complete.result, Ok(skein_io::kernel::Done::Nothing));
                self.waiting = false;
                self.phase = Phase::Settled;
            } else {
                file_layer::up(&mut self.io, complete, &mut self.events, &mut self.submissions);
                self.observe(now);
            }
        }
        if self.retry.is_some_and(|deadline| deadline <= now) {
            self.retry = None;
        }
        if self.io.takes()
            && !self.waiting
            && self.retry.is_none()
            && let Some(request) = self.request()
        {
            self.waiting = true;
            file_layer::down(&mut self.io, now, request, &mut self.events, &mut self.submissions);
            self.observe(now);
        }
    }
    fn completions(&mut self) -> &mut Queue<Complete> {
        &mut self.completions
    }
    fn submissions(&mut self) -> &mut Queue<Submit> {
        &mut self.submissions
    }
    fn work_pending(&self, now: Time) -> bool {
        !self.completions.is_empty()
            || (!self.waiting
                && self.retry.is_none_or(|at| at <= now)
                && !matches!(self.phase, Phase::Dormant | Phase::Hold | Phase::Settled))
    }
    fn next_deadline(&self) -> Option<Time> {
        self.retry.or(self.io.next_deadline())
    }
    fn next_policy_deadline(&self) -> Option<Time> {
        None
    }
    fn is_empty(&self) -> bool {
        self.phase == Phase::Settled
            && self.io.open_files() == 0
            && self.io.takes()
            && self.completions.is_empty()
            && self.submissions.is_empty()
            && self.events.is_empty()
    }
    fn worst_case(&self) -> u64 {
        FileIo::worst_case(2, 4, 4, 4).expect("bounded driver")
            + Queue::<Complete>::worst_case(2).expect("completions")
            + Queue::<Submit>::worst_case(2).expect("submissions")
            + Queue::<Event>::worst_case(2).expect("events")
            + 2 * u64::try_from(size_of::<Token>()).expect("root tokens")
            + 8 * u64::try_from(size_of::<Heard>()).expect("bounded answers")
    }
    fn operations(&self) -> u32 {
        2
    }
}
fn make(inherited: &Inherited) -> Owner {
    let (name, root) = &inherited.roots[0];
    Owner::new(&[*root], Some(inherited.signal), if name.as_ref() == b"holder" { Role::Holder } else { Role::Waiter })
}
#[derive(Debug)]
pub struct FileSystem {
    pub files: Files,
}
impl Machine for FileSystem {
    fn cut(&mut self, cut: Cut, held: &[Handle], _seed: u64) {
        assert_eq!(cut, Cut::Kill);
        for handle in held {
            self.files.close(Opened::new(handle.raw()));
        }
    }
    fn open_root(&mut self, path: &[u8]) -> Result<Handle, Error> {
        assert_eq!(path, b"root-0");
        Ok(Handle::new(self.files.reopen_root(0).raw()))
    }
    fn close_root(&mut self, root: Handle) {
        self.files.close(Opened::new(root.raw()));
    }
    fn step(&mut self, call: Call, answers: &mut Queue<Answer>) {
        skein_fake_machine::step(&mut self.files, call, answers);
    }
}
#[derive(Debug)]
pub struct Judge {
    started: bool,
    peers: bool,
    wake: Option<Time>,
}
impl Judge {
    #[must_use]
    pub const fn single() -> Self {
        Self { started: false, peers: false, wake: None }
    }
}
impl Referee<Owner> for Judge {
    fn act(&mut self, _now: Time, procs: &mut [Owner]) {
        if !self.peers {
            return;
        }
        if !self.started {
            procs[0].phase = Phase::Lock;
            self.started = true;
        }
        if procs[0].heard == [Heard::Locked] && procs[1].phase == Phase::Dormant {
            procs[1].phase = Phase::Lock;
        }
        if procs[1].heard.first() == Some(&Heard::Busy) {
            if procs[0].phase == Phase::Hold {
                procs[0].phase = Phase::Stat;
            }
            if procs[0].phase == Phase::Dormant {
                procs[0].phase = Phase::CloseRoot;
            }
        }
    }
    fn observe(&mut self, now: Time, procs: &[Owner]) {
        self.wake = (self.peers
            && ((procs[0].phase == Phase::Hold && procs[1].phase == Phase::Dormant)
                || (matches!(procs[0].phase, Phase::Hold | Phase::Dormant)
                    && procs[1].heard.first() == Some(&Heard::Busy))))
        .then_some(now);
    }
    fn next_deadline(&self) -> Option<Time> {
        self.wake
    }
    fn overdue(&self, _now: Time) -> Option<String> {
        None
    }
    fn passed(&self) -> bool {
        true
    }
}
#[must_use]
pub fn world(seed: u64, peers: bool) -> World<Owner, Judge, FileSystem> {
    let mut files = Files::new();
    let root = files.lay(&[]);
    files.close(root);
    let single = if peers { None } else { Some(Handle::new(files.reopen_root(0).raw())) };
    let mut world = World::new(seed, Config::calm(), Judge { started: false, peers, wake: None }, Memory::Checked)
        .with_machine(FileSystem { files });
    if peers {
        for name in [b"holder".as_slice(), b"waiter".as_slice()] {
            world.spawn_restartable(vec![StartupRoot { name: Box::from(name), path: Box::from(&b"root-0"[..]) }], make);
        }
        world.cut(0, 1, Cut::Kill);
    } else {
        let root = single.expect("one root");
        world.spawn_root(root, Owner::again);
    }
    world
}
