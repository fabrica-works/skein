//! The server and Chromium are children of one io loop (ui-web.md, 14.2).
use crate::BrowserFace;
use browser::boundary::{Below, Down, Event, Request};
use skein_browser::{self as browser, Browser};
use skein_fake_person::{Next, Person};
use skein_io::{
    self as io, Io,
    kernel::{self, Complete, Op, Pipe, Spawn, Submit, Way},
};
use skein_lib::{Duration, Env, Queue, Time, Token, Wall, stream};
use skein_shell::{Clock, Config, Kernel, Wait, open_root};
use std::fmt::Write as _;
use std::{collections::VecDeque, ffi::OsString, os::unix::ffi::OsStrExt, path::PathBuf};

const SERVER: Token = Token::new(10);
const PROCESS: Token = Token::new(11);
const BROWSER: Token = Token::new(12);
const SNAPSHOT: Token = Token::new(u64::MAX - 3);
const IMAGE: Token = Token::new(u64::MAX - 2);
const ROOT: Token = Token::new(u64::MAX - 1);
const SERVER_ROOT: Token = Token::new(u64::MAX);

pub struct Setup<'a> {
    pub server: PathBuf,
    pub args: Vec<OsString>,
    pub listening: &'a [u8],
    pub chromium: PathBuf,
    pub expected: Vec<Box<[u8]>>,
}
pub trait Play {
    fn next(&mut self, face: &mut BrowserFace<'_>, now: Time) -> Next<Box<[u8]>>;
    /// Current zero-based script position, for diagnostics.
    fn at(&self) -> u32 {
        0
    }
    fn server_action(&mut self) -> Option<ServerAction> {
        None
    }
}
pub enum Step {
    Person(skein_fake_person::Step<Box<[u8]>>),
    StopServer,
    StartServer,
    Title(Box<[u8]>),
    Warned(bool),
    Compose { find: skein_fake_person::Find, words: Box<[u8]> },
    Type { find: skein_fake_person::Find, words: Box<[u8]> },
    Back,
    Forward,
}
#[derive(Clone, Copy)]
pub enum ServerAction {
    Stop,
    Start,
}
pub struct Script {
    steps: VecDeque<Step>,
    person: Option<Person<Box<[u8]>>>,
    at: u32,
    action: Option<ServerAction>,
    title: Option<Box<[u8]>>,
    text: Option<(skein_fake_person::Find, Box<[u8]>, bool)>,
}
impl Script {
    #[must_use]
    pub fn new(steps: Box<[Step]>) -> Self {
        Self { steps: steps.into_vec().into(), person: None, at: 0, action: None, title: None, text: None }
    }
}
impl Play for Script {
    fn next(&mut self, face: &mut BrowserFace<'_>, now: Time) -> Next<Box<[u8]>> {
        if let Some((find, words, compose)) = self.text.take() {
            match skein_fake_person::Face::find(face, &find) {
                skein_fake_person::Found::Pending => self.text = Some((find, words, compose)),
                skein_fake_person::Found::Absent => {
                    return Next::Failed { step: self.at, why: skein_fake_person::Why::Missing { find } };
                }
                skein_fake_person::Found::Node(node) => {
                    if let Err(why) = face.text(node, words, compose) {
                        face.fail(why);
                    }
                    self.at += 1;
                }
            }
            return Next::Wait { until: None };
        }
        if let Some(title) = self.title.take() {
            match face.title(&title) {
                skein_fake_person::Found::Pending => {
                    self.title = Some(title);
                    return Next::Wait { until: None };
                }
                skein_fake_person::Found::Absent => {
                    return Next::Failed {
                        step: self.at,
                        why: skein_fake_person::Why::Missing {
                            find: skein_fake_person::Find::exactly_named(skein_ui::Role::Page, &title),
                        },
                    };
                }
                skein_fake_person::Found::Node(_) => {
                    self.at += 1;
                }
            }
        }
        if let Some(person) = &mut self.person {
            let next = person.next(face, now);
            match next {
                Next::Done => {
                    self.at += person.at();
                    self.person = None;
                }
                Next::Failed { step, why } => return Next::Failed { step: self.at + step, why },
                other @ (Next::Do(_) | Next::Wait { .. }) => return other,
            }
        }
        let Some(step) = self.steps.pop_front() else {
            return Next::Done;
        };
        match step {
            Step::Compose { find, words } => self.text = Some((find, words, true)),
            Step::Type { find, words } => self.text = Some((find, words, false)),
            Step::Back | Step::Forward => {
                face.history(matches!(step, Step::Forward));
                self.at += 1;
            }
            Step::Person(step) => {
                let mut steps = vec![step];
                while matches!(self.steps.front(), Some(Step::Person(_))) {
                    let Some(Step::Person(step)) = self.steps.pop_front() else { unreachable!("person step") };
                    steps.push(step);
                }
                self.person = Some(Person::new(steps.into()));
            }
            Step::Warned(expected) => {
                let warned = face.warned();
                if warned != expected {
                    face.fail(format!("last go warned {warned}, expected {expected}"));
                }
                self.at += 1;
            }
            Step::Title(words) => {
                self.title = Some(words);
            }
            Step::StopServer => {
                self.action = Some(ServerAction::Stop);
                self.at += 1;
            }
            Step::StartServer => {
                self.action = Some(ServerAction::Start);
                self.at += 1;
            }
        }
        Next::Wait { until: None }
    }
    fn at(&self) -> u32 {
        self.at + self.person.as_ref().map_or(0, Person::at)
    }
    fn server_action(&mut self) -> Option<ServerAction> {
        self.action.take()
    }
}
#[derive(PartialEq, Eq, Debug)]
pub enum Outcome {
    Done,
    Failed { person: u32, step: u32, report: String },
}
pub struct Harness {
    server: PathBuf,
    args: Vec<OsString>,
    listening: Box<[u8]>,
    chromium: PathBuf,
    expected: Vec<Box<[u8]>>,
}
impl Harness {
    #[must_use]
    pub fn new(setup: Setup<'_>) -> Self {
        Self {
            server: setup.server,
            args: setup.args,
            listening: setup.listening.into(),
            chromium: setup.chromium,
            expected: setup.expected,
        }
    }
    pub fn run(&mut self, people: &mut [&mut dyn Play]) -> Outcome {
        let profile = skein_scratch::Scratch::new("ui-browser");
        Runtime::new(self, profile.path(), people.len()).run(self, people)
    }
}
/// Accept only a complete listening line, including one split across reads.
#[must_use]
pub fn listening_address(stderr: &[u8], prefix: &[u8]) -> Option<String> {
    stderr
        .split_inclusive(|b| *b == b'\n')
        .filter(|line| line.ends_with(b"\n"))
        .find_map(|line| line.strip_prefix(prefix))
        .and_then(|address| std::str::from_utf8(address).ok())
        .map(|address| address.trim().to_owned())
}
#[derive(PartialEq, Eq)]
enum ServerState {
    Running,
    Stopping,
    Stopped,
    Starting,
}
struct Runtime {
    browser: Browser,
    io: Io,
    kernel: Kernel,
    clock: Clock,
    benv: Env<browser::Limits>,
    ienv: Env<io::Limits>,
    complete: Queue<Complete>,
    submits: Queue<Submit>,
    events: Queue<Event>,
    io_events: Queue<io::Event>,
    down: Queue<Down>,
    requests: VecDeque<io::Request>,
    roots: [kernel::Fd; 2],
    pipes: Option<[Token; 3]>,
    server_pipe: Option<Token>,
    children: Vec<Token>,
    closed: u32,
    spawned: u32,
    server_child: Option<Token>,
    server_state: ServerState,
    stderr: Vec<u8>,
    server_stderr: Vec<u8>,
    address: Option<String>,
    ready: bool,
    context: bool,
    opening: usize,
    opened: Vec<bool>,
    outcome: Option<Outcome>,
    capturing: u8,
    snapshot: String,
    screenshot: Option<PathBuf>,
    closing: bool,
}
impl Runtime {
    fn new(setup: &Harness, profile: &std::path::Path, players: usize) -> Self {
        let bl = browser::Limits::default();
        let il = io::Limits {
            sockets: 12,
            refusals: 8,
            intake: bl.message + 1,
            receive: 65_536,
            output: bl.command + 1,
            sends: 16,
            accepts: 1,
            backlog: 1,
            close_timeout: Duration::from_secs(2),
            names: 0,
            retry: Duration::from_millis(10),
        };
        let kernel = Kernel::open(Config { operations: io::operations(&il).expect("ring size") })
            .expect("io_uring is required for the browser suite");
        let command = browser::command::command(setup.chromium.as_os_str().as_bytes(), profile.as_os_str().as_bytes());
        let root = open_root(std::path::Path::new("/")).expect("child root");
        let server_root = open_root(std::path::Path::new("/")).expect("server root");
        let browser_spawn = Spawn {
            program: command.program,
            args: command.args.into_boxed(),
            env: command
                .env
                .iter()
                .map(|entry| {
                    let mut bytes = entry.name.to_vec();
                    bytes.push(b'=');
                    bytes.extend_from_slice(&entry.value);
                    bytes.into()
                })
                .collect(),
            root,
            dir: b".".as_slice().into(),
            pipes: command
                .pipes
                .into_iter()
                .map(|pipe| Pipe {
                    child: pipe.descriptor,
                    way: match pipe.direction {
                        browser::command::Direction::Read => Way::In,
                        browser::command::Direction::Write => Way::Out,
                    },
                    parent: None,
                })
                .collect(),
        };
        let server_spawn = Spawn {
            program: setup.server.as_os_str().as_bytes().into(),
            args: setup.args.iter().map(|arg| arg.as_bytes().into()).collect(),
            env: Box::new([]),
            root: server_root,
            dir: b".".as_slice().into(),
            pipes: Box::new([Pipe { child: 2, way: Way::Out, parent: None }]),
        };
        Self {
            browser: Browser::new(BROWSER, &bl),
            io: Io::new(&il),
            kernel,
            clock: Clock::new(),
            benv: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: bl },
            ienv: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: il },
            complete: Queue::with_capacity(256),
            submits: Queue::with_capacity(256),
            events: Queue::with_capacity(256),
            io_events: Queue::with_capacity(256),
            down: Queue::with_capacity(256),
            requests: VecDeque::from([
                io::Request::Spawn { owner: SERVER, spawn: server_spawn },
                io::Request::Spawn { owner: PROCESS, spawn: browser_spawn },
            ]),
            roots: [server_root, root],
            pipes: None,
            server_pipe: None,
            children: Vec::new(),
            closed: 0,
            spawned: 0,
            server_child: None,
            server_state: ServerState::Running,
            stderr: Vec::new(),
            server_stderr: Vec::new(),
            address: None,
            ready: false,
            context: false,
            opening: 0,
            opened: vec![false; players],
            outcome: None,
            capturing: 0,
            snapshot: String::new(),
            screenshot: None,
            closing: false,
        }
    }
    fn ask(&mut self, request: Request) {
        browser::down(&mut self.browser, &self.benv, request, &mut self.events, &mut self.down);
    }
    fn read_server(&mut self) {
        self.requests.push_back(io::Request::Stream {
            stream: self.server_pipe.expect("server pipe"),
            down: stream::Down::Demand { read: stream::Read::Arrived { max: 32768 }, room: 0 },
        });
    }
    fn fail(&mut self, person: usize, step: u32, report: String) {
        if self.outcome.is_none() {
            self.outcome = Some(Outcome::Failed { person: u32::try_from(person).expect("person fits"), step, report });
        }
    }
    fn io_event(&mut self, event: io::Event, setup: &Harness) {
        match event {
            io::Event::Spawned { owner, child, pipes } => {
                self.children.push(child);
                self.spawned += 1;
                if owner == SERVER {
                    self.server_child = Some(child);
                    self.server_pipe = pipes.first().copied();
                    self.read_server();
                } else {
                    let [commands, replies, errors] = pipes.as_ref() else { panic!("Chromium pipes") };
                    self.pipes = Some([*commands, *replies, *errors]);
                    self.submits.push(Submit { op: ROOT, kind: Op::Close { fd: self.roots[1] } });
                }
            }
            io::Event::Stream { owner, up } => {
                if Some(owner) == self.server_pipe {
                    if let stream::Up::Bytes(bytes) = up {
                        tail(&mut self.server_stderr, &bytes);
                        if let Some(address) = listening_address(&self.server_stderr, &setup.listening) {
                            self.address = Some(address);
                            self.server_state = ServerState::Running;
                        }
                        self.read_server();
                    }
                    return;
                }
                let Some([commands, replies, errors]) = self.pipes else { return };
                let below = if owner == commands {
                    Below::Commands(up)
                } else if owner == replies {
                    if let stream::Up::Bytes(bytes) = &up {
                        self.context |= bytes.windows(16).any(|part| part == b"browserContextId");
                    }
                    Below::Replies(up)
                } else if owner == errors {
                    if let stream::Up::Bytes(bytes) = &up {
                        tail(&mut self.stderr, bytes);
                    }
                    Below::Errors(up)
                } else {
                    return;
                };
                browser::up(&mut self.browser, &self.benv, below, &mut self.events, &mut self.down);
            }
            io::Event::Exited { owner, exit } => {
                if !(self.closing || owner == SERVER && self.server_state == ServerState::Stopping) {
                    self.fail(0, 0, format!("child {owner:?} exited: {exit:?}"));
                }
                if owner == SERVER {
                    self.children.retain(|child| Some(*child) != self.server_child);
                }
                if owner == PROCESS {
                    if let Some(pipes) = self.pipes {
                        for entity in pipes {
                            self.requests.push_back(io::Request::Close { entity });
                        }
                    }
                } else if let Some(entity) = self.server_pipe {
                    self.requests.push_back(io::Request::Close { entity });
                }
            }
            io::Event::Closed { owner } => {
                if owner == PROCESS || owner == SERVER {
                    self.closed += 1;
                    if owner == SERVER {
                        self.server_child = None;
                        self.server_state = ServerState::Stopped;
                    }
                }
            }
            io::Event::Failed { owner, error } => self.fail(0, 0, format!("io {owner:?}: {error:?}")),
            io::Event::Listening { .. }
            | io::Event::Accepted { .. }
            | io::Event::Connecting { .. }
            | io::Event::Connected { .. }
            | io::Event::Output { .. }
            | io::Event::Usage { .. }
            | io::Event::Resized
            | io::Event::Window { .. }
            | io::Event::Shutdown { .. } => {}
        }
    }
    fn event(&mut self, event: &Event, setup: &Harness) {
        match event {
            Event::Ready { .. } => {
                self.ready = true;
            }
            Event::Opened { page } => {
                let index = usize::try_from(page.raw() - 100).expect("page index");
                if let Some(opened) = self.opened.get_mut(index) {
                    *opened = true;
                }
                self.opening += 1;
                self.context = false;
                if self.opening < self.opened.len() {
                    self.ask(Request::Person { person: person(self.opening) });
                }
            }
            Event::Trouble { page, trouble, text } => {
                if !setup.expected.iter().any(|expected| {
                    !expected.is_empty() && text.windows(expected.len()).any(|part| part == expected.as_ref())
                }) {
                    self.fail(
                        usize::try_from(page.raw().saturating_sub(100)).unwrap_or(0),
                        0,
                        format!("unexpected {trouble:?}: {}", String::from_utf8_lossy(text)),
                    );
                }
            }
            Event::Snapshot { op: SNAPSHOT, text } => {
                self.snapshot = String::from_utf8_lossy(text).into_owned();
                self.capturing = 2;
                self.ask(Request::Screenshot { page: self.failure_page(), op: IMAGE });
            }
            Event::Screenshot { op: IMAGE, png } => {
                let path = std::env::temp_dir().join(format!(
                    "skein-ui-failure-{}-{}.png",
                    std::process::id(),
                    self.clock.now().now.as_nanos()
                ));
                match std::fs::write(&path, png) {
                    Ok(()) => self.screenshot = Some(path),
                    Err(error) => {
                        write!(self.snapshot, "\nscreenshot failed: {error}").expect("string write");
                    }
                }
                self.capturing = 3;
            }
            Event::Refused { op, why } if *op == SNAPSHOT || *op == IMAGE => {
                write!(self.snapshot, "\ncapture refused: {why:?}").expect("string write");
                self.capturing = 3;
            }
            Event::Warned { .. }
            | Event::Found { .. }
            | Event::Met { .. }
            | Event::Missed { .. }
            | Event::Done { .. }
            | Event::Refused { .. }
            | Event::Snapshot { .. }
            | Event::Screenshot { .. }
            | Event::Closed { .. } => {}
        }
    }
    fn failure_page(&self) -> Token {
        match &self.outcome {
            Some(Outcome::Failed { person, .. }) => page(usize::try_from(*person).expect("person fits")),
            Some(Outcome::Done) | None => page(0),
        }
    }
    fn close(&mut self) {
        self.closing = true;
        self.submits.push(Submit { op: SERVER_ROOT, kind: Op::Close { fd: self.roots[0] } });
        self.ask(Request::Close { entity: BROWSER });
        for child in &self.children {
            self.requests.push_back(io::Request::Signal {
                child: *child,
                signal: kernel::Signal::Terminate,
                to: kernel::Target::Child,
            });
        }
    }
    fn server_action(&mut self, action: ServerAction, setup: &Harness) {
        match action {
            ServerAction::Stop => {
                assert!(self.server_state == ServerState::Running, "server running before stop");
                self.server_state = ServerState::Stopping;
                self.requests.push_back(io::Request::Signal {
                    child: self.server_child.expect("server is running"),
                    signal: kernel::Signal::Terminate,
                    to: kernel::Target::Child,
                });
            }
            ServerAction::Start => {
                assert!(self.server_state == ServerState::Stopped, "server stopped before start");
                self.server_state = ServerState::Starting;
                self.server_stderr.clear();
                let mut args = setup.args.clone();
                *args.first_mut().expect("server address argument") =
                    self.address.as_ref().expect("listening address").into();
                self.requests.push_back(io::Request::Spawn {
                    owner: SERVER,
                    spawn: Spawn {
                        program: setup.server.as_os_str().as_bytes().into(),
                        args: args.iter().map(|arg| arg.as_bytes().into()).collect(),
                        env: Box::new([]),
                        root: self.roots[0],
                        dir: b".".as_slice().into(),
                        pipes: Box::new([Pipe { child: 2, way: Way::Out, parent: None }]),
                    },
                });
            }
        }
    }
    fn play(
        &mut self,
        people: &mut [&mut dyn Play],
        faces: &mut [BrowserFace<'_>],
        done: &mut [bool],
        now: Time,
        setup: &Harness,
    ) {
        if matches!(self.server_state, ServerState::Stopping | ServerState::Starting) {
            return;
        }
        for (index, (play, face)) in people.iter_mut().zip(faces).enumerate() {
            if !self.opened[index] || done[index] {
                continue;
            }
            if let Some(why) = face.failure() {
                self.fail(index, play.at().saturating_sub(1), why);
                break;
            }
            for request in face.requests() {
                self.ask(request);
            }
            if face.pending() {
                continue;
            }
            match play.next(face, now) {
                Next::Do(doing) => {
                    if let Err(why) = face.perform(doing) {
                        self.fail(index, play.at().saturating_sub(1), why);
                        break;
                    }
                }
                Next::Wait { .. } => {}
                Next::Done => done[index] = true,
                Next::Failed { step, why } => {
                    self.fail(index, step, format!("{why:?}"));
                    break;
                }
            }
            if let Some(action) = play.server_action() {
                self.server_action(action, setup);
            }
            for request in face.requests() {
                self.ask(request);
            }
            if matches!(self.server_state, ServerState::Stopping | ServerState::Starting) {
                break;
            }
        }
        if done.iter().all(|done| *done) {
            self.outcome = Some(Outcome::Done);
        }
    }
    fn tick(&mut self, setup: &Harness, faces: &mut [BrowserFace<'_>], now: Time) -> bool {
        let mut opened = false;
        while self.io.is_ready() {
            io::resume(&mut self.io, &self.ienv, &mut self.io_events, &mut self.submits);
        }
        while let Some(complete) = self.complete.pop() {
            if complete.op != ROOT && complete.op != SERVER_ROOT {
                io::up(&mut self.io, &self.ienv, complete, &mut self.io_events, &mut self.submits);
            }
        }
        while self.io.is_due(now) {
            io::fire(&mut self.io, &self.ienv, &mut self.io_events, &mut self.submits);
        }
        while let Some(event) = self.io_events.pop() {
            self.io_event(event, setup);
        }
        if self.pipes.is_some() {
            for _ in 0..32 {
                if !self.browser.work_pending() && self.browser.next_deadline().is_none_or(|at| at > now) {
                    break;
                }
                browser::fire(&mut self.browser, &self.benv, now, &mut self.events, &mut self.down);
            }
        }
        while let Some(event) = self.events.pop() {
            for face in faces.iter_mut() {
                BrowserFace::event(face, &event);
            }
            if let Event::Opened { .. } = event {
                opened = true;
            }
            self.event(&event, setup);
        }
        opened
    }
    fn run(mut self, setup: &Harness, people: &mut [&mut dyn Play]) -> Outcome {
        let deadline = self.clock.now().now.saturating_add(Duration::from_secs(20));
        let mut faces = Vec::new();
        let mut done = vec![false; people.len()];
        let mut began = false;
        let mut opening_page = false;
        loop {
            self.kernel.reap(&mut self.complete);
            let time = self.clock.now();
            self.benv.now = time.now;
            self.benv.wall = time.wall;
            self.ienv.now = time.now;
            self.ienv.wall = time.wall;
            if time.now >= deadline {
                self.fail(0, 0, "browser harness deadline passed".into());
                if !self.closing {
                    self.close();
                }
            }
            if self.tick(setup, &mut faces, time.now) {
                opening_page = false;
            }
            if !began && self.ready && self.address.is_some() {
                began = true;
                for index in 0..people.len() {
                    faces.push(BrowserFace::new(page(index), self.address.clone().expect("listening")));
                }
                if people.is_empty() {
                    self.outcome = Some(Outcome::Done);
                } else {
                    self.ask(Request::Person { person: person(0) });
                }
            }
            if began && self.context && !opening_page && self.opening < people.len() {
                opening_page = true;
                self.ask(Request::Page {
                    person: person(self.opening),
                    page: page(self.opening),
                    url: format!("http://{}/", self.address.as_ref().expect("listening")).into_bytes().into(),
                });
            }
            if self.outcome.is_none() {
                self.play(people, &mut faces, &mut done, time.now, setup);
            }
            if self.outcome.is_some() && !self.closing {
                if matches!(self.outcome, Some(Outcome::Failed { .. }))
                    && self.opened.iter().any(|open| *open)
                    && self.capturing == 0
                {
                    self.capturing = 1;
                    self.ask(Request::Snapshot { page: self.failure_page(), op: SNAPSHOT });
                } else if (self.capturing == 0 && !opening_page) || self.capturing == 3 {
                    self.close();
                }
            }
            while let Some(down) = self.down.pop() {
                let [commands, replies, errors] = self.pipes.expect("browser spawned");
                let (pipe, down) = match down {
                    Down::Commands(d) => (commands, d),
                    Down::Replies(d) => (replies, d),
                    Down::Errors(d) => (errors, d),
                };
                self.requests.push_back(io::Request::Stream { stream: pipe, down });
            }
            while self.io.takes() {
                let Some(request) = self.requests.pop_front() else { break };
                io::down(&mut self.io, &self.ienv, request, &mut self.submits);
            }
            self.io.reclaim();
            if self.closing && self.closed == self.spawned && self.io.is_empty() && self.kernel.in_flight() == 0 {
                break;
            }
            let pending = self.io.is_ready()
                || !self.complete.is_empty()
                || !self.io_events.is_empty()
                || !self.events.is_empty()
                || !self.down.is_empty()
                || !self.requests.is_empty()
                || self.browser.work_pending();
            let until = self
                .io
                .next_deadline()
                .into_iter()
                .chain(self.browser.next_deadline())
                .fold(time.now.saturating_add(Duration::from_millis(10)), Time::min);
            self.kernel.submit(&mut self.submits, if pending { Wait::No } else { Wait::Until(until) });
        }
        let mut outcome = self.outcome.expect("terminal outcome");
        if let Outcome::Failed { report, .. } = &mut outcome {
            write!(
                report,
                "\nSnapshot:\n{}\nScreenshot: {}\nChromium stderr:\n{}\nServer stderr:\n{}",
                self.snapshot,
                self.screenshot.as_ref().map_or_else(|| "unavailable".into(), |p| p.display().to_string()),
                String::from_utf8_lossy(&self.stderr),
                String::from_utf8_lossy(&self.server_stderr)
            )
            .expect("string write");
        }
        outcome
    }
}
fn page(index: usize) -> Token {
    Token::new(100 + u64::try_from(index).expect("person fits"))
}
fn person(index: usize) -> Token {
    Token::new(100_000 + u64::try_from(index).expect("person fits"))
}
fn tail(buffer: &mut Vec<u8>, bytes: &[u8]) {
    buffer.extend_from_slice(bytes);
    if buffer.len() > 32768 {
        buffer.drain(..buffer.len() - 32768);
    }
}
