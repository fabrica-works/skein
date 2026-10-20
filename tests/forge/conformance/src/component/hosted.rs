use crate::{
    Result,
    facts::{Fact, Outcome},
    fixture::Fixture,
};
use skein_forge::api;
use skein_forge_connection::{Component, Event, Request, Stopped};
use skein_io::{
    Io,
    kernel::{Complete, Submit},
};
use skein_lib::{Duration, Env, List, Queue, Secret, Time, Token, Wall, stream::Down};
use skein_shell::{Clock, Host, Kernel, Wait};
/// Counts request lines, never retaining a send's authentication bytes.
pub(crate) fn sent_requests(bytes: &[u8]) -> u64 {
    bytes
        .split(|b| *b == b'\n')
        .filter(|line| {
            [b"GET ".as_slice(), b"POST ", b"PATCH ", b"DELETE "].iter().any(|method| {
                line.strip_prefix(*method).is_some_and(|target| {
                    target.starts_with(b"/api/v1/") && target.windows(9).any(|v| v == b" HTTP/1.1")
                })
            })
        })
        .count() as u64
}
pub(super) struct Hosted {
    component: Component,
    env: Env<skein_forge_connection::Limits>,
    io: Io,
    io_env: Env<skein_io::Limits>,
    above: Queue<Event>,
    below: Queue<skein_io::Request>,
    events: Queue<skein_io::Event>,
    complete: Queue<Complete>,
    submit: Queue<Submit>,
    kernel: Option<Kernel>,
    clock: Clock,
    answer: Option<skein_forge_connection::Answered>,
    stopped: Option<Stopped>,
    closed: bool,
    serial: u64,
    heap: u64,
    pub(super) sent: u64,
    pub(super) costs: u64,
}
impl Hosted {
    fn limits() -> (skein_io::Limits, skein_forge_client::Limits, skein_forge_connection::Limits) {
        let io_limits = skein_io::Limits {
            sockets: 8,
            names: 0,
            refusals: 2,
            intake: 32768,
            receive: 16384,
            output: 32768,
            sends: 4,
            accepts: 1,
            backlog: 1,
            close_timeout: Duration::from_secs(1),
            retry: Duration::from_millis(10),
        };
        let client = skein_forge_client::Limits {
            pending: 4,
            calls: 2,
            rate: 100,
            reserve: 0,
            window: Duration::from_secs(10),
            op_bytes: 512,
            answer_bytes: 65536,
            rows: 8,
            inbox: 16,
            read_attempts: 3,
            backoff: Duration::from_secs(1),
            backoff_max: Duration::from_secs(4),
            entries: 8,
            write_attempts: 3,
            lifetime: Duration::from_secs(10),
            clock_margin: Duration::from_secs(1),
            resources: 8,
            repositories: 2,
            poll: Duration::from_secs(5),
            poll_max: Duration::from_secs(30),
            hinted: Duration::from_secs(1),
            slow: Duration::from_secs(40),
            settle: Duration::from_secs(30),
            facts: 16,
        };
        let limits = skein_forge_connection::Limits {
            forges: 1,
            token_bytes: 512,
            key_bytes: 1200,
            connections: 2,
            head_bytes: 16384,
            headers: 64,
            body_bytes_in: 16_777_216,
            depth: 16,
            chunk: 4096,
            string_bytes: 512,
            body_bytes: 16384,
            inner: 32,
            target_bytes: 4096,
            repositories: 2,
            users: 256,
            installations: 3,
            deadline: Duration::from_secs(10),
            idle: Duration::from_secs(4),
            start_backoff: Duration::from_secs(1),
            start_backoff_max: Duration::from_secs(60),
            renew_before: Duration::from_secs(600),
            hooks: 4,
            hook_head_bytes: 8192,
            hook_body_bytes: 1_048_576,
            hook_deadline: Duration::from_secs(10),
            hook_secret_bytes: 128,
            tls: skein_tls::client::Limits { read: 16384, send: 16384, records: skein_tls::client::MAX_RECORD },
        };
        (io_limits, client, limits)
    }
    pub(super) fn new(f: &Fixture, token: &skein_throwaway_forgejo::Token) -> Result<Self> {
        let (io_limits, client, limits) = Self::limits();
        let mut forges = List::with_capacity(1);
        forges
            .push(skein_forge_connection::settings::Forge {
                number: 1,
                kind: skein_forge_connection::settings::Kind::Forgejo,
                address: skein_io::Addr::Ip(([127, 0, 0, 1], f.instance.port()).into()),
                transport: skein_forge_connection::settings::Transport::Plaintext { name: None },
                access: Some(skein_forge_connection::Access {
                    credential: skein_forge::credential::Credential::Token(Secret::new(
                        token.value().as_bytes().into(),
                    )),
                    user: f.writer.id,
                    webhook_secret: None,
                }),
            })
            .expect("one forge");
        let shared = skein_forge_connection::Shared {
            calls: client.calls,
            rows: client.rows,
            answer_bytes: client.answer_bytes,
            op_bytes: client.op_bytes,
            lifetime: client.lifetime,
            name_bytes: 256,
            repositories: 2,
            io: io_limits,
        };
        let settings = skein_forge_connection::Settings {
            forges,
            webhooks: None,
            word: Box::from(b"kit".as_slice()),
            user_agent: Box::from(b"kit/0.0.0".as_slice()),
        };
        let owned_bytes = skein_forge_connection::settings::owned_bytes(&settings).ok_or("component owner bytes")?;
        let kinds = skein_forge_connection::limits::Forges::of(&settings).ok_or("component forge counts")?;
        let heap = skein_forge_connection::limits::worst_case(&limits, &shared, kinds, owned_bytes)
            .ok_or("component heap")?
            + skein_io::worst_case(&io_limits).ok_or("io heap")?
            + 256
                * (std::mem::size_of::<Event>()
                    + std::mem::size_of::<skein_io::Request>()
                    + std::mem::size_of::<skein_io::Event>()
                    + std::mem::size_of::<Complete>()
                    + std::mem::size_of::<Submit>()) as u64
            + 256 * (u64::from(client.answer_bytes) + 2 * u64::from(io_limits.output));
        let component = Component::new(settings, &limits, &shared).map_err(|e| format!("component limits: {e:?}"))?;
        let clock = Clock::new();
        let now = clock.now();
        let mut host = Self {
            component,
            env: Env { now: now.now, wall: now.wall, limits },
            io: Io::new(&io_limits),
            io_env: Env { now: now.now, wall: now.wall, limits: io_limits },
            above: Queue::with_capacity(256),
            below: Queue::with_capacity(256),
            events: Queue::with_capacity(256),
            complete: Queue::with_capacity(256),
            submit: Queue::with_capacity(256),
            kernel: Some(Kernel::open(skein_shell::Config {
                operations: skein_io::operations(&io_limits).ok_or("io operations")?,
            })?),
            clock,
            answer: None,
            stopped: None,
            closed: false,
            serial: 0,
            heap,
            sent: 0,
            costs: 0,
        };
        host.component.begin(&host.env, &mut host.above, &mut host.below);
        Ok(host)
    }
    fn pump(&mut self) {
        self.kernel.as_mut().expect("host ring").reap(&mut self.complete);
        let now = self.clock.now();
        self.iterate(now.now, now.wall);
        let wait = if self.answer.is_some()
            || self.stopped.is_some()
            || (self.serial == 0
                && matches!(self.component.start(1), Some(skein_forge_connection::start::Start::Ready { .. })))
            || self.work_pending(now.now)
        {
            Wait::No
        } else {
            Wait::Until(self.next_deadline().expect("component deadline"))
        };
        self.kernel.as_mut().expect("host ring").submit(&mut self.submit, wait);
    }
    pub(super) fn started(&mut self, name: &'static str, expected: Option<Stopped>) -> Result<Fact> {
        let end = self.env.now.saturating_add(Duration::from_secs(12));
        while !matches!(self.component.start(1), Some(skein_forge_connection::start::Start::Ready { .. }))
            && self.stopped.is_none()
        {
            if self.env.now >= end {
                return Err("component start did not settle".into());
            }
            self.pump();
        }
        let holds = self.stopped == expected;
        // Start requests belong to the start, outside logical call costs.
        self.sent = 0;
        Ok(Fact {
            name,
            outcome: if holds { Outcome::Holds } else { Outcome::Fails(format!("start {:?}", self.stopped)) },
        })
    }
    pub(super) fn call(
        &mut self,
        repository: api::Repository,
        op: api::Op,
    ) -> std::result::Result<api::Answer, api::Error> {
        self.serial += 1;
        self.component.down(
            &self.env,
            Request::Call { call: Token::new(self.serial), repository, op },
            &mut self.above,
            &mut self.below,
        );
        while self.answer.is_none() {
            self.pump();
        }
        let answer = self.answer.take().expect("call terminal");
        assert_eq!(answer.call, Token::new(self.serial), "terminal echoes call");
        self.costs += u64::from(answer.cost);
        answer.result
    }
    pub(super) fn finish(&mut self) {
        self.component.down(&self.env, Request::Close, &mut self.above, &mut self.below);
        // Move the ring out so drive borrows the host and kernel separately.
        let mut kernel = self.kernel.take().expect("host ring");
        let _exit = skein_shell::drive(&mut kernel, &Clock::new(), self);
    }
}
impl Host for Hosted {
    fn iterate(&mut self, now: Time, wall: Wall) {
        self.env.now = now;
        self.env.wall = wall;
        self.io_env.now = now;
        self.io_env.wall = wall;
        while self.io.is_ready() {
            skein_io::resume(&mut self.io, &self.io_env, &mut self.events, &mut self.submit);
        }
        while let Some(c) = self.complete.pop() {
            skein_io::up(&mut self.io, &self.io_env, c, &mut self.events, &mut self.submit);
            while self.io.is_ready() {
                skein_io::resume(&mut self.io, &self.io_env, &mut self.events, &mut self.submit);
            }
        }
        while self.io.is_due(now) {
            skein_io::fire(&mut self.io, &self.io_env, &mut self.events, &mut self.submit);
        }
        while let Some(e) = self.events.pop() {
            self.component.up(&self.env, e, &mut self.above, &mut self.below);
        }
        if self.component.next_deadline().is_some_and(|at| at <= now) {
            self.component.fire(&self.env, &mut self.above, &mut self.below);
        }
        while self.io.takes() {
            let Some(r) = self.below.pop() else {
                break;
            };
            if let skein_io::Request::Stream { down: Down::Send(bytes), .. } = &r {
                self.sent += sent_requests(bytes);
            }
            skein_io::down(&mut self.io, &self.io_env, r, &mut self.submit);
        }
        while let Some(e) = self.above.pop() {
            match e {
                Event::Installation { .. } | Event::Fact { .. } | Event::GitToken { .. } => panic!("Forgejo fixture"),
                Event::Connected { .. } => {}
                Event::Answered(a) => assert!(self.answer.replace(a).is_none(), "once per call"),
                Event::Stopped { why, .. } => {
                    assert!(self.stopped.replace(why).is_none(), "one stop");
                }
                Event::Hint(_) | Event::ListenerLost => panic!("webhooks absent"),
                Event::Closed => self.closed = true,
            }
        }
        self.component.reclaim();
        self.io.reclaim();
    }
    fn completions(&mut self) -> &mut Queue<Complete> {
        &mut self.complete
    }
    fn submissions(&mut self) -> &mut Queue<Submit> {
        &mut self.submit
    }
    fn work_pending(&self, now: Time) -> bool {
        self.io.is_ready()
            || self.io.is_due(now)
            || !self.below.is_empty()
            || self.component.next_deadline().is_some_and(|at| at <= now)
    }
    fn next_deadline(&self) -> Option<Time> {
        self.io.next_deadline().into_iter().chain(self.component.next_deadline()).min()
    }
    fn next_policy_deadline(&self) -> Option<Time> {
        self.component.next_deadline()
    }
    fn is_empty(&self) -> bool {
        self.closed && self.io.is_empty() && self.complete.is_empty() && self.submit.is_empty() && self.below.is_empty()
    }
    fn worst_case(&self) -> u64 {
        self.heap
    }
    fn operations(&self) -> u32 {
        skein_io::operations(&self.io_env.limits).expect("io operations")
    }
}
