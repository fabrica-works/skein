//! The directory descriptor is the cursor between page requests (io.md, 5).
use skein_fake_machine::{Item, Machine as Files};
use skein_io::{
    file::{Entry, Event, Request},
    file_layer::{self, FileIo},
    kernel::{Complete, Fd, Submit},
};
use skein_lib::{Duration, Queue, Time, Token, Wall};
use skein_sim::{Answer, Call, Config, Handle};
use skein_world::{Host, Machine, Memory, Referee, World};
const OPEN: Token = Token::new(1);
const PAGE: Token = Token::new(2);
const CLOSE: Token = Token::new(3);
const ROOT: Token = Token::new(4);
pub const ENTRIES: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Open,
    Page,
    Close,
    Root,
    Settled,
}
#[derive(Debug)]
pub struct Owner {
    io: FileIo,
    root: Token,
    directory: Option<Token>,
    max: u32,
    total: u32,
    phase: Phase,
    waiting: bool,
    pub pages: Vec<Box<[Entry]>>,
    completions: Queue<Complete>,
    submissions: Queue<Submit>,
    events: Queue<Event>,
}
impl Owner {
    #[must_use]
    pub fn new(root: Fd, max: u32, total: u32) -> Self {
        let mut io = FileIo::new(2, 16, ENTRIES, Duration::from_secs(1));
        let root = io.adopt_root(root).expect("startup root");
        Self {
            io,
            root,
            directory: None,
            max,
            total,
            phase: Phase::Open,
            waiting: false,
            pages: Vec::with_capacity(usize::try_from(total / max + 1).expect("bounded page count")),
            completions: Queue::with_capacity(2),
            submissions: Queue::with_capacity(2),
            events: Queue::with_capacity(2),
        }
    }
    fn observe(&mut self) {
        while let Some(event) = self.events.pop() {
            assert!(self.waiting, "one terminal per request");
            self.waiting = false;
            match event {
                Event::Opened { owner: OPEN, file, .. } => {
                    self.directory = Some(file);
                    self.phase = Phase::Page;
                }
                Event::Listed { owner: PAGE, entries } => {
                    assert!(entries.len() <= usize::try_from(self.max).expect("page max fits"));
                    self.phase = if entries.len() < usize::try_from(self.max).expect("page max fits") {
                        Phase::Close
                    } else {
                        Phase::Page
                    };
                    self.pages.push(entries);
                }
                Event::Closed { owner: CLOSE } => self.phase = Phase::Root,
                Event::Closed { owner: ROOT } => self.phase = Phase::Settled,
                Event::Made { .. }
                | Event::Locked { .. }
                | Event::Busy { .. }
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
                | Event::Closed { .. }
                | Event::Failed { .. }
                | Event::Cancelled { .. } => panic!("unexpected page terminal {event:?}"),
            }
        }
    }
}
impl Host for Owner {
    fn iterate(&mut self, now: Time, _wall: Wall) {
        while let Some(complete) = self.completions.pop() {
            file_layer::up(&mut self.io, complete, &mut self.events, &mut self.submissions);
            self.observe();
        }
        if self.io.is_due(now) {
            file_layer::expire(&mut self.io, now, &mut self.submissions);
        }
        if self.io.takes() && !self.waiting {
            let request = match self.phase {
                Phase::Open => Some(Request::OpenDirectory {
                    owner: OPEN,
                    root: self.io.descriptor(self.root).expect("root open"),
                    name: Box::from(&b"cache"[..]),
                    no_follow: true,
                }),
                Phase::Page => Some(Request::ListPage {
                    owner: PAGE,
                    directory: self.directory.expect("directory open"),
                    max: self.max,
                }),
                Phase::Close => {
                    Some(Request::Close { owner: CLOSE, file: self.directory.take().expect("directory open") })
                }
                Phase::Root => Some(Request::Close { owner: ROOT, file: self.root }),
                Phase::Settled => None,
            };
            if let Some(request) = request {
                self.waiting = true;
                file_layer::down(&mut self.io, now, request, &mut self.events, &mut self.submissions);
                self.observe();
            }
        }
    }
    fn completions(&mut self) -> &mut Queue<Complete> {
        &mut self.completions
    }
    fn submissions(&mut self) -> &mut Queue<Submit> {
        &mut self.submissions
    }
    fn work_pending(&self, _now: Time) -> bool {
        !self.completions.is_empty() || (self.io.takes() && !self.waiting && self.phase != Phase::Settled)
    }
    fn next_deadline(&self) -> Option<Time> {
        self.io.next_deadline()
    }
    fn next_policy_deadline(&self) -> Option<Time> {
        None
    }
    fn is_empty(&self) -> bool {
        self.phase == Phase::Settled
            && self.io.open_files() == 0
            && self.io.takes()
            && self.submissions.is_empty()
            && self.completions.is_empty()
            && self.events.is_empty()
    }
    fn worst_case(&self) -> u64 {
        FileIo::worst_case(2, 16, ENTRIES, 16).expect("bounded file driver")
            + Queue::<Complete>::worst_case(2).expect("completion cells")
            + Queue::<Submit>::worst_case(2).expect("submission cells")
            + Queue::<Event>::worst_case(2).expect("event cells")
            + u64::from(self.total) * (u64::try_from(size_of::<Entry>()).expect("entry cell") + 255)
            + u64::from(self.total / self.max + 1) * u64::try_from(size_of::<Box<[Entry]>>()).expect("page cell")
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
    fn step(&mut self, call: Call, answers: &mut Queue<Answer>) {
        skein_fake_machine::step(&mut self.files, call, answers);
    }
}
#[derive(Debug)]
pub struct Judge;
impl Referee<Owner> for Judge {
    fn act(&mut self, _now: Time, _procs: &mut [Owner]) {}
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
#[must_use]
pub fn names(n: u32) -> Vec<Vec<u8>> {
    (0..n).map(|index| format!("file-{index:02}").into_bytes()).collect()
}
#[must_use]
pub fn world(seed: u64, n: u32, max: u32) -> World<Owner, Judge, FileSystem> {
    let mut files = Files::new();
    let mut items = vec![Item::directory(b"cache")];
    for name in names(n) {
        let mut path = b"cache/".to_vec();
        path.extend(name);
        items.push(Item::file(&path, b"entry"));
    }
    let root = Handle::new(files.lay(&items).raw());
    let mut world = World::new(seed, Config::calm(), Judge, Memory::Checked).with_machine(FileSystem { files });
    world.spawn_root(root, |fd| Owner::new(fd, max, n));
    world
}
