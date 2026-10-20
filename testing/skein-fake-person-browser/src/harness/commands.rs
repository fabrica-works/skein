//! A person's child and three pipes, driven by the harness's io.
use super::{Command, Opens, Output, Stdin, substitute};
use skein_io::{
    self as io,
    kernel::{Exit, Fd, Pipe, Signal, Spawn, Target, Way},
};
use skein_lib::{
    Time, Token,
    stream::{self, Down, Up},
};
use std::{collections::VecDeque, os::unix::ffi::OsStrExt};

// A complete command transcript may include diagnostics as well as a URL.
// One MiB per output admits that transcript while bounding two MiB per person.
pub(super) const OUTPUT_BYTES: usize = 1_048_576;
const FIRST: u64 = 1 << 41;

pub(super) enum WaitFor {
    Start,
    Link,
    Navigation,
    Exit { code: i32 },
}
pub(super) struct Process {
    pub owner: Token,
    pub name: String,
    pub child: Option<Token>,
    pipes: Vec<Token>,
    closed: [bool; 3],
    ended: [bool; 2],
    pub settled: bool,
    closing_child: bool,
    stopping: bool,
    pub exit: Option<Exit>,
    input: VecDeque<Box<[u8]>>,
    pub output: [Vec<u8>; 2],
    pub stderr: Vec<u8>,
    opens: Option<Opens>,
    pub wait: Option<(Time, WaitFor)>,
    pub error: Option<String>,
    pub requests: VecDeque<io::Request>,
}
#[derive(Clone, Copy)]
pub(super) struct Context<'a> {
    pub held: Option<&'a [u8]>,
    pub root: Fd,
    pub address: &'a str,
    pub peers: &'a [(Box<[u8]>, String)],
    pub now: Time,
    pub write_cap: usize,
}
impl Process {
    pub fn new(serial: u64, command: Command, context: Context<'_>) -> Self {
        let Context { held, root, address, peers, now, write_cap } = context;
        let owner = Token::new(FIRST.checked_add(serial).expect("command identity fits"));
        let mut input = VecDeque::new();
        let mut error = None;
        if matches!(command.stdin, Stdin::Printed) {
            if let Some(line) = held {
                input.extend(line.chunks(write_cap).map(Box::from));
            } else {
                error = Some("no held printed line for stdin".into());
            }
        }
        let env = command
            .env
            .iter()
            .map(|(key, value)| {
                let mut entry = key.as_bytes().to_vec();
                entry.push(b'=');
                entry.extend_from_slice(&substitute(value.as_bytes(), Some(address), peers));
                entry.into()
            })
            .collect();
        let spawn = Spawn {
            program: command.program.as_os_str().as_bytes().into(),
            args: command.args.iter().map(|arg| substitute(arg.as_bytes(), Some(address), peers)).collect(),
            env,
            root,
            dir: b".".as_slice().into(),
            pipes: Box::new([
                Pipe { child: 0, way: Way::In, parent: None },
                Pipe { child: 1, way: Way::Out, parent: None },
                Pipe { child: 2, way: Way::Out, parent: None },
            ]),
        };
        Self {
            owner,
            name: command.program.display().to_string(),
            child: None,
            pipes: Vec::new(),
            closed: [false; 3],
            ended: [false; 2],
            settled: false,
            closing_child: false,
            stopping: false,
            exit: None,
            input,
            output: [Vec::new(), Vec::new()],
            stderr: Vec::new(),
            opens: command.opens,
            wait: Some((now.saturating_add(command.within), WaitFor::Start)),
            error,
            requests: VecDeque::from([io::Request::Spawn { owner, spawn }]),
        }
    }
    pub fn owns(&self, owner: Token) -> bool {
        owner == self.owner || self.pipes.contains(&owner)
    }
    fn down(&mut self, pipe: Token, down: Down) {
        self.requests.push_back(io::Request::Stream { stream: pipe, down });
    }
    fn read(&mut self, pipe: Token) {
        self.down(pipe, Down::Demand { read: stream::Read::Arrived { max: 32768 }, room: 0 });
    }
    fn input(&mut self) {
        let pipe = self.pipes[0];
        if let Some(bytes) = self.input.front() {
            self.down(
                pipe,
                Down::Demand {
                    read: stream::Read::Nothing,
                    room: u32::try_from(bytes.len()).expect("input chunk fits"),
                },
            );
        } else {
            self.down(pipe, Down::Finish);
        }
    }
    pub fn event(&mut self, event: io::Event) {
        match event {
            io::Event::Spawned { child, pipes, .. } => {
                assert_eq!(pipes.len(), 3);
                self.child = Some(child);
                self.pipes = pipes.into_vec();
                if self.stopping {
                    self.stop();
                } else {
                    self.input();
                    self.read(self.pipes[1]);
                    self.read(self.pipes[2]);
                }
            }
            io::Event::Exited { exit, .. } => {
                self.exit = Some(exit);
            }
            io::Event::Stream { owner, up } => {
                let at = self.pipes.iter().position(|pipe| *pipe == owner).expect("owned pipe");
                match up {
                    Up::Bytes(bytes) => {
                        assert!(at > 0, "only outputs read");
                        if at == 2 {
                            super::tail(&mut self.stderr, &bytes);
                        }
                        if self.output[at - 1].len().checked_add(bytes.len()).is_none_or(|size| size > OUTPUT_BYTES) {
                            self.error.get_or_insert_with(|| format!("output exceeds {OUTPUT_BYTES} bytes"));
                        } else {
                            self.output[at - 1].extend_from_slice(&bytes);
                        }
                        self.read(owner);
                    }
                    Up::Room => {
                        let bytes = self.input.pop_front().expect("input grant");
                        self.down(owner, Down::Send(bytes));
                        self.input();
                    }
                    Up::End => {
                        if at > 0 {
                            self.ended[at - 1] = true;
                        }
                        self.requests.push_back(io::Request::Close { entity: owner });
                    }
                    Up::Failed(error) => {
                        if !self.stopping {
                            self.error.get_or_insert_with(|| format!("pipe failed: {error:?}"));
                        }
                        self.requests.push_back(io::Request::Close { entity: owner });
                    }
                }
            }
            io::Event::Closed { owner } => {
                if owner == self.owner {
                    self.settled = true;
                } else if let Some(at) = self.pipes.iter().position(|pipe| *pipe == owner) {
                    self.closed[at] = true;
                }
            }
            io::Event::Failed { error, .. } => {
                self.error.get_or_insert_with(|| format!("io failed: {error:?}"));
            }
            other @ (io::Event::Listening { .. }
            | io::Event::Accepted { .. }
            | io::Event::Connecting { .. }
            | io::Event::Connected { .. }
            | io::Event::Output { .. }
            | io::Event::Usage { .. }
            | io::Event::Resized
            | io::Event::Window { .. }
            | io::Event::Shutdown { .. }) => panic!("unexpected command event: {other:?}"),
        }
        if self.exit.is_some()
            && self.closed.iter().all(|closed| *closed)
            && !self.closing_child
            && let Some(child) = self.child
        {
            self.closing_child = true;
            self.requests.push_back(io::Request::Close { entity: child });
        }
    }
    pub fn advance(&mut self, now: Time, navigating: bool, failed: bool) -> Progress {
        if let Some(error) = self.error.take() {
            self.wait = None;
            return Progress::Failed(error);
        }
        let Some((until, wait)) = self.wait.take() else { return Progress::Idle };
        let pending = match wait {
            WaitFor::Start => {
                if self.child.is_some() {
                    if self.has_opens() { Some(WaitFor::Link) } else { return Progress::Done }
                } else {
                    Some(WaitFor::Start)
                }
            }
            WaitFor::Link => {
                if let Some(link) = self.link() {
                    self.wait = Some((until, WaitFor::Navigation));
                    return Progress::Open(link);
                }
                if self.exit.is_some() && self.selected_ended() {
                    return Progress::Failed("exited without matching link".into());
                }
                Some(WaitFor::Link)
            }
            WaitFor::Navigation => {
                if !navigating {
                    return if failed {
                        Progress::Done
                    } else {
                        Progress::Kept { name: self.keep_name(), value: self.link().expect("opened link retained") }
                    };
                }
                Some(WaitFor::Navigation)
            }
            WaitFor::Exit { code } => match self.exit {
                Some(Exit::Code(actual)) if i32::from(actual) != code => {
                    return Progress::Failed(format!("exited with code {actual}, expected {code}"));
                }
                Some(Exit::Signal(signal)) => {
                    return Progress::Failed(format!("exited on signal {signal}, expected code {code}"));
                }
                Some(Exit::Code(_)) if self.settled => return Progress::Done,
                Some(Exit::Code(_)) | None => Some(WaitFor::Exit { code }),
            },
        };
        if let Some(wait) = pending {
            if now >= until {
                return Progress::Failed(
                    match wait {
                        WaitFor::Exit { .. } => "Exited deadline passed",
                        WaitFor::Start | WaitFor::Link | WaitFor::Navigation => {
                            "Run missing-link/start deadline passed"
                        }
                    }
                    .into(),
                );
            }
            self.wait = Some((until, wait));
        }
        Progress::Idle
    }
    pub fn opening(&self) -> bool {
        matches!(self.wait, Some((_, WaitFor::Navigation)))
    }
    pub fn link(&self) -> Option<Box<[u8]>> {
        let opens = self.opens.as_ref()?;
        let at = usize::from(opens.on == Output::Stderr);
        self.output[at]
            .split_inclusive(|byte| *byte == b'\n')
            .find(|line| line.ends_with(b"\n") && line.starts_with(&opens.prefix))
            .map(|line| {
                let line = line.strip_suffix(b"\n").expect("complete line");
                line.strip_suffix(b"\r").unwrap_or(line).into()
            })
    }
    pub fn selected_ended(&self) -> bool {
        self.opens.as_ref().is_some_and(|opens| self.ended[usize::from(opens.on == Output::Stderr)])
    }
    pub fn keep_name(&self) -> Box<[u8]> {
        self.opens.as_ref().expect("link requested").keep.clone()
    }
    pub fn has_opens(&self) -> bool {
        self.opens.is_some()
    }
    pub fn stop(&mut self) {
        self.stopping = true;
        if self.exit.is_some() {
            return;
        }
        if let Some(child) = self.child {
            if !self.closing_child {
                self.closing_child = true;
                self.requests.push_back(io::Request::Signal { child, signal: Signal::Kill, to: Target::Group });
                self.requests.push_back(io::Request::Close { entity: child });
            }
            for (at, &pipe) in self.pipes.iter().enumerate() {
                if !self.closed[at] {
                    self.requests.push_back(io::Request::Abort { entity: pipe });
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Progress {
    Idle,
    Done,
    Open(Box<[u8]>),
    Kept { name: Box<[u8]>, value: Box<[u8]> },
    Failed(String),
}

impl super::Runtime {
    pub(super) fn command_event(&self, event: &io::Event) -> bool {
        super::event_owner(event).is_some_and(|owner| self.commands.iter().flatten().any(|command| command.owns(owner)))
    }
    pub(super) fn command_action(
        &mut self,
        index: usize,
        action: super::CommandAction,
        face: &mut crate::BrowserFace<'_>,
        step: u32,
        now: Time,
    ) {
        match action {
            super::CommandAction::Run(command) => {
                if let Some(previous) = &self.commands[index]
                    && previous.exit.is_none()
                {
                    let name = previous.name.clone();
                    face.command_complete();
                    self.fail(index, step, format!("command {name}: another Run before it settled"));
                    return;
                }
                if self.commands[index].as_ref().is_some_and(|previous| !previous.settled) {
                    self.pending_commands[index] = Some(command);
                    return;
                }
                let command = Process::new(
                    self.command_serial,
                    command,
                    Context {
                        held: self.held[index].as_deref(),
                        root: self.roots[0],
                        address: self.address.as_deref().expect("server address"),
                        peers: &self.peer_addresses,
                        now,
                        write_cap: usize::try_from(self.ienv.limits.output).expect("output fits"),
                    },
                );
                self.command_serial = self.command_serial.checked_add(1).expect("command serial fits");
                self.commands[index] = Some(command);
            }
            super::CommandAction::Exit { code, within } => {
                if let Some(command) = &mut self.commands[index] {
                    command.wait = Some((now.saturating_add(within), WaitFor::Exit { code }));
                } else {
                    face.command_complete();
                    self.fail(index, step, "Exited without a command".into());
                }
            }
        }
    }
    pub(super) fn drive_commands(
        &mut self,
        faces: &mut [crate::BrowserFace<'_>],
        people: &[&mut dyn super::Play],
        now: Time,
    ) {
        for (index, face) in faces.iter_mut().enumerate() {
            if !self.closing
                && self.commands[index].as_ref().is_some_and(|previous| previous.settled)
                && let Some(command) = self.pending_commands[index].take()
            {
                self.command_action(index, super::CommandAction::Run(command), face, people[index].at(), now);
            }
            let Some(command) = &mut self.commands[index] else { continue };
            if self.closing {
                command.wait = None;
            }
            let name = command.name.clone();
            match command.advance(now, face.navigating(), face.failed()) {
                Progress::Idle => {}
                Progress::Done => face.command_complete(),
                Progress::Open(link) => face.absolute(link),
                Progress::Kept { name, value } => {
                    self.kept.push(super::Kept { person: u32::try_from(index).expect("person fits"), name, value });
                    face.command_complete();
                }
                Progress::Failed(error) => {
                    face.command_complete();
                    self.fail(index, people[index].at(), format!("command {name}: {error}"));
                }
            }
            for request in face.requests() {
                self.ask(request);
            }
        }
    }
}

#[cfg(test)]
mod tests;
