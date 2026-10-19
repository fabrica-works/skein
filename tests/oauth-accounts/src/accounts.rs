//! Added accounts on the shared owner, issuer, filesystem and stream worlds.

use crate::{private, world};
use skein_fake_peers::Transport;
use skein_io::{self as io, file};
use skein_lib::{Duration, Env, List, Time, Token, Wall, bytes};
use skein_oauth as oauth;
use skein_oauth_accounts as accounts;
use skein_world::{Host, Memory, Referee, World};
use std::net::Ipv4Addr;

/// The account state in which its owner requests removal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Story {
    Idle,
    Loading,
    SigningIn,
    Refreshing,
    Keeping,
    Holding,
    Wanted,
    WantedFailure,
}

/// Content-free owner outcomes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Fact {
    Removed(u32),
    Failed(accounts::Ends, accounts::Failure),
    Keep(u64),
    Granted(u64),
    Closed,
}

pub(crate) struct Owner {
    story: Story,
    pub(crate) facts: Vec<Fact>,
    removing: bool,
    reuse: bool,
    reused: bool,
    record: Option<oauth::SavedToken>,
}
const ACCOUNT: u32 = 7;
const SOCKET: Token = Token::new(99);
const PRIVATE_ROOT: Token = Token::new(100);

fn record(expiry: u64) -> oauth::SavedToken {
    oauth::SavedToken {
        key: ACCOUNT,
        generation: 0,
        access_token: bytes::copy_of(b"old"),
        refresh_token: Some(bytes::copy_of(b"seed")),
        metadata: None,
        expires_at: Wall::from_nanos(Duration::from_secs(expiry).as_nanos()),
        issued_client_id: None,
        granted_scopes: None,
        subject: None,
        id_token: None,
        earliest_refresh_at: None,
    }
}
fn client(story: Story, reuse: bool) -> world::Client {
    let mut client = world::Client::new(world::Story::Rotate, Transport::Plaintext);
    client.accounts = Some(Owner { story, facts: Vec::new(), removing: false, reuse, reused: false, record: None });
    client
}

fn configuration(client: &world::Client) -> accounts::Account {
    let owner = client.accounts.as_ref().expect("accounts story");
    if owner.story == Story::Idle {
        return accounts::Account::HandedIn;
    }
    let keeper = if owner.story == Story::Loading {
        accounts::Keeper::Private {
            root: client.file_driver.as_ref().map_or(SOCKET, skein_world::files::Driver::root),
            directory: bytes::copy_of(b"secret"),
            file: bytes::copy_of(b"record"),
        }
    } else {
        accounts::Keeper::Owner {
            kept: if owner.reused {
                owner.record.clone()
            } else {
                match owner.story {
                    Story::Holding | Story::Wanted => Some(record(30)),
                    Story::WantedFailure => {
                        let mut kept = record(30);
                        kept.refresh_token = Some(bytes::copy_of(b"revoked"));
                        Some(kept)
                    }
                    Story::Refreshing | Story::Keeping => Some(record(5)),
                    Story::Idle | Story::Loading | Story::SigningIn => None,
                }
            },
        }
    };
    accounts::Account::SignIn {
        origins: List::with_capacity(0),
        shared: false,
        registration: world::registration(Transport::Plaintext),
        endpoint: accounts::Endpoint {
            address: client.address.expect("issuer address"),
            transport: accounts::Transport::Plaintext,
        },
        keeper,
    }
}
fn add(client: &mut world::Client, env: &Env<accounts::Limits>) {
    let configuration = configuration(client);
    client.component.as_mut().expect("component").add(env, ACCOUNT, configuration, &mut client.above);
}
pub(crate) fn start(client: &mut world::Client, env: &Env<accounts::Limits>) {
    client.component = Some(
        accounts::Component::new(
            List::with_capacity(0),
            &env.limits,
            skein_lib::CryptoRng::new(&skein_world::crypto_seed(17), 0).expect("seed"),
        )
        .expect("empty accounts"),
    );
    add(client, env);
    match client.accounts.as_ref().expect("story").story {
        Story::Wanted | Story::WantedFailure => {
            client.ask(env, accounts::Request::Want { account: ACCOUNT, wanted: true });
        }
        Story::Idle => remove(client, env),
        Story::SigningIn => client.ask(env, accounts::Request::SignIn { account: ACCOUNT, subject: None }),
        Story::Loading | Story::Refreshing | Story::Keeping | Story::Holding => {
            client.ask(env, accounts::Request::Grant { account: ACCOUNT });
        }
    }
}
fn remove(client: &mut world::Client, env: &Env<accounts::Limits>) {
    let owner = client.accounts.as_mut().expect("story");
    if owner.removing {
        return;
    }
    owner.removing = true;
    client.ask(env, accounts::Request::Remove { account: ACCOUNT });
    // A reused account must remain open until its second removal.
    if !client.accounts.as_ref().expect("story").reuse {
        client.ask(env, accounts::Request::Close);
        client.closing = true;
    }
}
pub(crate) fn observe(client: &mut world::Client, env: &Env<accounts::Limits>) {
    let Some(event) = client.above.pop() else { return };
    match event {
        accounts::Event::Visit { .. } => remove(client, env),
        accounts::Event::Keep { account, record } => {
            let generation = record.generation;
            let owner = client.accounts.as_mut().expect("story");
            owner.facts.push(Fact::Keep(generation));
            owner.record = Some(record);
            if owner.story == Story::Keeping {
                remove(client, env);
            }
            client.ask(env, accounts::Request::Kept { account, generation, keeping: accounts::Keeping::Kept });
            if client.accounts.as_ref().expect("story").story == Story::Wanted {
                client.ask(env, accounts::Request::Close);
                client.closing = true;
            }
        }
        accounts::Event::Granted { generation, .. } => {
            client.accounts.as_mut().expect("story").facts.push(Fact::Granted(generation));
            remove(client, env);
        }
        accounts::Event::Failed { ends, failure, .. } => {
            client.accounts.as_mut().expect("story").facts.push(Fact::Failed(ends, failure));
            if client.accounts.as_ref().expect("story").story == Story::WantedFailure {
                client.ask(env, accounts::Request::Close);
                client.closing = true;
            }
        }
        accounts::Event::Removed { account } => {
            let owner = client.accounts.as_mut().expect("story");
            owner.facts.push(Fact::Removed(account));
            if owner.reuse && !owner.reused {
                owner.reused = true;
                owner.removing = false;
                add(client, env);
                client.ask(env, accounts::Request::Grant { account: ACCOUNT });
            } else if owner.reuse {
                client.ask(env, accounts::Request::Close);
                client.closing = true;
            }
        }
        accounts::Event::Closed => {
            client.accounts.as_mut().expect("story").facts.push(Fact::Closed);
            client.closed = true;
            if let Some(driver) = &mut client.file_driver {
                driver.close_root(env.now);
            }
        }
        accounts::Event::Lock { .. }
        | accounts::Event::Unlock { .. }
        | accounts::Event::SignedOut { .. }
        | accounts::Event::SignedIn { .. }
        | accounts::Event::Expiring { .. }
        | accounts::Event::Refused { .. } => {
            panic!("unexpected added-account terminal")
        }
    }
}
pub(crate) fn before_socket(client: &mut world::Client, env: &Env<accounts::Limits>, request: &io::Request) {
    if client.accounts.as_ref().is_some_and(|owner| owner.story == Story::Refreshing)
        && matches!(request, io::Request::Stream { down: skein_lib::stream::Down::Send(bytes), .. } if !bytes.is_empty())
    {
        remove(client, env);
    }
}
pub(crate) fn before_file(client: &mut world::Client, env: &Env<accounts::Limits>, request: &file::Request) {
    if client.accounts.as_ref().is_some_and(|owner| owner.story == Story::Loading)
        && matches!(request, file::Request::Load { .. })
    {
        remove(client, env);
    }
}

pub(crate) fn protocol_files(client: &mut world::Client, env: &Env<accounts::Limits>) {
    if client.accounts.is_none() {
        return;
    }
    if let Some(lower) = client.files.pop() {
        let event = match lower {
            accounts::FileLower::Request { request, .. } => {
                before_file(client, env, &request);
                match request {
                    file::Request::OpenPrivate { owner, .. } => {
                        file::Event::Opened { owner, file: PRIVATE_ROOT, len: 0 }
                    }
                    file::Request::Load { owner, .. } => file::Event::Loaded {
                        owner,
                        bytes: oauth::encode_record(&record(30), &env.limits.client.document).expect("kept record"),
                    },
                    file::Request::Close { owner, .. } => file::Event::Closed { owner },
                    file::Request::Lock { .. }
                    | file::Request::MakeDirectory { .. }
                    | file::Request::Create { .. }
                    | file::Request::CreateNoFollow { .. }
                    | file::Request::OpenRead { .. }
                    | file::Request::OpenReadNoFollow { .. }
                    | file::Request::OpenWrite { .. }
                    | file::Request::OpenWriteNoFollow { .. }
                    | file::Request::OpenDirectory { .. }
                    | file::Request::Scan { .. }
                    | file::Request::Store { .. }
                    | file::Request::Stat { .. }
                    | file::Request::WriteAt { .. }
                    | file::Request::ReadAt { .. }
                    | file::Request::Cut { .. }
                    | file::Request::Sync { .. }
                    | file::Request::SyncDirectory { .. }
                    | file::Request::Rename { .. }
                    | file::Request::Remove { .. }
                    | file::Request::List { .. }
                    | file::Request::ListPage { .. } => panic!("load and close only"),
                }
            }
            accounts::FileLower::Remove { .. } => panic!("no sign-out requested"),
            accounts::FileLower::Cancel { owner } => file::Event::Cancelled { owner },
        };
        client.component.as_mut().expect("component").filed(
            env,
            event,
            &mut client.above,
            &mut client.requests,
            &mut client.files,
        );
    }
}

pub(crate) struct Judge {
    delivered: bool,
    shutdown: bool,
    wake: Option<Time>,
    passed: bool,
}
impl Judge {
    pub(crate) fn new() -> Self {
        Self { delivered: false, shutdown: false, wake: None, passed: false }
    }
}
impl Referee<world::Process> for Judge {
    fn act(&mut self, _now: Time, processes: &mut [world::Process]) {
        let address = processes.iter().find_map(|p| match p {
            world::Process::Issuer(peer) => peer.address(),
            world::Process::Client(_) => None,
        });
        for process in processes.iter_mut() {
            if let world::Process::Client(client) = process {
                if !self.delivered
                    && let Some(address) = address
                {
                    client.address = Some(address);
                    self.delivered = true;
                }
                if client.is_empty() {
                    self.shutdown = true;
                }
            }
        }
        if self.shutdown {
            for process in processes.iter_mut() {
                if let world::Process::Issuer(peer) = process {
                    peer.shutdown();
                }
            }
        }
    }
    fn observe(&mut self, now: Time, processes: &[world::Process]) {
        let address_ready = processes.iter().any(|process| match process {
            world::Process::Issuer(peer) => peer.address().is_some(),
            world::Process::Client(_) => false,
        });
        let settled = processes.iter().any(|process| match process {
            world::Process::Client(client) => client.is_empty(),
            world::Process::Issuer(_) => false,
        });
        self.wake = ((!self.delivered && address_ready) || (settled && !self.shutdown)).then_some(now);
        self.passed = self.shutdown && processes.iter().all(Host::is_empty);
    }
    fn next_deadline(&self) -> Option<Time> {
        self.wake.or(Some(Time::from_nanos(180_000_000_000)))
    }
    fn overdue(&self, now: Time) -> Option<String> {
        (now >= Time::from_nanos(180_000_000_000) && !self.passed).then(|| "account removal did not settle".to_owned())
    }
    fn passed(&self) -> bool {
        self.passed
    }
}

/// Runs added accounts through the existing client and independent issuer over simulated io.
#[must_use]
pub fn run(seed: u64, story: Story, memory: Memory) -> skein_world::Outcome<world::Process, private::FileSystem> {
    run_with_reuse(seed, story, memory, false)
}
fn run_with_reuse(
    seed: u64,
    story: Story,
    memory: Memory,
    reuse: bool,
) -> skein_world::Outcome<world::Process, private::FileSystem> {
    let mut files = private::FileSystem::new(private::Story::Load);
    let root = files.startup();
    let mut config = skein_sim::Config::calm();
    config.wall = Wall::EPOCH;
    let mut world = World::new(seed, config, Judge::new(), memory).with_machine(files);
    world.spawn(|| world::issuer(Transport::Plaintext, world::Story::Rotate, (Ipv4Addr::LOCALHOST, 31000).into()));
    world.spawn_root(root, |fd| {
        let mut client = client(story, reuse);
        client.file_driver = Some(skein_world::files::Driver::new(
            fd,
            4,
            client.limits.client.document.record_bytes,
            client.limits.file_stall,
            1234,
        ));
        world::Process::Client(Box::new(client))
    });
    world.run()
}
/// Runs the same owner directly over the shared seeded HTTP streams.
#[must_use]
pub fn protocol(seed: u64, story: Story) -> (Vec<Fact>, Vec<String>) {
    let (client, trace) = world::protocol_client(seed, client(story, false));
    (client.accounts.expect("story").facts, trace)
}
/// Reads the added-account facts from a simulated outcome.
#[must_use]
pub fn facts(processes: &[world::Process]) -> &[Fact] {
    processes
        .iter()
        .find_map(|process| match process {
            world::Process::Client(client) => Some(client.accounts.as_ref().expect("story").facts.as_slice()),
            world::Process::Issuer(_) => None,
        })
        .expect("client")
}
/// Reuses a removed name with its last keeper record in both worlds.
#[must_use]
pub fn reused(seed: u64, memory: Memory) -> (Vec<Fact>, skein_world::Outcome<world::Process, private::FileSystem>) {
    let (client, _) = world::protocol_client(seed, client(Story::Keeping, true));
    (client.accounts.expect("story").facts, run_with_reuse(seed, Story::Keeping, memory, true))
}
