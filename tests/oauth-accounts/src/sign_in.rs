//! Actual accounts sign-in with an independent issuer and scripted browser.
//! The shared world owns schedules, replay and process heaps (oauth.md, 6.8).
use skein_fake_oauth as fake;
use skein_fake_peers::{Transport, oauth as peer};
use skein_io::{self as io, kernel};
use skein_lib::{Duration, Env, List, Queue, Time, Wall, bytes};
use skein_oauth as oauth;
use skein_oauth_accounts as accounts;
use skein_world::domain::Ledger;
use skein_world::{Host, Memory, Outcome, Referee, World};
use std::net::{Ipv4Addr, SocketAddr};
mod browser;
#[cfg(test)]
mod cuts;
mod expected;
mod issuer;
mod owner;
mod protocol;
pub use browser::Browser;
use issuer::issuer;
pub use issuer::key_set;
pub use protocol::protocol;
const ROOM: u32 = 64;
fn io_limits() -> io::Limits {
    super::world::limits().io
}

/// The outside owner's actions during one sign-in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Story {
    /// Two sign-ins fetch fresh issuer documents before their visits.
    OpenIdAgain,
    /// An issued client id completes a new registration.
    IssuedClientId,
    /// The returned subject differs from the owner's expectation.
    AnotherSubject,
    /// Sign-in keeps declined scopes, then its grant is refused.
    RequiredScopes,
    /// Public sign-in followed by a rotating refresh.
    Public,
    /// The preferred loopback port is held by another listener.
    VaryingTaken,
    /// Two sign-ins wait together on their own ephemeral ports.
    TwoPorts,
    /// The owner's web server hands in a confidential redirect.
    Confidential,
    /// A wrong path is refused before the real callback.
    WrongPath,
    /// Another authority is refused before the real callback.
    WrongHost,
    /// An oversized request head is refused before the real callback.
    LongHead,
    /// Close keeps waiting for the person's redirect, then drains.
    CloseWaiting,
    /// A confidential redirect completes the admitted sign-in through a close.
    CloseConfidential,
    /// Owner cancel ends its sign-in and listener.
    Cancel,
    /// Abort after close cancels the sign-in.
    Abort,
    /// A keeper failure is the sign-in's failure terminal.
    NotKept,
    /// The shipped person deadline ends a sign-in that receives no redirect.
    Timeout,
}
fn confidential(story: Story) -> bool {
    matches!(story, Story::Confidential | Story::CloseConfidential)
}
/// The sign-in story's registration, shared by component heap fixtures.
#[must_use]
pub fn registration(story: Story) -> oauth::Registration {
    let mut registration = super::world::registration(Transport::Plaintext);
    if confidential(story) {
        registration.client_secret = Some(bytes::copy_of(b"fake-secret"));
        registration.redirect_uri = bytes::copy_of(b"https://service.example/callback");
    }
    if matches!(story, Story::VaryingTaken | Story::TwoPorts) {
        registration.redirect_uri = bytes::copy_of(if story == Story::TwoPorts {
            b"http://127.0.0.1:0/callback"
        } else {
            b"http://127.0.0.1:31234/callback"
        });
        registration.terms = Some(oauth::Terms { varying_port: true, ..crate::world::terms() });
    }
    if story == Story::RequiredScopes {
        registration.scope = bytes::copy_of(REQUIRED_SCOPES);
        registration.terms =
            Some(oauth::Terms { required_scopes: Some(bytes::copy_of(REQUIRED_SCOPES)), ..crate::world::terms() });
    }
    if matches!(story, Story::OpenIdAgain | Story::IssuedClientId | Story::AnotherSubject) {
        registration.scope = bytes::copy_of(b"openid");
        registration.terms = Some(oauth::Terms {
            issued_client_id: story == Story::IssuedClientId,
            openid: Some(oauth::OpenId { issuer: bytes::copy_of(ISSUER), key_set: None }),
            ..crate::world::terms()
        });
    }
    registration
}
/// Content-free outside facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fact {
    /// Authorization URL was ready to show.
    Visit,
    /// A new record reached the durable keeper.
    Keep(u64),
    /// The kept sign-in completed.
    SignedIn(u64),
    /// Required scopes declined by the issuer.
    Missing(Box<[u8]>),
    /// The owner heard a usable grant.
    Granted(u64),
    /// An admitted request failed.
    Failed(accounts::Failure),
    /// A late owner redirect was refused.
    Late,
    /// The component settled all children.
    Closed,
}
/// The sign-in terminal referee uses the shared request ledger.
pub struct Contracts {
    sign_in: Ledger<u32, ()>,
    keeps: Ledger<u64, ()>,
    kept: Vec<u64>,
    closed: bool,
}
impl Contracts {
    /// Starts with one admitted sign-in.
    #[must_use]
    pub fn new() -> Self {
        let mut sign_in = Ledger::new("sign-in and keeper terminals");
        sign_in.open(0, ());
        Self { sign_in, keeps: Ledger::new("sign-in and keeper terminals"), kept: Vec::new(), closed: false }
    }
    /// Records a candidate that has no keeper answer yet.
    pub fn keep(&mut self, generation: u64) {
        self.keeps.open(generation, ());
    }
    /// Records the owner's durable answer.
    pub fn kept(&mut self, generation: u64, yes: bool) {
        self.keeps.end(generation);
        if yes {
            self.kept.push(generation);
        }
    }
    /// Success must follow keeping and end the admitted sign-in once.
    pub fn signed_in(&mut self, generation: u64) {
        assert!(self.kept.contains(&generation), "signed in before kept");
        self.sign_in.end(0);
    }
    /// A failed sign-in has the same one-terminal rule.
    pub fn failed(&mut self) {
        self.sign_in.end(0);
    }
    /// Lending requires durable keeping.
    pub fn granted(&mut self, generation: u64) {
        assert!(self.kept.contains(&generation), "grant before kept");
    }
    /// Closed follows all request and keeper terminals once.
    pub fn closed(&mut self) {
        assert!(!self.closed);
        self.sign_in.assert_settled();
        self.keeps.assert_settled();
        self.closed = true;
    }
}
impl Default for Contracts {
    fn default() -> Self {
        Self::new()
    }
}
/// The actual owner adapter routes io to the accounts component.
pub struct Client {
    io: io::Io,
    component: Option<accounts::Component>,
    limits: accounts::Limits,
    events: Queue<io::Event>,
    requests: Queue<io::Request>,
    files: Queue<accounts::FileLower>,
    file_driver: Option<skein_world::files::Driver>,
    file_keep: Option<(skein_lib::Token, oauth::SavedToken)>,
    opened_private: Option<Time>,
    stored_private: Option<Time>,
    above: Queue<accounts::Event>,
    completions: Queue<kernel::Complete>,
    submissions: Queue<kernel::Submit>,
    address: Option<SocketAddr>,
    story: Story,
    facts: Vec<Fact>,
    records: Vec<oauth::SavedToken>,
    contracts: Vec<Contracts>,
    visits: Vec<Box<[u8]>>,
    closing: bool,
    closed: bool,
    large: bool,
    redirected: [u8; 1024],
    redirected_len: usize,
    again_at: Option<Time>,
}
impl Client {
    fn new(story: Story, large: bool) -> Self {
        Self::with_limits(story, large, &super::world::limits(), ROOM)
    }
    fn with_limits(story: Story, large: bool, limits: &accounts::Limits, room: u32) -> Self {
        let mut limits = *limits;
        if story == Story::TwoPorts {
            limits.listeners = 2;
            limits.io.sockets = 6;
        }
        Self {
            io: io::Io::new(&limits.io),
            component: None,
            limits,
            events: Queue::with_capacity(room),
            requests: Queue::with_capacity(room),
            files: Queue::with_capacity(room),
            file_driver: None,
            file_keep: None,
            opened_private: None,
            stored_private: None,
            above: Queue::with_capacity(room),
            completions: Queue::with_capacity(room),
            submissions: Queue::with_capacity(room),
            address: None,
            story,
            facts: Vec::with_capacity(16),
            records: Vec::with_capacity(2),
            contracts: (0..if story == Story::TwoPorts { 2 } else { 1 }).map(|_| Contracts::new()).collect(),
            visits: Vec::with_capacity(2),
            closing: false,
            closed: false,
            large,
            redirected: [0; 1024],
            redirected_len: 0,
            again_at: None,
        }
    }
    fn private_io(&mut self, env: &Env<accounts::Limits>) {
        if self.file_driver.is_none() {
            return;
        }
        while let Some(lower) = self.files.pop() {
            match lower {
                accounts::FileLower::Request { request, deadline } => {
                    if let io::file::Request::Store { owner, bytes, .. } = &request {
                        let record =
                            oauth::decode_record(bytes, &env.limits.client.document).expect("candidate record");
                        self.contracts[0].keep(record.generation);
                        self.facts.push(Fact::Keep(record.generation));
                        assert!(self.file_keep.is_none());
                        self.file_keep = Some((*owner, record));
                    }
                    self.file_driver.as_mut().expect("file driver").request(request, deadline);
                }
                accounts::FileLower::Remove { owner, root, name, directory, deadline } => {
                    let driver = self.file_driver.as_mut().expect("file driver");
                    let root = driver.descriptor(root).expect("private root");
                    driver.request(io::file::Request::Remove { owner, root, name, directory }, deadline);
                }
                accounts::FileLower::Cancel { owner } => self.file_driver.as_mut().expect("file driver").cancel(owner),
            }
        }
        self.file_driver.as_mut().expect("file driver").progress(env.now);
        while let Some(event) = self.file_driver.as_mut().expect("file driver").take_event() {
            if event.owner() == skein_lib::Token::new(u64::MAX) {
                continue;
            }
            if matches!(event, io::file::Event::Opened { .. }) {
                self.opened_private = Some(env.now);
            }
            if matches!(event, io::file::Event::Stored { .. }) {
                self.stored_private = Some(env.now);
            }
            if self.file_keep.as_ref().is_some_and(|(owner, _)| *owner == event.owner()) {
                let (_, record) = self.file_keep.take().expect("matching keep");
                self.contracts[0].kept(record.generation, matches!(event, io::file::Event::Stored { .. }));
                self.records.push(record);
            }
            self.component.as_mut().expect("started component").filed(
                env,
                event,
                &mut self.above,
                &mut self.requests,
                &mut self.files,
            );
        }
        while let Some(submit) = self.file_driver.as_mut().expect("file driver").take_submit() {
            self.submissions.push(submit);
        }
    }
}
impl Host for Client {
    fn iterate(&mut self, now: Time, wall: Wall) {
        let env = Env { now, wall, limits: self.limits };
        self.restart(&env);
        if self.component.is_none() && self.address.is_some() {
            self.start(&env);
        }
        if self.redirected_len > 0 {
            let uri = bytes::copy_of(&self.redirected[..self.redirected_len]);
            self.redirected_len = 0;
            self.ask(&env, accounts::Request::Redirected { account: 0, uri });
        }
        let io_env = Env { now, wall, limits: self.limits.io };
        self.private_io(&env);
        for _ in 0..ROOM {
            if self.events.room() < 3 || self.submissions.room() < 2 || !self.io.is_ready() {
                break;
            }
            io::resume(&mut self.io, &io_env, &mut self.events, &mut self.submissions);
        }
        for _ in 0..ROOM {
            if self.events.room() < 3 || self.submissions.room() < 2 {
                break;
            }
            let Some(complete) = self.completions.pop() else { break };
            if skein_world::files::Driver::owns(complete.op) {
                self.file_driver.as_mut().expect("tagged file completion").up(complete);
            } else {
                io::up(&mut self.io, &io_env, complete, &mut self.events, &mut self.submissions);
            }
        }
        for _ in 0..ROOM {
            if self.events.room() < 3 || self.submissions.room() < 2 || !self.io.is_due(now) {
                break;
            }
            io::fire(&mut self.io, &io_env, &mut self.events, &mut self.submissions);
        }
        for _ in 0..ROOM {
            if self.above.room() < accounts::MAX_OUT_UP.above || self.requests.room() < accounts::MAX_OUT_UP.io {
                break;
            }
            let Some(event) = self.events.pop() else { break };
            self.component.as_mut().expect("io follows started component").up(
                &env,
                event,
                &mut self.above,
                &mut self.requests,
                &mut self.files,
            );
        }
        if let Some(component) = &mut self.component
            && self.above.room() >= accounts::MAX_OUT_FIRE.above
            && self.requests.room() >= accounts::MAX_OUT_FIRE.io
            && (component.has_work() || component.next_deadline().is_some_and(|due| due <= now))
        {
            component.fire(&env, &mut self.above, &mut self.requests, &mut self.files);
        }
        if self.requests.room() >= accounts::MAX_OUT_DOWN.io && self.above.room() >= accounts::MAX_OUT_DOWN.above {
            self.observe(&env);
        }
        for _ in 0..ROOM {
            if !self.io.takes() || self.submissions.room() < 2 || self.above.room() < accounts::MAX_OUT_DOWN.above {
                break;
            }
            let Some(request) = self.requests.pop() else { break };
            io::down(&mut self.io, &io_env, request, &mut self.submissions);
        }
        self.private_io(&env);
        self.io.reclaim();
        if let Some(component) = &mut self.component {
            component.reclaim();
        }
    }
    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        &mut self.completions
    }
    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        &mut self.submissions
    }
    fn work_pending(&self, now: Time) -> bool {
        self.file_driver.as_ref().is_some_and(skein_world::files::Driver::has_work)
            || !self.files.is_empty()
            || self.io.is_ready()
            || !self.events.is_empty()
            || !self.requests.is_empty()
            || !self.above.is_empty()
            || !self.completions.is_empty()
            || self.redirected_len > 0
            || (self.component.is_none() && self.address.is_some())
            || self.component.as_ref().is_some_and(|component| {
                component.has_work() || component.next_deadline().is_some_and(|due| due <= now)
            })
    }
    fn next_deadline(&self) -> Option<Time> {
        [
            self.io.next_deadline(),
            self.again_at,
            self.component.as_ref().and_then(accounts::Component::next_deadline),
            self.file_driver.as_ref().and_then(skein_world::files::Driver::next_deadline),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn next_policy_deadline(&self) -> Option<Time> {
        self.component.as_ref().and_then(accounts::Component::next_deadline)
    }

    fn is_empty(&self) -> bool {
        self.closed
            && self.io.is_empty()
            && self.file_driver.as_ref().is_none_or(skein_world::files::Driver::is_empty)
            && self.events.is_empty()
            && self.requests.is_empty()
            && self.files.is_empty()
            && self.above.is_empty()
            && self.completions.is_empty()
            && self.submissions.is_empty()
    }
    fn worst_case(&self) -> u64 {
        accounts::worst_case(&self.limits).expect("component bound")
            + self.file_driver.as_ref().map_or(0, |driver| driver.worst_case(4))
            + io::worst_case(&self.limits.io).expect("io bound")
            + 512_000
    }
    fn operations(&self) -> u32 {
        io::operations(&self.limits.io).expect("io operations")
    }
}

/// The two actual processes of a component world.
pub enum Process {
    /// The owner-driven component with its own io loop.
    Client(Box<Client>),
    /// The independent scripted issuer, which stays live until the owner settles.
    Issuer(Box<peer::Peer>),
    /// The browser receives the authorization redirect and callback page.
    Browser(Box<Browser>),
}

impl Host for Process {
    fn iterate(&mut self, now: Time, wall: Wall) {
        match self {
            Process::Client(client) => client.iterate(now, wall),
            Process::Issuer(peer) => peer.iterate(now, wall),
            Process::Browser(browser) => browser.iterate(now, wall),
        }
    }
    fn completions(&mut self) -> &mut Queue<kernel::Complete> {
        match self {
            Process::Client(client) => client.completions(),
            Process::Issuer(peer) => peer.completions(),
            Process::Browser(browser) => browser.completions(),
        }
    }
    fn submissions(&mut self) -> &mut Queue<kernel::Submit> {
        match self {
            Process::Client(client) => client.submissions(),
            Process::Issuer(peer) => peer.submissions(),
            Process::Browser(browser) => browser.submissions(),
        }
    }
    fn work_pending(&self, now: Time) -> bool {
        match self {
            Process::Client(client) => client.work_pending(now),
            Process::Issuer(peer) => peer.work_pending(now),
            Process::Browser(browser) => browser.work_pending(now),
        }
    }
    fn next_deadline(&self) -> Option<Time> {
        match self {
            Process::Client(client) => client.next_deadline(),
            Process::Issuer(peer) => peer.next_deadline(),
            Process::Browser(browser) => browser.next_deadline(),
        }
    }

    fn next_policy_deadline(&self) -> Option<Time> {
        match self {
            Process::Client(client) => client.next_policy_deadline(),
            Process::Issuer(peer) => peer.next_policy_deadline(),
            Process::Browser(browser) => browser.next_policy_deadline(),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Process::Client(client) => client.is_empty(),
            Process::Issuer(peer) => peer.is_empty(),
            Process::Browser(browser) => browser.is_empty(),
        }
    }
    fn worst_case(&self) -> u64 {
        match self {
            Process::Client(client) => client.worst_case(),
            Process::Issuer(peer) => peer.worst_case(),
            Process::Browser(browser) => browser.worst_case(),
        }
    }
    fn operations(&self) -> u32 {
        match self {
            Process::Client(client) => client.operations(),
            Process::Issuer(peer) => peer.operations(),
            Process::Browser(browser) => browser.operations(),
        }
    }
}

#[expect(clippy::struct_excessive_bools, reason = "independent outside delivery, shutdown and success observations")]
struct Judge {
    delivered: usize,
    redirected: bool,
    shutdown: bool,
    wake: Option<Time>,
    passed: bool,
    story: Story,
    private_failure: bool,
}
impl Referee<Process> for Judge {
    fn act(&mut self, _now: Time, processes: &mut [Process]) {
        let address = processes.iter().find_map(|process| match process {
            Process::Issuer(peer) => peer.address(),
            Process::Client(_) | Process::Browser(_) => None,
        });
        let holder_ready =
            processes.iter().any(|process| matches!(process, Process::Browser(browser) if browser.holder.is_some()));
        let mut visit = [0; 1024];
        let mut visit_len = 0;
        let mut redirect = [0; 1024];
        let mut redirect_len = 0;
        for process in processes.iter() {
            match process {
                Process::Client(client) => {
                    if let Some(url) = client.visits.get(self.delivered) {
                        visit_len = url.len();
                        visit[..visit_len].copy_from_slice(url);
                    }
                }
                Process::Browser(browser) => {
                    if let Some(uri) = &browser.redirect {
                        redirect_len = uri.len();
                        redirect[..redirect_len].copy_from_slice(uri);
                    }
                }
                Process::Issuer(_) => {}
            }
        }
        let settled = processes.iter().filter(|process| !matches!(process, Process::Issuer(_))).all(Host::is_empty);
        for process in processes {
            match process {
                Process::Client(client) => {
                    if self.story != Story::VaryingTaken || holder_ready {
                        client.address = address;
                    }
                    if confidential(self.story) && !self.redirected && redirect_len > 0 {
                        client.redirected[..redirect_len].copy_from_slice(&redirect[..redirect_len]);
                        client.redirected_len = redirect_len;
                        self.redirected = true;
                    }
                }
                Process::Browser(browser) => {
                    if !matches!(self.story, Story::Cancel | Story::Abort | Story::Timeout) && visit_len > 0 {
                        browser.inbox[..visit_len].copy_from_slice(&visit[..visit_len]);
                        browser.inbox_len = visit_len;
                        self.delivered += 1;
                    }
                }
                Process::Issuer(peer) => {
                    if settled && !self.shutdown {
                        peer.shutdown();
                        self.shutdown = true;
                    }
                }
            }
        }
    }
    fn observe(&mut self, now: Time, processes: &[Process]) {
        let client = processes
            .iter()
            .find_map(|process| match process {
                Process::Client(client) => Some(client),
                Process::Browser(_) | Process::Issuer(_) => None,
            })
            .expect("owner");
        let browser = processes
            .iter()
            .find_map(|process| match process {
                Process::Browser(browser) => Some(browser),
                Process::Client(_) | Process::Issuer(_) => None,
            })
            .expect("browser");
        let issuer = processes
            .iter()
            .find_map(|process| match process {
                Process::Issuer(peer) => Some(peer),
                Process::Client(_) | Process::Browser(_) => None,
            })
            .expect("issuer");
        self.wake = ((client.component.is_none()
            && issuer.address().is_some()
            && (self.story != Story::VaryingTaken || browser.holder.is_some()))
            || (client.visits.len() > self.delivered
                && !matches!(self.story, Story::Cancel | Story::Abort | Story::Timeout))
            || (confidential(self.story) && !self.redirected && browser.redirect.is_some())
            || (!self.shutdown && client.is_empty() && browser.is_empty()))
        .then_some(now);
        if client.closed {
            self.client_closed(client, issuer);
        }
        let posts = issuer
            .observations()
            .iter()
            .filter(|observation| matches!(observation, peer::Observation::Post { .. }))
            .count();
        assert!(
            posts <= if matches!(self.story, Story::Public | Story::TwoPorts | Story::OpenIdAgain) { 2 } else { 1 }
        );
        if !self.private_failure
            && client.closed
            && browser.is_empty()
            && !matches!(self.story, Story::Cancel | Story::Abort | Story::Timeout)
        {
            assert!(browser.done);
            assert_eq!(
                posts,
                if matches!(self.story, Story::Public | Story::TwoPorts | Story::OpenIdAgain) { 2 } else { 1 }
            );
        }
        self.passed = self.shutdown && processes.iter().all(Host::is_empty);
    }
    fn next_deadline(&self) -> Option<Time> {
        self.wake.or(Some(Time::from_nanos(180_000_000_000)))
    }
    fn overdue(&self, now: Time) -> Option<String> {
        (now >= Time::from_nanos(180_000_000_000) && !self.passed).then(|| "accounts sign-in did not settle".to_owned())
    }
    fn passed(&self) -> bool {
        self.passed
    }
}
/// Runs sign-in with the actual component and independent socket processes.
#[must_use]
pub fn run(seed: u64, story: Story, faulted: bool, large: bool, memory: Memory) -> Outcome<Process> {
    let mut config = skein_sim::Config::calm();
    if faulted {
        config.buffer = 127;
        config.faults.short_recv = 300;
        config.faults.short_send = 300;
    }
    let mut world = World::new(
        seed,
        config,
        Judge {
            delivered: 0,
            redirected: false,
            shutdown: false,
            wake: None,
            passed: false,
            story,
            private_failure: false,
        },
        memory,
    );
    world.spawn(|| issuer(story, large));
    world.spawn(|| Process::Client(Box::new(Client::new(story, large))));
    world.spawn(|| Process::Browser(Box::new(Browser::new(story))));
    world.run()
}
/// Reads owner facts from a simulated outcome.
#[must_use]
pub fn facts(processes: &[Process]) -> &[Fact] {
    processes
        .iter()
        .find_map(|process| match process {
            Process::Client(client) => Some(client.facts.as_slice()),
            Process::Issuer(_) | Process::Browser(_) => None,
        })
        .expect("client")
}
/// Compares the actual wire trace, owner terminals and issuer observations.
pub fn assert_replay(first: &Outcome<Process>, second: &Outcome<Process>) {
    assert_eq!(first.trace, second.trace);
    for (first, second) in first.procs.iter().zip(&second.procs) {
        match (first, second) {
            (Process::Client(first), Process::Client(second)) => {
                assert_eq!(first.facts, second.facts);
                assert!(first.records == second.records);
                assert!(first.visits == second.visits);
            }
            (Process::Issuer(first), Process::Issuer(second)) => assert!(first.observations() == second.observations()),
            (Process::Browser(first), Process::Browser(second)) => {
                assert_eq!(first.done, second.done);
                assert!(first.redirect == second.redirect);
            }
            _ => panic!("process order"),
        }
    }
}

/// Signs in with the private keeper on the actual minimal machine and independent issuer/browser.
#[must_use]
pub fn run_private(seed: u64, private: crate::private::Story) -> Outcome<Process, crate::private::FileSystem> {
    let mut files = crate::private::FileSystem::new(private);
    let root = files.startup();
    let story = Story::Confidential;
    let private_failure = matches!(
        private,
        crate::private::Story::RootMode
            | crate::private::Story::FileMode
            | crate::private::Story::FileOwner
            | crate::private::Story::FileLinks
            | crate::private::Story::FileLink
            | crate::private::Story::LoadFailed
            | crate::private::Story::LoadStalled
            | crate::private::Story::TooLarge
    );
    let mut config = skein_sim::Config::calm();
    config.wall = Wall::EPOCH;
    let mut world = World::new(
        seed,
        config,
        Judge { delivered: 0, redirected: false, shutdown: false, wake: None, passed: false, story, private_failure },
        Memory::Checked,
    )
    .with_machine(files);
    world.spawn(|| issuer(story, false));
    world.spawn_root(root, |fd| {
        let mut client = Client::new(story, false);
        client.file_driver = Some(skein_world::files::Driver::new(
            fd,
            4,
            client.limits.client.document.record_bytes,
            client.limits.file_stall,
            1234,
        ));
        Process::Client(Box::new(client))
    });
    world.spawn(|| Process::Browser(Box::new(Browser::new(story))));
    let mut stalled = false;
    let mut outcome = world.run_with_faults(|_, _, submissions| {
        let stall = private == crate::private::Story::LoadStalled
            && !stalled
            && submissions.iter().any(|submit| matches!(submit.kind, kernel::Op::Read { .. }));
        stalled |= stall;
        Some(skein_sim::Faults { hung: if stall { 1000 } else { 0 }, ..skein_sim::Faults::NONE })
    });
    outcome.machine.faulted |= stalled;
    outcome
}

const REQUIRED_SCOPES: &[u8] = b"read write";

pub(super) const ISSUER: &[u8] = b"http://127.0.0.1:31000";
pub(super) const KEYS: &[u8] = b"http://127.0.0.1:31000/keys";
pub(super) const SUBJECT: &[u8] = b"account-subject";
pub(super) const EXPECTED_SUBJECT: &[u8] = b"expected-subject";
pub(super) const ISSUED_CLIENT: &[u8] = b"issued-client";
