use crate::{Command, Print, Refusal, command, refusal_line, render};
use alloc::boxed::Box;
use skein_io::{
    Event, Io, Request,
    kernel::{Complete, Fd, Op, Submit},
};
use skein_lib::{
    Duration, Env, List, Queue, Time, Token, Wall, Writer,
    stream::{Down, Read, Up},
};
use skein_ui::{Region, Tree};
const TAG: u64 = 1 << 62;
const BATCH: u32 = 64;
/// The application's whole terminal configuration, with no defaults.
#[derive(Debug)]
pub struct Settings {
    pub name: Box<[u8]>,
    pub longest: u32,
    pub interval: Duration,
    pub status: Option<Region>,
}
#[derive(Debug, PartialEq, Eq)]
enum Phase {
    Open,
    Closing,
    Drained,
}
#[derive(Debug, PartialEq, Eq)]
enum Output {
    Waiting,
    Writable,
}
#[derive(Debug)]
struct Input {
    line: List<u8>,
    overflow: bool,
    reading: bool,
}
/// A terminal with tagged io; its host drains commands after iteration.
#[derive(Debug)]
pub struct Terminal {
    settings: Settings,
    io: Io,
    limits: skein_io::Limits,
    input: Option<Token>,
    output: Option<Token>,
    completions: Queue<Complete>,
    submissions: Queue<Submit>,
    raw: Queue<Submit>,
    events: Queue<Event>,
    commands: Queue<Command>,
    lines: Queue<Box<[u8]>>,
    pending: Option<Box<[u8]>>,
    intake: Input,
    writing: Output,
    print: Print,
    printed: Option<Time>,
    dirty: bool,
    immediate: bool,
    after_act: bool,
    phase: Phase,
}
impl Terminal {
    #[must_use]
    pub fn new(settings: Settings, view: &skein_ui::Limits) -> Option<Self> {
        if settings.name.len() > usize::try_from(view.text_per_node).ok()? || settings.longest == 0 {
            return None;
        }
        let print = Print::new(view)?;
        let output = u32::try_from(crate::worst_case(view)?.1)
            .ok()?
            .max(settings.longest.checked_add(view.text_per_node)?.checked_add(64)?);
        let limits = skein_io::Limits {
            sockets: 2,
            names: 0,
            refusals: 1,
            intake: BATCH,
            receive: BATCH,
            output,
            sends: 1,
            accepts: 1,
            backlog: 1,
            close_timeout: Duration::from_secs(30),
            retry: Duration::from_millis(1),
        };
        Some(Self {
            intake: Input {
                line: List::with_capacity(settings.longest.checked_add(1)?),
                overflow: false,
                reading: false,
            },
            settings,
            io: Io::new(&limits),
            limits,
            input: None,
            output: None,
            completions: Queue::with_capacity(32),
            submissions: Queue::with_capacity(256),
            raw: Queue::with_capacity(256),
            events: Queue::with_capacity(32),
            commands: Queue::with_capacity(BATCH),
            lines: Queue::with_capacity(BATCH.checked_add(1)?),
            pending: None,
            writing: Output::Waiting,
            print,
            printed: None,
            dirty: true,
            immediate: false,
            after_act: false,
            phase: Phase::Open,
        })
    }
    pub fn adopt(&mut self, input: Fd, output: Fd) -> Result<(), Fd> {
        assert!(self.input.is_none() && self.output.is_none(), "adopt once");
        self.input = Some(self.io.adopt_read_pipe(input)?);
        self.output = Some(self.io.adopt_write_pipe(output, None)?);
        self.demand_output(Time::ZERO);
        Ok(())
    }
    #[must_use]
    pub fn owns(op: Token) -> bool {
        op.raw() & TAG != 0
    }
    pub fn completions(&mut self) -> &mut Queue<Complete> {
        &mut self.completions
    }
    pub fn submissions(&mut self) -> &mut Queue<Submit> {
        &mut self.submissions
    }
    pub fn commands(&mut self) -> &mut Queue<Command> {
        &mut self.commands
    }
    /// Queue a bounded application line; the host supplies room before saying it.
    pub fn say(&mut self, line: &[u8]) {
        let prefix = self.settings.name.len().checked_add(3).expect("bounded prefix");
        let length = prefix.checked_add(line.len()).expect("bounded application line");
        let mut out = Writer::new(length);
        out.put(&self.settings.name).expect("name fits");
        out.put(b": ").expect("prefix fits");
        out.put(line).expect("line fits");
        out.put(b"\n").expect("newline fits");
        self.lines.push(out.finish());
    }
    pub fn close(&mut self) {
        if self.phase == Phase::Open {
            self.say(b"closed");
            self.phase = Phase::Closing;
        }
    }
    #[must_use]
    pub fn closed(&self) -> bool {
        self.phase == Phase::Drained && self.io.is_empty() && self.submissions.is_empty()
    }
    #[must_use]
    pub fn next_deadline(&self) -> Option<Time> {
        let print = if self.dirty && self.phase == Phase::Open && self.writing == Output::Writable {
            Some(self.printed.unwrap_or(Time::ZERO).saturating_add(self.settings.interval))
        } else {
            None
        };
        match (self.io.next_deadline(), print) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
    #[must_use]
    pub fn work_pending(&self, now: Time) -> bool {
        !self.completions.is_empty()
            || !self.submissions.is_empty()
            || self.io.is_ready()
            || self.io.is_due(now)
            || (self.writing == Output::Writable
                && (self.pending.is_some() || !self.lines.is_empty() || self.due(now) || self.phase == Phase::Closing))
            || self.after_act
    }
    fn due(&self, now: Time) -> bool {
        self.phase == Phase::Open
            && (self.immediate
                || (self.dirty
                    && match self.printed {
                        None => true,
                        Some(at) => now >= at.saturating_add(self.settings.interval),
                    }))
    }
    pub fn iterate(&mut self, now: Time, tree: &Tree, changed: bool) {
        self.dirty |= changed;
        self.immediate |= core::mem::replace(&mut self.after_act, false);
        for _ in 0..self.completions.len() {
            let mut complete = self.completions.pop().expect("counted completion");
            complete.op = Token::new(complete.op.raw() & !TAG);
            if let Op::Cancel { target } = &mut complete.kind {
                *target = Token::new(target.raw() & !TAG);
            }
            skein_io::up(
                &mut self.io,
                &Env { now, wall: Wall::EPOCH, limits: self.limits },
                complete,
                &mut self.events,
                &mut self.raw,
            );
            self.events(now, tree);
        }
        for _ in 0..BATCH {
            if !self.io.is_ready() {
                break;
            }
            skein_io::resume(
                &mut self.io,
                &Env { now, wall: Wall::EPOCH, limits: self.limits },
                &mut self.events,
                &mut self.raw,
            );
            self.events(now, tree);
        }
        for _ in 0..BATCH {
            if !self.io.is_due(now) {
                break;
            }
            skein_io::fire(
                &mut self.io,
                &Env { now, wall: Wall::EPOCH, limits: self.limits },
                &mut self.events,
                &mut self.raw,
            );
            self.events(now, tree);
        }
        if self.writing == Output::Writable && self.phase != Phase::Drained {
            if let Some(bytes) = self.pending.take() {
                self.write(now, bytes);
            } else if let Some(line) = self.lines.pop() {
                self.write(now, line);
            } else if self.phase != Phase::Open {
                self.phase = Phase::Drained;
                if let Some(input) = self.input {
                    self.request(now, Request::Abort { entity: input });
                }
                if let Some(output) = self.output {
                    self.request(now, Request::Close { entity: output });
                }
            } else if self.due(now) {
                render(tree, &self.settings.name, self.settings.status, &mut self.print);
                self.printed = Some(now);
                self.dirty = false;
                self.immediate = false;
                let bytes = self.print.text.to_boxed();
                self.write(now, bytes);
            }
        }
        if self.phase == Phase::Open
            && !self.intake.reading
            && self.commands.is_empty()
            && self.lines.room() >= BATCH
            && let Some(input) = self.input
        {
            self.intake.reading = true;
            self.request(
                now,
                Request::Stream { stream: input, down: Down::Demand { read: Read::Arrived { max: BATCH }, room: 0 } },
            );
        }
        self.io.reclaim();
        for _ in 0..self.raw.len() {
            let mut submit = self.raw.pop().expect("counted submission");
            assert!(submit.op.raw() & TAG == 0, "finite terminal operation namespace");
            submit.op = Token::new(submit.op.raw() | TAG);
            if let Op::Cancel { target } = &mut submit.kind {
                *target = Token::new(target.raw() | TAG);
            }
            self.submissions.push(submit);
        }
    }
    fn request(&mut self, now: Time, request: Request) {
        skein_io::down(&mut self.io, &Env { now, wall: Wall::EPOCH, limits: self.limits }, request, &mut self.raw);
    }
    fn demand_output(&mut self, now: Time) {
        self.writing = Output::Waiting;
        let stream = self.output.expect("adopted output");
        let down = Down::Demand { read: Read::Nothing, room: self.limits.output };
        self.request(now, Request::Stream { stream, down });
    }
    fn write(&mut self, now: Time, bytes: Box<[u8]>) {
        let cap = usize::try_from(self.limits.output).expect("output fits memory");
        let bytes = if bytes.len() > cap {
            self.pending = Some(skein_lib::bytes::copy_of(bytes.get(cap..).expect("remaining line")));
            skein_lib::bytes::copy_of(bytes.get(..cap).expect("first chunk"))
        } else {
            bytes
        };
        self.request(now, Request::Stream { stream: self.output.expect("adopted output"), down: Down::Send(bytes) });
        self.demand_output(now);
    }
    fn events(&mut self, now: Time, tree: &Tree) {
        for _ in 0..self.events.len() {
            let event = self.events.pop().expect("counted event");
            match event {
                Event::Stream { up, .. } => match up {
                    Up::Bytes(bytes) => {
                        self.intake.reading = false;
                        for byte in &bytes {
                            if self.phase == Phase::Open {
                                self.byte(*byte, tree);
                            }
                        }
                    }
                    Up::Room => self.writing = Output::Writable,
                    Up::End | Up::Failed(_) => self.close(),
                },
                Event::Closed { owner } => {
                    if self.input == Some(owner) {
                        self.input = None;
                    }
                    if self.output == Some(owner) {
                        self.output = None;
                        self.phase = Phase::Drained;
                        if let Some(input) = self.input {
                            self.request(now, Request::Abort { entity: input });
                        }
                    }
                }
                Event::Failed { .. } => self.close(),
                Event::Listening { .. }
                | Event::Accepted { .. }
                | Event::Connecting { .. }
                | Event::Connected { .. }
                | Event::Output { .. }
                | Event::Spawned { .. }
                | Event::Exited { .. }
                | Event::Usage { .. }
                | Event::Resized
                | Event::Window { .. }
                | Event::Shutdown { .. } => unreachable!("terminal owns only two pipes"),
            }
        }
    }
    fn byte(&mut self, byte: u8, tree: &Tree) {
        if byte != b'\n' {
            if self.intake.line.push(byte).is_err() {
                self.intake.overflow = true;
            }
            return;
        }
        if self.intake.line.last() == Some(&b'\r') {
            self.intake.line.truncate(self.intake.line.len().saturating_sub(1));
        }
        let command = if self.intake.overflow || self.intake.line.len() > self.settings.longest {
            Command::Refused { refusal: Refusal::TooLong, number: 0 }
        } else {
            command(
                self.intake.line.as_slice(),
                if self.printed.is_some() { Some(self.print.numbers.as_slice()) } else { None },
                tree,
                self.settings.longest,
            )
        };
        self.intake.line.clear();
        self.intake.overflow = false;
        match command {
            Command::Act { .. } => {
                self.after_act = true;
                self.commands.push(command);
            }
            Command::Print => self.immediate = true,
            Command::Close => self.close(),
            Command::Nothing => {}
            Command::Refused { refusal, number } => {
                self.lines.push(refusal_line(&self.settings.name, refusal, number));
                match refusal {
                    Refusal::NoNumber | Refusal::Gone => self.immediate = true,
                    Refusal::TakesNoWords | Refusal::NoPage | Refusal::TooLong | Refusal::Unknown => {}
                }
            }
        }
    }
}
