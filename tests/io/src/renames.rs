//! Rename durability checked by restarting the owner at every operation.
use skein_fake_machine::{Item, Machine as Files, Opened};
use skein_io::{
    file::{Event, Request},
    file_layer::{self, FileIo},
    kernel::{Complete, Error, Fd, Op, Submit},
};
use skein_lib::{Duration, Queue, Time, Token, Wall};
use skein_sim::{Answer, Call, Config, Handle};
use skein_world::{Cut, Host, Inherited, Machine, Memory, Referee, StartupRoot, World};

const OPEN_STAGE: Token = Token::new(1);
const OPEN_CACHE: Token = Token::new(2);
const RENAME: Token = Token::new(3);
const SYNC: Token = Token::new(4);
const LIST_STAGE: Token = Token::new(5);
const LIST_CACHE: Token = Token::new(6);
const CLOSE_STAGE: Token = Token::new(7);
const CLOSE_CACHE: Token = Token::new(8);
const CLOSE_ROOT: Token = Token::new(9);
const CLOSE_SIGNAL: Token = Token::new(u64::MAX);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    Source,
    Destination,
}

#[derive(Debug)]
pub struct Owner {
    io: FileIo,
    root: Token,
    stage: Option<Token>,
    cache: Option<Token>,
    signal: Option<Fd>,
    phase: u64,
    waiting: bool,
    rename: bool,
    issued: u32,
    pub synced_at: Option<u32>,
    pub found: Option<Found>,
    completions: Queue<Complete>,
    submissions: Queue<Submit>,
    events: Queue<Event>,
}

impl Owner {
    #[must_use]
    pub fn new(root: Fd, signal: Option<Fd>, rename: bool) -> Self {
        let mut io = FileIo::new(3, 16, 4, Duration::from_secs(1));
        let root = io.adopt_root(root).expect("startup root fits");
        Self {
            io,
            root,
            stage: None,
            cache: None,
            signal,
            phase: 1,
            waiting: false,
            rename,
            issued: 0,
            synced_at: None,
            found: None,
            completions: Queue::with_capacity(2),
            submissions: Queue::with_capacity(2),
            events: Queue::with_capacity(2),
        }
    }

    fn fd(&self, token: Token) -> Fd {
        self.io.descriptor(token).expect("directory is open")
    }

    fn observe(&mut self) {
        while let Some(event) = self.events.pop() {
            assert!(self.waiting);
            self.waiting = false;
            match event {
                Event::Opened { owner: OPEN_STAGE, file, .. } => {
                    self.stage = Some(file);
                    self.phase = 2;
                }
                Event::Opened { owner: OPEN_CACHE, file, .. } => {
                    self.cache = Some(file);
                    self.phase = if self.rename { 3 } else { 5 };
                }
                Event::Renamed { owner: RENAME } => self.phase = 4,
                Event::Synced { owner: SYNC } => {
                    self.synced_at = Some(self.issued);
                    self.phase = 5;
                }
                Event::Listed { owner, entries } => {
                    assert!(owner == LIST_STAGE || owner == LIST_CACHE);
                    for entry in entries {
                        assert_eq!(entry.name.as_ref(), b"index");
                        assert_eq!(self.found, None, "the move never leaves both names");
                        self.found = Some(if owner == LIST_STAGE { Found::Source } else { Found::Destination });
                    }
                    if owner == LIST_STAGE {
                        self.phase = 6;
                    } else {
                        assert!(self.found.is_some(), "the move never loses both names");
                        self.phase = 7;
                    }
                }
                Event::Closed { owner } => {
                    assert!([CLOSE_STAGE, CLOSE_CACHE, CLOSE_ROOT].contains(&owner));
                    self.phase = self.phase.checked_add(1).expect("bounded scenario state");
                }
                Event::Made { .. }
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
                | Event::Failed { .. }
                | Event::Cancelled { .. } => panic!("unexpected rename terminal {event:?}"),
            }
        }
    }
}

impl Host for Owner {
    fn iterate(&mut self, now: Time, _wall: Wall) {
        while let Some(complete) = self.completions.pop() {
            if complete.op == CLOSE_SIGNAL {
                assert!(complete.result.is_ok());
                self.phase = 11;
                self.waiting = false;
            } else {
                file_layer::up(&mut self.io, complete, &mut self.events, &mut self.submissions);
                self.observe();
            }
        }
        if self.io.is_due(now) {
            file_layer::expire(&mut self.io, now, &mut self.submissions);
        }
        if self.io.takes() && !self.waiting {
            let owner = Token::new(self.phase);
            let request = match self.phase {
                1 | 2 => Some(Request::OpenDirectory {
                    owner,
                    root: self.fd(self.root),
                    name: Box::from(if self.phase == 1 { b"staging".as_slice() } else { b"cache".as_slice() }),
                    no_follow: true,
                }),
                3 => Some(Request::Rename {
                    owner,
                    from_root: self.fd(self.stage.expect("bounded scenario state")),
                    from: Box::from(&b"index"[..]),
                    to_root: self.fd(self.cache.expect("bounded scenario state")),
                    to: Box::from(&b"index"[..]),
                }),
                4 => Some(Request::SyncDirectory { owner, root: self.fd(self.cache.expect("bounded scenario state")) }),
                5 | 6 => Some(Request::List {
                    owner,
                    root: self.fd(if self.phase == 5 {
                        self.stage.expect("bounded scenario state")
                    } else {
                        self.cache.expect("bounded scenario state")
                    }),
                }),
                7 => Some(Request::Close { owner, file: self.stage.take().expect("bounded scenario state") }),
                8 => Some(Request::Close { owner, file: self.cache.take().expect("bounded scenario state") }),
                9 => Some(Request::Close { owner, file: self.root }),
                10 => {
                    if let Some(fd) = self.signal.take() {
                        self.submissions.push(Submit { op: CLOSE_SIGNAL, kind: Op::Close { fd } });
                        self.waiting = true;
                    } else {
                        self.phase = 11;
                    }
                    None
                }
                11 => None,
                other => panic!("bad phase {other}"),
            };
            if let Some(request) = request {
                self.waiting = true;
                file_layer::down(&mut self.io, now, request, &mut self.events, &mut self.submissions);
                self.observe();
            }
        }
        self.issued += self.submissions.len();
    }
    fn completions(&mut self) -> &mut Queue<Complete> {
        &mut self.completions
    }
    fn submissions(&mut self) -> &mut Queue<Submit> {
        &mut self.submissions
    }
    fn work_pending(&self, _now: Time) -> bool {
        !self.completions.is_empty() || (self.io.takes() && !self.waiting && self.phase < 11)
    }
    fn next_deadline(&self) -> Option<Time> {
        self.io.next_deadline()
    }
    fn next_policy_deadline(&self) -> Option<Time> {
        None
    }
    fn is_empty(&self) -> bool {
        self.phase == 11
            && self.io.open_files() == 0
            && self.io.takes()
            && self.submissions.is_empty()
            && self.completions.is_empty()
            && self.events.is_empty()
    }
    fn worst_case(&self) -> u64 {
        FileIo::worst_case(3, 16, 4, 16).expect("bounded scenario state")
            + Queue::<Complete>::worst_case(2).expect("bounded scenario state")
            + Queue::<Submit>::worst_case(2).expect("bounded scenario state")
            + Queue::<Event>::worst_case(2).expect("bounded scenario state")
    }
    fn operations(&self) -> u32 {
        2
    }
}

#[derive(Debug)]
pub struct FileSystem {
    pub files: Files,
}
impl Machine for FileSystem {
    fn cut(&mut self, cut: Cut, held: &[Handle], seed: u64) {
        for handle in held {
            self.files.close(Opened::new(handle.raw()));
        }
        match cut {
            Cut::Kill => {}
            Cut::PowerLoss => self.files.crash(seed),
        }
    }
    fn open_root(&mut self, _path: &[u8]) -> Result<Handle, Error> {
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
}
impl Referee<Owner> for Judge {
    fn act(&mut self, _now: Time, procs: &mut [Owner]) {
        if !self.started {
            procs[0].rename = true;
            self.started = true;
        }
    }
    fn observe(&mut self, _now: Time, _procs: &[Owner]) {}
    fn next_deadline(&self) -> Option<Time> {
        None
    }
    fn overdue(&self, _now: Time) -> Option<String> {
        None
    }
    fn passed(&self) -> bool {
        true
    }
}
fn make(inherited: &Inherited) -> Owner {
    Owner::new(inherited.roots[0].1, Some(inherited.signal), false)
}
#[must_use]
pub fn world(seed: u64) -> World<Owner, Judge, FileSystem> {
    let mut files = Files::new();
    let root = files.lay(&[
        Item::directory(b"staging"),
        Item::directory(b"cache"),
        Item::file(b"staging/index", b"whole index"),
    ]);
    files.close(root);
    let mut world =
        World::new(seed, Config::calm(), Judge { started: false }, Memory::Checked).with_machine(FileSystem { files });
    world.spawn_restartable(vec![StartupRoot { name: Box::from(&b"root"[..]), path: Box::from(&b"root"[..]) }], make);
    world
}
