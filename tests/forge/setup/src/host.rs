//! The host over HTTP bytes, a fake Forgejo and io played by hand.
use crate::{fixtures, world::seeded_config};
use skein_fake_forge_protocol::{self as fake, Service};
use skein_forge_machine_world::pipe::{Cut, Pipe};
use skein_forge_setup::{
    host::{Answer, Event, Host, Limits, Need, Plan, Request, Settings},
    item::Item,
    reconcile::{Failure, Outcome},
};
use skein_lib::{
    Duration, Env, Queue, Secret, Time, Token, Wall,
    stream::{Down, Read, Up},
};
use std::collections::{BTreeMap, VecDeque};
#[derive(Clone, Copy, Debug)]
pub enum Connect {
    Made,
    Refused,
    Silent,
    Unavailable,
    Answers(u16, &'static [u8]),
}
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Heard {
    Done { run: Token, outcome: Outcome, at: Duration },
    Stop { run: Token, failure: Failure, at: Duration },
    Closed { at: Duration },
}
struct Peer {
    pipe: Pipe,
    server: Option<fake::Server>,
    above: Queue<fake::Event>,
    below: Queue<Down>,
    answer: Option<(u16, &'static [u8])>,
    head: Vec<u8>,
    responded: bool,
}
pub struct HostWorld {
    host: Host,
    env: Env<Limits>,
    service: Service,
    fake_env: Env<fake::Limits>,
    above: Queue<Event>,
    below: Queue<skein_io::Request>,
    peers: BTreeMap<Token, Peer>,
    script: VecDeque<Connect>,
    files: Vec<(Item, Secret)>,
    heard: Vec<Heard>,
    connects: Vec<Duration>,
    connection_addresses: Vec<skein_io::Addr>,
    aborts: u32,
    seed: u64,
    needs: Vec<Need>,
    pause_writes: bool,
    pending_writes: Vec<(Token, Item, Secret)>,
    pause_reads: bool,
    pending_reads: Vec<(Token, Item)>,
}
impl HostWorld {
    #[must_use]
    pub fn new(seed: u64, script: &[Connect]) -> Self {
        Self::new_with(seed, script, fixtures::host_settings(), fixtures::host_limits())
    }
    #[must_use]
    pub fn new_with(seed: u64, script: &[Connect], settings: Settings, limits: Limits) -> Self {
        Self::with_settings_and_limits(seed, script, fixtures::config(), settings, limits)
    }
    #[must_use]
    pub fn with_config(seed: u64, script: &[Connect], config: fake::Config) -> Self {
        Self::with_settings(seed, script, config, fixtures::host_settings())
    }
    #[must_use]
    pub fn with_settings(
        seed: u64,
        script: &[Connect],
        config: fake::Config,
        settings: skein_forge_setup::host::Settings,
    ) -> Self {
        Self::with_settings_and_limits(seed, script, config, settings, fixtures::host_limits())
    }
    fn with_settings_and_limits(
        seed: u64,
        script: &[Connect],
        config: fake::Config,
        settings: Settings,
        limits: Limits,
    ) -> Self {
        let (service, fake_limits) = seeded_config(seed, false, config);
        assert_eq!(skein_forge_setup::host::documents(&limits), fake_limits.documents);
        Self {
            host: Host::new(&limits, settings, fixtures::crypto()).expect("host fixtures"),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            service,
            fake_env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: fake_limits },
            above: Queue::with_capacity(16),
            below: Queue::with_capacity(16),
            peers: BTreeMap::new(),
            script: script.iter().copied().collect(),
            files: vec![(Item::AdminToken, fixtures::secret(fixtures::ADMIN))],
            heard: Vec::new(),
            connects: Vec::new(),
            connection_addresses: Vec::new(),
            aborts: 0,
            seed,
            needs: Vec::new(),
            pause_writes: false,
            pending_writes: Vec::new(),
            pause_reads: false,
            pending_reads: Vec::new(),
        }
    }
    #[must_use]
    pub fn connection_addresses(&self) -> &[skein_io::Addr] {
        &self.connection_addresses
    }
    pub fn run(&mut self, run: u64) {
        self.run_adopted(run, &[]);
    }
    pub fn run_adopted(&mut self, run: u64, adopted: &[u64]) {
        self.run_plan(run, Plan::Reconcile { forge: 1, adopted: adopted.into() });
    }
    pub fn run_plan(&mut self, run: u64, plan: Plan) {
        self.action(Request::Run { run: Token::new(run), plan });
    }
    #[must_use]
    pub fn store(&self) -> &fake::setup::Store {
        self.service.setup()
    }
    pub fn store_mut(&mut self) -> &mut fake::setup::Store {
        self.service.setup_mut()
    }
    pub fn put(&mut self, item: Item, bytes: Secret) {
        if let Some((_, value)) = self.files.iter_mut().find(|(key, _)| *key == item) {
            *value = bytes;
        } else {
            self.files.push((item, bytes));
        }
    }
    #[must_use]
    pub fn needs(&self) -> &[Need] {
        &self.needs
    }
    pub fn pause_reads(&mut self) {
        self.pause_reads = true;
    }
    #[must_use]
    pub fn pending_reads(&self) -> &[(Token, Item)] {
        &self.pending_reads
    }
    /// Answers the chosen run's held read from the owner's current items.
    pub fn read_pending(&mut self, run: u64) {
        let run = Token::new(run);
        let at = self.pending_reads.iter().position(|(token, _)| *token == run).expect("held read");
        let (_, item) = self.pending_reads.remove(at);
        self.action(Request::Answer { run, answer: Answer::Read(self.file(item).cloned()) });
    }
    pub fn resume_reads(&mut self) {
        self.pause_reads = false;
        for (run, item) in core::mem::take(&mut self.pending_reads) {
            self.action(Request::Answer { run, answer: Answer::Read(self.file(item).cloned()) });
        }
    }
    pub fn pause_writes(&mut self) {
        self.pause_writes = true;
    }
    /// Keeps each item whole before answering Written, as the owner must.
    pub fn write_pending(&mut self) {
        for (run, item, bytes) in core::mem::take(&mut self.pending_writes) {
            self.put(item, bytes);
            self.action(Request::Answer { run, answer: Answer::Written });
        }
    }
    #[must_use]
    pub fn requests(&self) -> &fake::Tally {
        self.service.tally()
    }
    pub fn close(&mut self) {
        self.action(Request::Close);
    }
    fn action(&mut self, request: Request) {
        self.host.down(&self.env, request, &mut self.above, &mut self.below);
    }
    fn up(&mut self, event: skein_io::Event) {
        self.host.up(&self.env, event, &mut self.above, &mut self.below);
    }
    #[must_use]
    pub fn heard(&self) -> &[Heard] {
        &self.heard
    }
    #[must_use]
    pub fn connects(&self) -> &[Duration] {
        &self.connects
    }
    #[must_use]
    pub fn aborts(&self) -> u32 {
        self.aborts
    }
    #[must_use]
    pub fn file(&self, item: Item) -> Option<&Secret> {
        self.files.iter().find(|(key, _)| *key == item).map(|(_, value)| value)
    }
    /// Completes all work at the present virtual time.
    pub fn settle(&mut self) {
        for _ in 0..100_000 {
            self.tick();
            if self.above.is_empty()
                && self.below.is_empty()
                && !self
                    .peers
                    .values()
                    .any(|peer| peer.pipe.has_work() || !peer.above.is_empty() || !peer.below.is_empty())
                && self.host.next_deadline().is_none_or(|at| at > self.env.now)
            {
                return;
            }
        }
        panic!("host world's same-time work did not settle");
    }
    /// Advances only to the host's next deadline until every run is idle.
    pub fn finish(&mut self) {
        for _ in 0..4096 {
            self.settle();
            let Some(at) = self.host.next_deadline() else {
                return;
            };
            assert!(at >= self.env.now);
            self.env.now = at;
            self.fake_env.now = at;
            self.host.fire(&self.env, &mut self.above, &mut self.below);
        }
        panic!("host world's deadlines did not settle");
    }
    fn tick(&mut self) {
        self.events();
        while let Some(request) = self.below.pop() {
            self.io(request);
        }
        let owners: Vec<_> = self.peers.keys().copied().collect();
        for owner in owners {
            let peer = self.peers.get_mut(&owner).expect("peer");
            if let Some(server) = &mut peer.server {
                fake::resume(server, &mut self.service, &self.fake_env, &mut peer.above, &mut peer.below);
            }
            while let Some(down) = peer.below.pop() {
                peer.pipe.server_down(down);
            }
            if let Some(event) = peer.pipe.server_up() {
                if let Some(server) = &mut peer.server {
                    fake::up(server, &mut self.service, &self.fake_env, event, &mut peer.above, &mut peer.below);
                } else if peer.answer.is_some() {
                    answer(peer, event);
                }
            }
            if let Some(up) = peer.pipe.client_up() {
                self.up(skein_io::Event::Stream { owner, up });
            }
            let peer = self.peers.get_mut(&owner).expect("peer");
            while let Some(event) = peer.above.pop() {
                match event {
                    fake::Event::Domain(_) => panic!("setup has no repository domain requests"),
                    fake::Event::Close => {
                        fake::closed(
                            peer.server.as_mut().expect("fake server"),
                            &mut self.service,
                            &self.fake_env,
                            &mut peer.above,
                            &mut peer.below,
                        );
                    }
                    fake::Event::Closed => (),
                }
            }
        }
        if self.host.next_deadline().is_some_and(|at| at <= self.env.now) {
            self.host.fire(&self.env, &mut self.above, &mut self.below);
        }
        self.service.reclaim();
    }
    fn events(&mut self) {
        while let Some(event) = self.above.pop() {
            let at = self.env.now.saturating_since(Time::ZERO);
            match event {
                Event::Need { run, need } => {
                    self.needs.push(need.clone());
                    let answer = match need {
                        Need::Read(item) => {
                            if self.pause_reads {
                                self.pending_reads.push((run, item));
                                continue;
                            }
                            Answer::Read(self.file(item).cloned())
                        }
                        Need::Write { item, bytes } => {
                            if self.pause_writes {
                                self.pending_writes.push((run, item, bytes));
                                continue;
                            }
                            self.put(item, bytes);
                            Answer::Written
                        }
                    };
                    self.action(Request::Answer { run, answer });
                }
                Event::Done { run, outcome } => self.heard.push(Heard::Done { run, outcome, at }),
                Event::Stop { run, failure } => self.heard.push(Heard::Stop { run, failure, at }),
                Event::Closed => self.heard.push(Heard::Closed { at }),
            }
        }
    }
    fn io(&mut self, request: skein_io::Request) {
        match request {
            skein_io::Request::Connect { owner, addr } => {
                self.connection_addresses.push(addr);
                self.connects.push(self.env.now.saturating_since(Time::ZERO));
                let connect = self.script.pop_front().unwrap_or(Connect::Made);
                self.up(skein_io::Event::Connecting { owner, socket: owner });
                if matches!(connect, Connect::Refused) {
                    self.up(skein_io::Event::Failed { owner, error: skein_io::Error::Refused });
                    self.up(skein_io::Event::Closed { owner });
                    return;
                }
                let mut peer = Peer {
                    pipe: Pipe::new(self.seed ^ owner.raw(), Cut::Never),
                    server: None,
                    above: Queue::with_capacity(16),
                    below: Queue::with_capacity(16),
                    answer: match connect {
                        Connect::Unavailable => Some((503, b"")),
                        Connect::Answers(status, body) => Some((status, body)),
                        Connect::Made | Connect::Refused | Connect::Silent => None,
                    },
                    head: Vec::new(),
                    responded: false,
                };
                if matches!(connect, Connect::Made) {
                    let mut server = fake::Server::new(owner, &self.fake_env.limits).expect("server");
                    fake::start(&mut server, &mut self.service, &self.fake_env, &mut peer.above, &mut peer.below);
                    peer.server = Some(server);
                } else if peer.answer.is_some() {
                    peer.below.push(Down::Demand { read: Read::Arrived { max: 4096 }, room: 0 });
                }
                assert!(self.peers.insert(owner, peer).is_none());
                if !matches!(connect, Connect::Silent) {
                    self.up(skein_io::Event::Connected { owner });
                }
            }
            skein_io::Request::Stream { stream, down } => {
                if let Some(peer) = self.peers.get_mut(&stream) {
                    peer.pipe.client_down(down);
                }
            }
            skein_io::Request::Abort { entity } => {
                self.aborts += 1;
                self.end(entity);
            }
            skein_io::Request::Close { entity } => self.end(entity),
            skein_io::Request::Listen { .. }
            | skein_io::Request::Bind { .. }
            | skein_io::Request::Reject { .. }
            | skein_io::Request::Output { .. }
            | skein_io::Request::Spawn { .. }
            | skein_io::Request::Signal { .. }
            | skein_io::Request::Usage { .. }
            | skein_io::Request::Window { .. } => panic!("unexpected io request"),
        }
    }
    fn end(&mut self, owner: Token) {
        if let Some(mut peer) = self.peers.remove(&owner)
            && let Some(server) = &mut peer.server
        {
            fake::closed(server, &mut self.service, &self.fake_env, &mut peer.above, &mut peer.below);
        }
        self.up(skein_io::Event::Closed { owner });
    }
}
fn answer(peer: &mut Peer, event: Up) {
    match event {
        Up::Bytes(bytes) => {
            peer.head.extend_from_slice(&bytes);
            if peer.head.windows(4).any(|piece| piece == b"\r\n\r\n") {
                peer.below.push(Down::Demand { read: Read::Nothing, room: 4096 });
            } else {
                peer.below.push(Down::Demand { read: Read::Arrived { max: 4096 }, room: 0 });
            }
        }
        Up::Room => {
            assert!(!peer.responded);
            peer.responded = true;
            let (status, body) = peer.answer.expect("scripted answer");
            peer.below.push(Down::Send(skein_forge_machine_world::fixtures::response(status, body).into_boxed_slice()));
            peer.below.push(Down::Finish);
        }
        Up::End => (),
        Up::Failed(_) => panic!("uncut scripted stream"),
    }
}
