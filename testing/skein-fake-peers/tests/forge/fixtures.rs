use super::support;
use skein_fake_forge_domain::{self as domain, api};
use skein_fake_forge_protocol::{Config, Faults, Repository};
use skein_fake_peers::{
    Limits,
    forge::{self, Forge, Peer},
};
use skein_io::kernel::{Complete, Done, Family, Fd, Op, Submit};
use skein_lib::{Duration, List, Queue, Secret, Time, Token, bytes};
use skein_sim::{Pid, Sim};
use skein_world::Host;
use std::{
    cell::RefCell,
    net::{Ipv4Addr, SocketAddr},
    rc::Rc,
};

pub const SEED: u64 = 23;
pub const MAX_PASSES: u32 = 1000;
pub const REPOSITORY: &[u8] = b"org/repo";
pub const BRANCH: &[u8] = b"work";
pub const USER: u64 = 1;
pub const ONE_REPLY: usize = 1;
// Setup creates commit 1; the tiny git caller stores and pushes the next commit.
pub const PUSHED_COMMIT: u64 = 2;
pub const WORK_BRANCH_BODY: &[u8] = br#"{"name":"work","commit":{"id":"0000000000000002000000000000000000000000"},"effective_branch_protection_name":""}"#;
pub fn address() -> SocketAddr {
    (Ipv4Addr::LOCALHOST, 0).into()
}
pub fn limits() -> Limits {
    Limits {
        io: skein_io::Limits {
            sockets: 3,
            refusals: 1,
            intake: 32_768,
            receive: 1024,
            output: 32_768,
            sends: 2,
            accepts: 1,
            backlog: 1,
            close_timeout: Duration::from_secs(1),
            names: 0,
            retry: Duration::from_millis(1),
        },
        connections: 2,
        queue: 64,
        plaintext: 32_768,
        ciphertext: 32_768,
        observations: 16,
        observation_bytes: 32_768,
    }
}
pub const TOKEN: &[u8] = b"1111111111111111111111111111111111111111";
pub fn config() -> Config {
    let mut tokens = List::with_capacity(1);
    tokens.push((Secret::new(bytes::copy_of(TOKEN)), 1)).expect("one token");
    let mut users = List::with_capacity(2);
    users.push((1, bytes::copy_of(b"engine"))).expect("two users");
    users.push((2, bytes::copy_of(b"person"))).expect("two users");
    let mut repositories = List::with_capacity(1);
    repositories
        .push(Repository { id: 2, full: bytes::copy_of(b"org/repo"), protected: None })
        .expect("one repository");
    Config {
        base: bytes::copy_of(b"http://127.0.0.1:3000"),
        tokens,
        users,
        repositories,
        max_response_items: 2,
        default_paging_num: 30,
        hooks: None,
        faults: Faults { reset: 0, drop: 0, server_error: 0 },
    }
}
pub fn protocol_limits() -> skein_fake_forge_protocol::Limits {
    skein_fake_forge_protocol::Limits {
        calls: 4,
        connections: 4,
        raw_bytes: 4096,
        http: skein_http::server::Limits {
            head: 4096,
            headers: 16,
            body: 16384,
            read: 1024,
            response: 4096,
            send: 1024,
        },
        documents: skein_forge::forgejo::limits::Limits {
            json: skein_json::tokenizer::Limits { chunk: 1024, string: 16384, number: 32, depth: 16, length: 65536 },
            writer: skein_json::writer::Limits { depth: 16, length: 65536 },
            string_bytes: 512,
            body_bytes: 16384,
            inner: 32,
            target_bytes: 4096,
        },
    }
}
pub fn boxed(text: &[u8]) -> Box<[u8]> {
    bytes::copy_of(text)
}
pub fn fake_config() -> domain::Config {
    domain::Config {
        limits: domain::Limits {
            repositories: 1,
            users: 3,
            labels: 2,
            items: 64,
            comments: 32,
            dependencies: 2,
            reviews: 8,
            branches: 4,
            commits: 16,
            files: 4,
            statuses: 4,
            contexts: 4,
            pages: 1,
            name_bytes: 64,
            title_bytes: 64,
            body_bytes: 1024,
            content_bytes: 128,
            page_size: 2,
            calls: 4,
            hooks: 8,
            observations: 64,
        },
        latency_min: Duration::from_millis(10),
        latency_max: Duration::from_millis(50),
        late: 0,
        late_min: Duration::from_secs(1),
        late_max: Duration::from_secs(3),
        unavailable: 0,
        timeouts: 0,
        landing: 0,
        land_min: Duration::from_secs(1),
        land_max: Duration::from_secs(3),
        rate_limit: 0,
        rate_window: Duration::from_secs(10),
        ci: 3,
        hook_min: Duration::ZERO,
        hook_max: Duration::ZERO,
        hooks_late: 0,
        hooks_lost: 0,
        resolution: Duration::from_secs(1),
        skew: domain::Skew::Ahead(Duration::from_secs(20)),
        status_updates: false,
        edit_updates: false,
    }
}

pub fn shared() -> Rc<RefCell<Forge>> {
    let config = fake_config();
    let mut domain = domain::Domain::new(&config, SEED);
    domain::repository(
        &mut domain,
        &config,
        api::Setup {
            id: 2,
            name: boxed(REPOSITORY),
            default: boxed(b"main"),
            tree: Box::new([]),
            labels: Box::new([]),
            checks: api::Checks {
                contexts: Box::new([]),
                latency_min: Duration::ZERO,
                latency_max: Duration::ZERO,
                silent: 0,
                passes: 0,
                reruns: 0,
                cue: None,
            },
            protection: None,
            hooked: false,
        },
    );
    domain::grant(&mut domain, REPOSITORY, USER, api::Permission::Admin);
    Rc::new(RefCell::new(Forge::new(domain, config)))
}

pub struct World {
    pub peer: Peer,
    pub forge: Rc<RefCell<Forge>>,
    pub sim: Sim,
    peer_pid: Pid,
    client_pid: Pid,
    passes: u32,
    serial: u64,
    pub replies: usize,
}
impl World {
    pub fn new() -> Self {
        let forge = shared();
        let peer = Peer::new(address(), limits(), config(), protocol_limits(), Rc::clone(&forge), SEED)
            .expect("checked forge fixture");
        let mut sim = Sim::new(SEED, skein_sim::Config::calm());
        let peer_pid = sim.spawn_process();
        let client_pid = sim.spawn_process();
        Self { peer, forge, sim, peer_pid, client_pid, passes: 0, serial: 0, replies: 0 }
    }
    pub fn pass(&mut self) {
        self.passes += 1;
        assert!(self.passes <= MAX_PASSES, "forge world settles within its pass bound");
        self.sim.reap(self.peer_pid, self.peer.completions());
        self.peer.iterate(self.sim.now(), self.sim.wall());
        self.sim.submit(self.peer_pid, self.peer.submissions());
        if !self.peer.work_pending(self.sim.now())
            && let Some(next) = [self.sim.next_due(), self.peer.next_deadline()].into_iter().flatten().min()
        {
            self.sim.advance_to(next);
        }
    }
    pub fn bind(&mut self) {
        while self.peer.address().is_none() {
            self.pass();
        }
    }
    pub fn open(&mut self) -> Fd {
        self.bind();
        let Done::Fd(fd) = support::call(&mut self.sim, self.client_pid, 1, Op::Socket { family: Family::Ipv4 }) else {
            panic!("socket descriptor");
        };
        assert_eq!(
            support::call(
                &mut self.sim,
                self.client_pid,
                2,
                Op::Connect { fd, addr: self.peer.address().expect("listening") }
            ),
            Done::Nothing
        );
        while !self
            .peer
            .observations()
            .iter()
            .any(|record| matches!(record, forge::Observation::Connection { open: true, .. }))
        {
            self.pass();
        }
        fd
    }
    fn client(&mut self, kind: Op) -> Complete {
        self.serial += 1;
        let token = Token::new(self.serial);
        let mut out = Queue::with_capacity(1);
        let mut input = Queue::with_capacity(1);
        out.push(Submit { op: token, kind });
        self.sim.submit(self.client_pid, &mut out);
        loop {
            self.sim.reap(self.client_pid, &mut input);
            if let Some(complete) = input.pop() {
                assert_eq!(complete.op, token);
                return complete;
            }
            self.pass();
        }
    }
    pub fn send(&mut self, fd: Fd, bytes: &[u8]) {
        let mut from = 0;
        while usize::try_from(from).expect("request length") < bytes.len() {
            let complete = self.client(Op::Send { fd, bytes: boxed(bytes), from });
            let Ok(Done::Count(sent)) = complete.result else {
                panic!("send succeeds: {complete:?}");
            };
            assert!(sent > 0);
            from += sent;
        }
    }
    pub fn response(&mut self, fd: Fd) -> Vec<u8> {
        let mut response = Vec::new();
        loop {
            let complete = self.client(Op::Recv { fd, buf: Box::new([0; 1024]) });
            let Ok(Done::Count(count)) = complete.result else {
                panic!("read succeeds: {complete:?}");
            };
            if count == 0 {
                break;
            }
            let Op::Recv { buf, .. } = complete.kind else {
                panic!("receive buffer");
            };
            response.extend_from_slice(&buf[..usize::try_from(count).expect("read size")]);
        }
        self.replies += 1;
        response
    }
    pub fn close(&mut self, fd: Fd) {
        assert_eq!(support::call(&mut self.sim, self.client_pid, 3, Op::Close { fd }), Done::Nothing);
    }
    pub fn settle(&mut self) {
        self.peer.shutdown();
        while !self.peer.is_empty() {
            self.pass();
        }
        assert!(self.peer.is_empty());
        for pid in [self.peer_pid, self.client_pid] {
            self.sim.assert_quiescent(pid);
            self.sim.assert_no_open_fds(pid);
        }
    }
    pub fn git_push(&mut self) -> (u64, usize, Time) {
        let (token, commit) = {
            let mut forge = self.forge.borrow_mut();
            assert!(forge.pending.room() >= domain::MAX_OUT, "reserve external domain output");
            let config = forge.config;
            let commit = domain::commit(
                &mut forge.domain,
                &config,
                1,
                Box::new([api::File { path: boxed(b"file"), content: boxed(b"content") }]),
                b"message",
            )
            .expect("commit room")
            .expect("changed commit");
            let token = forge.next_reply();
            let env = skein_lib::Env { now: forge.now, wall: self.sim.wall(), limits: config };
            let Forge { domain, pending, .. } = &mut *forge;
            domain::step(
                domain,
                &env,
                domain::Event::Call {
                    reply_to: skein_lib::ReplyTo::new(token),
                    user: USER,
                    repository: boxed(REPOSITORY),
                    op: api::Op::Git(api::Git::Push { branch: boxed(BRANCH), commit, expected: None }),
                },
                pending,
            );
            (token, commit)
        };
        loop {
            self.pass();
            let mut forge = self.forge.borrow_mut();
            let mut replies = 0;
            for _ in 0..forge.pending.len() {
                let request = forge.pending.pop().expect("pending pass length");
                match request {
                    domain::Request::Reply { to, result } => {
                        let destination = to.into_token();
                        if destination == token {
                            assert_eq!(result, Ok(api::Answer::Pushed(api::Pushed::Pushed)));
                            replies += 1;
                        } else {
                            forge
                                .pending
                                .push(domain::Request::Reply { to: skein_lib::ReplyTo::new(destination), result });
                        }
                    }
                    request @ domain::Request::Hook { .. } => forge.pending.push(request),
                }
            }
            if replies > 0 {
                return (commit, replies, forge.now);
            }
        }
    }
}

pub fn request(path: &[u8]) -> Vec<u8> {
    let mut request = b"GET ".to_vec();
    request.extend_from_slice(path);
    request.extend_from_slice(b" HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: token ");
    request.extend_from_slice(TOKEN);
    request.extend_from_slice(b"\r\nConnection: close\r\n\r\n");
    request
}

pub struct Refusal {
    pub name: &'static str,
    pub address: SocketAddr,
    pub limits: Limits,
    pub protocol: skein_fake_forge_protocol::Limits,
    pub config: Config,
    pub shared: Rc<RefCell<Forge>>,
}
impl Refusal {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            address: address(),
            limits: limits(),
            protocol: protocol_limits(),
            config: config(),
            shared: shared(),
        }
    }
}
pub fn refusals() -> [Refusal; 8] {
    // Face's queue minimum is 64; one less cannot reserve a server pass.
    const SHORT_QUEUE: u32 = 63;
    // The protocol fixture admits four connections; five is one past it.
    const TOO_MANY_CONNECTIONS: u32 = 5;
    // The shared fixture needs four calls plus eight hooks pending records.
    const SHORT_PENDING: u32 = 11;
    // HTTP head bytes 4096 require at least that much plaintext space.
    const SHORT_PLAINTEXT: u32 = 4095;
    // Chances are per thousand.
    const INVALID_CHANCE: u32 = 1001;
    let pending = shared();
    pending.borrow_mut().pending = Queue::with_capacity(SHORT_PENDING);
    let domain = shared();
    domain.borrow_mut().config.late = INVALID_CHANCE;
    [
        Refusal { address: (Ipv4Addr::new(192, 0, 2, 1), 80).into(), ..Refusal::new("non-loopback") },
        Refusal { limits: Limits { queue: SHORT_QUEUE, ..limits() }, ..Refusal::new("short queue") },
        Refusal {
            protocol: skein_fake_forge_protocol::Limits { calls: 0, ..protocol_limits() },
            ..Refusal::new("no protocol bound")
        },
        Refusal { limits: Limits { connections: TOO_MANY_CONNECTIONS, ..limits() }, ..Refusal::new("connections") },
        Refusal { shared: pending, ..Refusal::new("pending") },
        Refusal { limits: Limits { plaintext: SHORT_PLAINTEXT, ..limits() }, ..Refusal::new("wire") },
        Refusal { config: Config { base: Box::new([]), ..config() }, ..Refusal::new("config") },
        Refusal { shared: domain, ..Refusal::new("domain") },
    ]
}
