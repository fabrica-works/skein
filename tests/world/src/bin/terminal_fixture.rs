//! The terminal startup, drive and restoration path (shell.md, section 6.3).
use skein_io::kernel::{Complete, Fd, Submit};
use skein_io::{Event, Io, Limits, Request, Window};
use skein_lib::stream::{Down, Read, Up};
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};
use skein_shell::{Clock, Config, Host, Input, Keys, Watch, drive, open_signals, raw_input};
use std::{collections::VecDeque, process::ExitCode};

struct Terminal {
    io: Io,
    limits: Limits,
    input: Token,
    signals: Token,
    output: Token,
    completions: Queue<Complete>,
    submissions: Queue<Submit>,
    events: Queue<Event>,
    requests: Queue<Request>,
    lines: VecDeque<Box<[u8]>>,
    ending: bool,
    writing: bool,
}
impl Terminal {
    fn line(&mut self, line: Vec<u8>) {
        self.lines.push_back(line.into_boxed_slice());
    }
    fn end(&mut self) {
        if !self.ending {
            self.ending = true;
            for entity in [self.input, self.signals] {
                self.requests.push(Request::Abort { entity });
            }
        }
    }
    fn event(&mut self, event: Event) {
        match event {
            Event::Stream { owner, up: Up::Bytes(bytes) } if owner == self.input => {
                for key in &bytes {
                    self.line(vec![b'k', b'e', b'y', b' ', *key, b'\n']);
                    if *key == b'q' {
                        self.end();
                    }
                }
                if !self.ending {
                    self.requests.push(Request::Stream {
                        stream: self.input,
                        down: Down::Demand { read: Read::Fill(1), room: 0 },
                    });
                }
            }
            Event::Stream { owner, up: Up::Room } if owner == self.output => {
                let bytes = self.lines.pop_front().expect("requested room for a line");
                self.requests.push(Request::Stream { stream: self.output, down: Down::Send(bytes) });
                self.writing = false;
            }
            Event::Resized => self.requests.push(Request::Window { owner: self.input, stream: self.input }),
            Event::Window { window: Window::Size(size), .. } => {
                self.line(format!("size {}x{}\n", size.rows, size.columns).into_bytes());
            }
            Event::Window { window: Window::Gone, .. }
            | Event::Shutdown { .. }
            | Event::Failed { .. }
            | Event::Stream { up: Up::End, .. } => self.end(),
            Event::Closed { .. }
            | Event::Stream { .. }
            | Event::Output { .. }
            | Event::Listening { .. }
            | Event::Accepted { .. }
            | Event::Connecting { .. }
            | Event::Connected { .. }
            | Event::Spawned { .. }
            | Event::Exited { .. }
            | Event::Usage { .. } => {}
        }
    }
}
impl Host for Terminal {
    fn iterate(&mut self, now: Time, wall: Wall) {
        let env = Env { now, wall, limits: self.limits };
        while self.io.is_ready() {
            skein_io::resume(&mut self.io, &env, &mut self.events, &mut self.submissions);
        }
        while let Some(complete) = self.completions.pop() {
            skein_io::up(&mut self.io, &env, complete, &mut self.events, &mut self.submissions);
        }
        while self.io.is_due(now) {
            skein_io::fire(&mut self.io, &env, &mut self.events, &mut self.submissions);
        }
        while let Some(event) = self.events.pop() {
            self.event(event);
        }
        if !self.writing {
            if let Some(line) = self.lines.front() {
                self.writing = true;
                self.requests.push(Request::Stream {
                    stream: self.output,
                    down: Down::Demand { read: Read::Nothing, room: u32::try_from(line.len()).expect("small line") },
                });
            } else if self.ending {
                self.requests.push(Request::Close { entity: self.output });
            }
        }
        while let Some(request) = self.requests.pop() {
            skein_io::down(&mut self.io, &env, request, &mut self.submissions);
        }
        self.io.reclaim();
    }
    fn completions(&mut self) -> &mut Queue<Complete> {
        &mut self.completions
    }
    fn submissions(&mut self) -> &mut Queue<Submit> {
        &mut self.submissions
    }
    fn work_pending(&self, _now: Time) -> bool {
        self.io.is_ready() || !self.completions.is_empty()
    }
    fn next_deadline(&self) -> Option<Time> {
        self.io.next_deadline()
    }
    fn next_policy_deadline(&self) -> Option<Time> {
        None
    }
    fn is_empty(&self) -> bool {
        self.io.is_empty() && self.lines.is_empty() && self.requests.is_empty() && self.events.is_empty()
    }
    fn worst_case(&self) -> u64 {
        64 * 1024
    }
    fn operations(&self) -> u32 {
        skein_io::operations(&self.limits).expect("small limits")
    }
}
fn main() -> ExitCode {
    let Input::Terminal { kept, .. } = raw_input(Fd::new(0), Keys::Pass).expect("terminal input") else {
        return ExitCode::FAILURE;
    };
    let result = run();
    kept.restore().expect("startup modes restored after drive");
    result
}
fn run() -> ExitCode {
    let limits = Limits {
        sockets: 3,
        refusals: 1,
        intake: 8,
        receive: 1,
        output: 256,
        sends: 8,
        accepts: 1,
        backlog: 1,
        close_timeout: Duration::from_secs(1),
        names: 0,
        retry: Duration::from_millis(10),
    };
    let mut io = Io::new(&limits);
    let input = io.adopt_read_pipe(Fd::new(0)).expect("input");
    let output = io.adopt_write_pipe(Fd::new(2), None).expect("stderr");
    let signals = io.adopt_signals(open_signals(Watch::TerminationAndWindow).expect("signals")).expect("source");
    let mut host = Terminal {
        io,
        limits,
        input,
        output,
        signals,
        completions: Queue::with_capacity(32),
        submissions: Queue::with_capacity(32),
        events: Queue::with_capacity(32),
        requests: Queue::with_capacity(32),
        lines: VecDeque::from([Box::from(b"ready\n".as_slice())]),
        ending: false,
        writing: false,
    };
    host.requests.push(Request::Stream { stream: input, down: Down::Demand { read: Read::Fill(1), room: 0 } });
    let Ok(mut kernel) = skein_shell::Kernel::open(Config { operations: host.operations() }) else {
        return ExitCode::FAILURE;
    };
    let _exit = drive(&mut kernel, &Clock::new(), &mut host);
    ExitCode::SUCCESS
}
