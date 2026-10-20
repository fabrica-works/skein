//! The real forge component and fake HTTP service, joined without byte logs.
use crate::{
    fixtures,
    pipe::{Cut, Pipe},
};
use skein_fake_forge_domain::{self as fake, api as raw};
use skein_fake_forge_protocol::{self as protocol, Faults, Server, Service, Tally};
use skein_forge::api;
use skein_forge_connection::{Component, Event, Limits, Request, Settings, Shared};
use skein_lib::{Duration, Env, Queue, ReplyTo, Rng, Time, Token, Wall};
use std::collections::BTreeMap;
pub const REPO: api::Repository = api::Repository { forge: 1, repository: 2 };
struct Peer {
    pipe: Pipe,
    server: Server,
    above: Queue<protocol::Event>,
    below: Queue<skein_lib::stream::Down>,
}
pub struct Joint {
    pub component: Component,
    pub env: Env<Limits>,
    pub above: Queue<Event>,
    pub below: Queue<skein_io::Request>,
    peers: BTreeMap<Token, Peer>,
    service: Service,
    hook_listener: Option<Token>,
    hook_peers: BTreeMap<Token, crate::hooks::Peer>,
    pub hints: Vec<skein_forge_connection::hooks::Hinted>,
    next_hook: u64,
    hook_waiting: bool,
    hook_posts: std::collections::VecDeque<protocol::delivery::Post>,
    pub fake_env: Env<fake::Config>,
    protocol_env: Env<protocol::Limits>,
    pub domain: fake::Domain,
    fake_out: Queue<fake::Request>,
    pub answers: BTreeMap<u64, skein_forge_connection::Answered>,
    pub observations: Vec<fake::Observation>,
    rng: Rng,
    cut_chance: u32,
    direct: Option<Result<raw::Answer, raw::Error>>,
}
impl Joint {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self::cutting(seed, 0, Faults { reset: 0, drop: 0, server_error: 0 })
    }
    #[must_use]
    pub fn cutting(seed: u64, cut_chance: u32, faults: Faults) -> Self {
        let mut settings = fixtures::settings();
        settings.forges.get_mut(0).expect("forge").access.as_mut().expect("fixture access").user = 1;
        let mut config = fixtures::protocol_config();
        config.faults = faults;
        Self::with(seed, cut_chance, config, fixtures::limits(), settings, fixtures::shared(), checks(1000))
    }
    #[must_use]
    pub fn hooked(seed: u64) -> Self {
        let mut settings = fixtures::hook_settings();
        settings.forges.get_mut(0).expect("forge").access.as_mut().expect("fixture access").user = 1;
        Self::with(
            seed,
            0,
            fixtures::hooked_protocol_config(),
            fixtures::hook_limits(),
            settings,
            fixtures::shared(),
            checks(1000),
        )
    }
    #[must_use]
    pub fn failing(seed: u64) -> Self {
        let mut settings = fixtures::settings();
        settings.forges.get_mut(0).expect("forge").access.as_mut().expect("fixture access").user = 1;
        Self::with(seed, 0, fixtures::protocol_config(), fixtures::limits(), settings, fixtures::shared(), checks(0))
    }
    #[must_use]
    pub fn with(
        seed: u64,
        cut_chance: u32,
        config: protocol::Config,
        limits: Limits,
        settings: Settings,
        shared: Shared,
        checks: raw::Checks,
    ) -> Self {
        let fake_config = fixtures::fake_config();
        let fake_limits = fixtures::fake_limits();
        let mut domain = fake::Domain::new(&fake_config, seed);
        fake::repository(
            &mut domain,
            &fake_config,
            raw::Setup {
                id: 2,
                name: Box::from(b"org/repo".as_slice()),
                default: Box::from(b"main".as_slice()),
                tree: Box::from([]),
                labels: Box::from([]),
                checks,
                protection: None,
                hooked: config.hooks.is_some(),
            },
        );
        fake::grant(&mut domain, b"org/repo", 1, raw::Permission::Admin);
        fake::grant(&mut domain, b"org/repo", 2, raw::Permission::Write);
        let now = Time::from_nanos(100_000_000_000);
        let mut world = Self {
            component: Component::new(settings, &limits, &shared).expect("component"),
            env: Env { now, wall: Wall::from_nanos(now.as_nanos()), limits },
            above: Queue::with_capacity(128),
            below: Queue::with_capacity(128),
            peers: BTreeMap::new(),
            hook_listener: None,
            hook_peers: BTreeMap::new(),
            hints: Vec::new(),
            next_hook: 1_u64 << 58_u32,
            hook_waiting: false,
            hook_posts: std::collections::VecDeque::new(),
            service: Service::new(config, &fake_limits, seed).expect("service"),
            fake_env: Env { now, wall: Wall::EPOCH, limits: fake_config },
            protocol_env: Env { now, wall: Wall::from_nanos(now.as_nanos()), limits: fake_limits },
            domain,
            fake_out: Queue::with_capacity(fake::MAX_OUT),
            answers: BTreeMap::new(),
            observations: Vec::new(),
            rng: Rng::new(seed),
            cut_chance,
            direct: None,
        };
        world.component.begin(&world.env, &mut world.above, &mut world.below);
        world
    }
    pub fn call(&mut self, call: u64, repository: api::Repository, op: api::Op) {
        self.component.down(
            &self.env,
            Request::Call { call: Token::new(call), repository, op },
            &mut self.above,
            &mut self.below,
        );
    }
    #[must_use]
    pub fn busy(&self) -> bool {
        !self.below.is_empty()
            || self.peers.values().any(|peer| {
                peer.pipe.has_work() || peer.server.has_work() || !peer.below.is_empty() || !peer.above.is_empty()
            })
    }
    pub fn run_for(&mut self, span: Duration) {
        let end = self.env.now.saturating_add(span);
        while self.env.now < end {
            self.tick();
            let busy = self.peers.values().any(|peer| {
                peer.pipe.has_work() || peer.server.has_work() || !peer.below.is_empty() || !peer.above.is_empty()
            });
            let next = if busy
                || !self.below.is_empty()
                || !self.hook_posts.is_empty()
                || self.hook_peers.values().any(|peer| peer.wire.received.is_empty())
            {
                self.env.now.saturating_add(Duration::from_nanos(100_000))
            } else {
                let component = self.component.next_deadline().unwrap_or(end);
                let fake = self.domain.next_deadline().unwrap_or(end);
                component.min(fake).max(self.env.now.saturating_add(Duration::from_nanos(100_000)))
            };
            self.time(next.min(end));
        }
        self.tick();
    }
    pub fn time(&mut self, now: Time) {
        self.env.now = now;
        self.env.wall = Wall::from_nanos(now.as_nanos());
        self.fake_env.now = now;
        self.protocol_env.now = fake::time(&self.fake_env.limits, now);
        self.protocol_env.wall = Wall::from_nanos(self.protocol_env.now.as_nanos());
    }
    fn up(&mut self, event: skein_io::Event) {
        self.component.up(&self.env, event, &mut self.above, &mut self.below);
    }
    pub fn tick(&mut self) {
        while let Some(request) = self.below.pop() {
            self.io(request);
        }
        let owners: Vec<_> = self.peers.keys().copied().collect();
        for owner in owners {
            let peer = self.peers.get_mut(&owner).expect("peer");
            protocol::resume(&mut peer.server, &mut self.service, &self.protocol_env, &mut peer.above, &mut peer.below);
            let _ = peer.server.take_head();
            while let Some(down) = peer.below.pop() {
                peer.pipe.server_down(down);
            }
            if let Some(up) = peer.pipe.server_up() {
                protocol::up(
                    &mut peer.server,
                    &mut self.service,
                    &self.protocol_env,
                    up,
                    &mut peer.above,
                    &mut peer.below,
                );
                let _ = peer.server.take_head();
            }
            let client_up = peer.pipe.client_up();
            if let Some(up) = client_up {
                self.up(skein_io::Event::Stream { owner, up });
            }
            self.server_events(owner);
        }
        let sockets: Vec<_> = self.hook_peers.keys().copied().collect();
        for socket in sockets {
            let peer = self.hook_peers.get_mut(&socket).expect("hook peer");
            if let Some(owner) = peer.owner
                && let Some(up) = peer.wire.answer()
            {
                self.up(skein_io::Event::Stream { owner, up });
            }
        }
        fake::fire(&mut self.domain, &self.fake_env, &mut self.fake_out);
        while let Some(reply) = self.fake_out.pop() {
            match reply {
                fake::Request::Reply { to, result } => {
                    let token = to.into_token();
                    if token == Token::new(u64::MAX) {
                        assert!(self.direct.replace(result).is_none(), "one direct answer");
                        continue;
                    }
                    if let Ok((owner, reply)) =
                        self.service.target(fake::Request::Reply { to: ReplyTo::new(token), result })
                        && let Some(peer) = self.peers.get_mut(&owner)
                    {
                        protocol::down(
                            &mut peer.server,
                            &mut self.service,
                            &self.protocol_env,
                            reply,
                            &mut peer.above,
                            &mut peer.below,
                        );
                    }
                }
                hook @ fake::Request::Hook { .. } => {
                    if let Some(post) = self.service.delivery(&hook, &self.protocol_env.limits) {
                        self.hook_posts.push_back(post);
                    }
                }
            }
        }
        self.accept_hook();
        if self.component.next_deadline().is_some_and(|at| at <= self.env.now) {
            self.component.fire(&self.env, &mut self.above, &mut self.below);
        }
        while let Some(event) = self.above.pop() {
            match event {
                Event::Answered(answer) => {
                    assert!(self.answers.insert(answer.call.raw(), answer).is_none(), "one answer per call");
                }
                Event::Stopped { forge, why } => panic!("forge {forge} stopped: {why:?}"),
                Event::Hint(hint) => self.hints.push(hint),
                Event::ListenerLost => panic!("hook listener lost"),
                Event::Connected { .. } | Event::Closed => {}
            }
        }
        while let Some(observation) = self.domain.pop_observation() {
            self.observations.push(observation);
        }
        self.service.reclaim();
        self.domain.reclaim();
        self.component.reclaim();
    }
    fn accept_hook(&mut self) {
        if !self.hook_waiting
            && let Some(post) = self.hook_posts.pop_front()
        {
            self.hook_waiting = true;
            let socket = Token::new(self.next_hook);
            self.next_hook += 1;
            let mut wire = skein_world::stream::Wire::new(self.rng.next_u64());
            wire.write(&post.head);
            wire.write(&post.body);
            self.hook_peers.insert(socket, crate::hooks::Peer { wire, owner: None, closed: false });
            self.up(skein_io::Event::Accepted {
                owner: self.hook_listener.expect("hook listener"),
                socket,
                peer: skein_io::Addr::Ip(([127, 0, 0, 1], 9000).into()),
            });
        }
    }
    fn server_events(&mut self, owner: Token) {
        while let Some(event) = self.peers.get_mut(&owner).expect("peer").above.pop() {
            match event {
                protocol::Event::Domain(event) => {
                    fake::step(&mut self.domain, &self.fake_env, event, &mut self.fake_out);
                }
                protocol::Event::Close => {
                    self.end(owner);
                    break;
                }
                protocol::Event::Closed => {}
            }
        }
    }
    fn io(&mut self, request: skein_io::Request) {
        match request {
            skein_io::Request::Connect { owner, .. } => {
                let cut =
                    if self.rng.chance(self.cut_chance) { Cut::At(self.rng.between(0, 2048)) } else { Cut::Never };
                let mut peer = Peer {
                    pipe: Pipe::new(self.rng.next_u64(), cut),
                    server: Server::new(owner, &self.protocol_env.limits).expect("server"),
                    above: Queue::with_capacity(64),
                    below: Queue::with_capacity(64),
                };
                protocol::start(
                    &mut peer.server,
                    &mut self.service,
                    &self.protocol_env,
                    &mut peer.above,
                    &mut peer.below,
                );
                assert!(self.peers.insert(owner, peer).is_none(), "fresh connection");
                self.up(skein_io::Event::Connecting { owner, socket: owner });
                self.up(skein_io::Event::Connected { owner });
            }
            skein_io::Request::Stream { stream, down } => {
                if let Some(peer) = self.hook_peers.get_mut(&stream) {
                    peer.wire.take(down);
                } else if let Some(peer) = self.peers.get_mut(&stream) {
                    peer.pipe.client_down(down);
                }
            }
            skein_io::Request::Abort { entity } | skein_io::Request::Close { entity } => self.end(entity),
            skein_io::Request::Listen { owner, addr } => {
                self.hook_listener = Some(owner);
                self.up(skein_io::Event::Listening { owner, listener: Token::new(1_u64 << 57_u32), addr });
            }
            skein_io::Request::Bind { owner, socket } => {
                self.hook_waiting = false;
                self.hook_peers.get_mut(&socket).expect("accepted hook").owner = Some(owner);
            }
            other @ (skein_io::Request::Reject { .. }
            | skein_io::Request::Output { .. }
            | skein_io::Request::Spawn { .. }
            | skein_io::Request::Signal { .. }
            | skein_io::Request::Usage { .. }
            | skein_io::Request::Window { .. }) => panic!("unexpected io {other:?}"),
        }
    }
    pub(crate) fn end(&mut self, owner: Token) {
        if let Some(peer) = self.hook_peers.remove(&owner) {
            self.up(skein_io::Event::Closed { owner: peer.owner.expect("bound hook") });
        }
        if let Some(mut peer) = self.peers.remove(&owner) {
            protocol::closed(&mut peer.server, &mut self.service, &self.protocol_env, &mut peer.above, &mut peer.below);
            self.up(skein_io::Event::Closed { owner });
        }
    }
    #[must_use]
    pub fn answered(&self, call: u64) -> Option<&skein_forge_connection::Answered> {
        self.answers.get(&call)
    }
    #[must_use]
    pub fn tally(&self) -> &Tally {
        self.service.tally()
    }
    #[must_use]
    pub fn now(&self) -> Time {
        self.env.now
    }
    /// A person calls the fake domain outside the component's API traffic.
    /// # Errors
    /// Returns the domain's refusal of the supplied operation.
    pub fn direct(&mut self, op: raw::Op) -> Result<raw::Answer, raw::Error> {
        fake::step(
            &mut self.domain,
            &self.fake_env,
            fake::Event::Call {
                reply_to: ReplyTo::new(Token::new(u64::MAX)),
                user: 2,
                repository: Box::from(b"org/repo".as_slice()),
                op,
            },
            &mut self.fake_out,
        );
        for _ in 0..1000 {
            self.tick();
            if let Some(result) = self.direct.take() {
                return result;
            }
            self.time(self.env.now.saturating_add(Duration::from_millis(1)));
        }
        panic!("direct reply")
    }
    /// Drops the old process's connections and installs its fresh component.
    pub fn replace(&mut self, component: Component) {
        let owners: Vec<_> = self.peers.keys().copied().collect();
        for owner in owners {
            self.end(owner);
        }
        self.component = component;
        while self.above.pop().is_some() {}
        while self.below.pop().is_some() {}
        self.component.begin(&self.env, &mut self.above, &mut self.below);
    }
    pub fn issue(&mut self, title: &[u8], body: &[u8]) -> u64 {
        let raw::Answer::Created(id) = self
            .direct(raw::Op::Write(raw::Write::CreateIssue {
                title: Box::from(title),
                body: Box::from(body),
                labels: Box::from([]),
            }))
            .expect("issue")
        else {
            panic!("issue id")
        };
        id
    }
    pub fn branch(&mut self, name: &[u8], at: u64) {
        self.direct(raw::Op::Write(raw::Write::CreateBranch { branch: Box::from(name), commit: at })).expect("branch");
    }
    pub fn pull(&mut self, head: &[u8], base: &[u8]) -> u64 {
        let raw::Answer::Created(id) = self
            .direct(raw::Op::Write(raw::Write::OpenPull {
                title: Box::from(b"pull".as_slice()),
                body: Box::from(b"body".as_slice()),
                head: Box::from(head),
                base: Box::from(base),
                opens_empty: true,
            }))
            .expect("pull")
        else {
            panic!("pull id")
        };
        id
    }
    pub fn close(&mut self, number: u64) {
        self.direct(raw::Op::Write(raw::Write::Close { number })).expect("close");
    }
    pub fn push(&mut self, branch: &[u8], content: &[u8]) -> u64 {
        let raw::Answer::Commit(parent) =
            self.direct(raw::Op::Read(raw::Read::Branch { branch: Box::from(b"main".as_slice()) })).expect("main")
        else {
            panic!("commit")
        };
        let commit = fake::commit(
            &mut self.domain,
            &self.fake_env.limits,
            parent,
            Box::new([raw::File { path: Box::from(b"file".as_slice()), content: Box::from(content) }]),
            b"message",
        )
        .expect("commit")
        .expect("changed");
        self.direct(raw::Op::Git(raw::Git::Push { branch: Box::from(branch), commit, expected: None })).expect("push");
        commit
    }
}

#[must_use]
pub fn checks(passes: u32) -> raw::Checks {
    raw::Checks {
        contexts: Box::new([Box::from(b"build".as_slice())]),
        latency_min: Duration::ZERO,
        latency_max: Duration::ZERO,
        silent: 0,
        passes,
        reruns: 0,
        cue: None,
    }
}
