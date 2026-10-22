//! One-thread pipe reader with deadlines and stderr evidence.
use crate::json::{self, Value};
use skein_io::{
    self as io, Io,
    kernel::{Complete, Fd, Submit},
};
use skein_lib::{Duration, Env, Queue, Time, Token, stream};
use skein_shell::{Clock, Wait};
use skein_world::ring::Ring;
use std::{
    collections::VecDeque,
    os::fd::IntoRawFd,
    path::{Path, PathBuf},
    process::{Child as Process, ChildStdout},
};
pub fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    let Value::Object(fields) = value else { panic!("object: {value:?}") };
    fields.get(key).unwrap_or_else(|| panic!("missing {key}: {value:?}"))
}
pub fn integer(value: &Value) -> i64 {
    let Value::Number(n) = value else { panic!("integer: {value:?}") };
    *n
}
pub fn text(value: &Value) -> &str {
    let Value::String(s) = value else { panic!("string: {value:?}") };
    s
}
pub struct Child(pub Process);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
pub fn chromium() -> PathBuf {
    if let Some(path) = std::env::var_os("SKEIN_TEST_CHROMIUM") {
        let path = PathBuf::from(path);
        assert!(path.is_file(), "SKEIN_TEST_CHROMIUM: {}", path.display());
        return path.canonicalize().expect("resolve Chromium path");
    }
    let path = std::env::var_os("PATH").expect("PATH locates chromium");
    for dir in std::env::split_paths(&path) {
        let path = dir.join("chromium");
        if path.is_file() {
            return path.canonicalize().expect("resolve Chromium path");
        }
    }
    panic!("Chromium is required: set SKEIN_TEST_CHROMIUM or put chromium on PATH");
}
pub fn fonts() -> PathBuf {
    let configured = std::env::var_os("SKEIN_TEST_FONTS").map(PathBuf::from);
    let fallback = Path::new("/usr/share/fonts/truetype/dejavu");
    if let Some(dir) = &configured
        && dir.join("DejaVuSans.ttf").is_file()
    {
        return dir.canonicalize().expect("resolve fonts directory");
    }
    if fallback.join("DejaVuSans.ttf").is_file() {
        return fallback.to_path_buf();
    }
    panic!(
        "DejaVuSans.ttf missing from SKEIN_TEST_FONTS ({configured:?}) and {}; install Debian package fonts-dejavu-core",
        fallback.display()
    );
}
pub struct Reader {
    io: Io,
    ring: Ring,
    env: Env<io::Limits>,
    clock: Clock,
    pipe: Token,
    complete: Queue<Complete>,
    submits: Queue<Submit>,
    events: Queue<io::Event>,
    received: VecDeque<stream::Up>,
    stderr: PathBuf,
}
impl Reader {
    pub fn new(stdout: ChildStdout, stderr: PathBuf) -> Self {
        let limits = io::Limits {
            sockets: 1,
            names: 0,
            refusals: 1,
            intake: 32 << 20,
            receive: 65536,
            output: 1,
            sends: 1,
            accepts: 1,
            backlog: 1,
            close_timeout: Duration::from_secs(1),
            retry: Duration::from_millis(10),
        };
        let mut io = Io::new(&limits);
        let pipe = io.adopt_read_pipe(Fd::new(stdout.into_raw_fd())).expect("adopt child stdout");
        let clock = Clock::new();
        let now = clock.now();
        Self {
            io,
            ring: Ring::new(
                io::operations(&limits).expect("reader operation bound"),
                vec![io::operations(&limits).expect("reader operation bound")],
            )
            .expect("reader ring"),
            env: Env { now: now.now, wall: now.wall, limits },
            clock,
            pipe,
            complete: Queue::with_capacity(32),
            submits: Queue::with_capacity(32),
            events: Queue::with_capacity(32),
            received: VecDeque::new(),
            stderr,
        }
    }
    pub fn now(&self) -> Time {
        self.clock.now().now
    }
    pub fn stderr(&self) -> String {
        std::fs::read_to_string(&self.stderr).expect("program stderr")
    }
    pub fn answer(&mut self) -> Value {
        let line = self.line();
        let value = json::parse(&line).unwrap_or_else(|e| panic!("answer JSON: {e}; {}", self.stderr()));
        assert_eq!(json::encode(&value), line, "answer must be canonical");
        value
    }
    pub fn line(&mut self) -> Vec<u8> {
        let deadline = self.now().saturating_add(Duration::from_secs(10));
        io::down(
            &mut self.io,
            &self.env,
            io::Request::Stream {
                stream: self.pipe,
                down: stream::Down::Demand {
                    read: stream::Read::Scan {
                        until: stream::Delimiter::new(b"\n").expect("LF delimiter"),
                        max: 32 << 20,
                    },
                    room: 0,
                },
            },
            &mut self.submits,
        );
        loop {
            if let Some(up) = self.received.pop_front() {
                let stream::Up::Bytes(bytes) = up else {
                    panic!("program stdout ended: {up:?}; {}", self.stderr());
                };
                assert!(bytes.ends_with(b"\n"), "answer missing newline: {}", self.stderr());
                return bytes[..bytes.len() - 1].to_vec();
            }
            assert!(self.now() < deadline, "answer deadline passed: {}", self.stderr());
            self.pause();
        }
    }
    pub fn pause(&mut self) {
        let now = self.clock.now();
        self.env.now = now.now;
        self.env.wall = now.wall;
        while let Some(complete) = self.ring.reap().pop() {
            let (host, complete) = self.ring.complete(complete);
            self.ring.returned(host, complete);
        }
        self.ring.deliver(0, &mut self.complete);
        while self.io.is_ready() {
            io::resume(&mut self.io, &self.env, &mut self.events, &mut self.submits);
        }
        while let Some(complete) = self.complete.pop() {
            io::up(&mut self.io, &self.env, complete, &mut self.events, &mut self.submits);
        }
        while let Some(event) = self.events.pop() {
            match event {
                io::Event::Stream { up, .. } => self.received.push_back(up),
                other @ (io::Event::Listening { .. }
                | io::Event::Accepted { .. }
                | io::Event::Connecting { .. }
                | io::Event::Connected { .. }
                | io::Event::Output { .. }
                | io::Event::Spawned { .. }
                | io::Event::Exited { .. }
                | io::Event::Usage { .. }
                | io::Event::Resized
                | io::Event::Window { .. }
                | io::Event::Shutdown { .. }
                | io::Event::Failed { .. }
                | io::Event::Closed { .. }) => panic!("stdout io: {other:?}"),
            }
        }
        self.io.reclaim();
        while let Some(submit) = self.submits.pop() {
            let submit = self.ring.register(0, submit);
            self.ring.queue(submit);
        }
        self.ring.enter(Wait::Until(now.now.saturating_add(Duration::from_millis(10))));
    }
}
