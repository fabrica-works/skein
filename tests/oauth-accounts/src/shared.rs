//! Shared records read under a scripted owner lock in both io worlds.

use crate::{accounts::Judge, private, world};
use skein_fake_peers::Transport;
use skein_lib::{Duration, Env, List, Wall, bytes};
use skein_oauth as oauth;
use skein_oauth_accounts as accounts;
use skein_world::{Memory, World};
use world::Fact;
const ACCOUNT: u32 = 0;

/// The other process either holds the lock or has already refreshed.
#[derive(Clone, Copy, Debug)]
pub enum Story {
    Busy,
    Slow,
    Newer,
    NoRecord,
    Subject,
    Cancelled,
    Removed,
    Aborted,
}
pub(crate) struct Owner {
    story: Story,
}
fn record(generation: u64, seconds: u64) -> oauth::SavedToken {
    oauth::SavedToken {
        key: ACCOUNT,
        generation,
        access_token: bytes::copy_of(b"shared"),
        refresh_token: Some(bytes::copy_of(b"seed")),
        metadata: None,
        expires_at: Wall::from_nanos(Duration::from_secs(seconds).as_nanos()),
        issued_client_id: None,
        granted_scopes: None,
        subject: None,
        id_token: None,
        earliest_refresh_at: None,
    }
}
fn client(story: Story) -> world::Client {
    let mut client = world::Client::new(world::Story::Rotate, Transport::Plaintext);
    client.shared = Some(Owner { story });
    client
}
pub(crate) fn start(client: &mut world::Client, env: &Env<accounts::Limits>) {
    let mut configured = List::with_capacity(1);
    assert!(
        configured
            .push(accounts::Account::SignIn {
                shared: true,
                registration: world::registration(Transport::Plaintext),
                endpoint: accounts::Endpoint {
                    address: client.address.expect("issuer"),
                    transport: accounts::Transport::Plaintext
                },
                keeper: accounts::Keeper::Owner { kept: Some(record(0, 5)) },
            })
            .is_ok()
    );
    client.component = Some(
        accounts::Component::new(
            configured,
            &env.limits,
            skein_lib::CryptoRng::new(&skein_world::crypto_seed(17), 0).expect("seed"),
        )
        .expect("shared account"),
    );
    client.ask(env, accounts::Request::Grant { account: ACCOUNT });
}
pub(crate) fn observe(client: &mut world::Client, env: &Env<accounts::Limits>) {
    let Some(event) = client.above.pop() else { return };
    match event {
        accounts::Event::Lock { account } => {
            client.facts.push(Fact::Lock);
            lock_answer(client, env, account);
        }
        accounts::Event::Granted { generation, .. } => {
            client.contracts.read(generation);
            client.contracts.granted(generation);
            client.facts.push(Fact::Granted(generation));
            client.close(env);
        }
        accounts::Event::Failed { failure, .. } => {
            client.contracts.failed();
            client.facts.push(Fact::Failed(failure));
            let story = client.shared.as_ref().expect("story").story;
            match story {
                Story::Cancelled | Story::Removed | Story::Aborted => held(client, env, ACCOUNT, None),
                Story::Busy | Story::Slow | Story::Newer | Story::NoRecord | Story::Subject => {}
            }
            if failure == accounts::Failure::AnotherSubject {
                client.contracts.requesting();
                client.ask(env, accounts::Request::Grant { account: ACCOUNT });
            } else {
                client.close(env);
            }
        }
        accounts::Event::Unlock { account } => {
            client.contracts.unlocked(account);
            client.facts.push(Fact::Unlock);
        }
        accounts::Event::Removed { account } => client.facts.push(Fact::Removed(account)),
        accounts::Event::Closed => {
            client.contracts.closed();
            client.facts.push(Fact::Closed);
            client.closed = true;
        }
        accounts::Event::Keep { .. }
        | accounts::Event::SignedIn { .. }
        | accounts::Event::SignedOut { .. }
        | accounts::Event::Visit { .. }
        | accounts::Event::Expiring { .. }
        | accounts::Event::Refused { .. } => panic!("shared refresh must settle from its lock answer"),
    }
}
fn held(client: &mut world::Client, env: &Env<accounts::Limits>, account: u32, record: Option<oauth::SavedToken>) {
    client.contracts.locked(account);
    client.ask(env, accounts::Request::Locked { account, answer: accounts::Locked::Held { record } });
}
fn lock_answer(client: &mut world::Client, env: &Env<accounts::Limits>, account: u32) {
    match client.shared.as_ref().expect("story").story {
        Story::Busy => client.ask(env, accounts::Request::Locked { account, answer: accounts::Locked::Busy }),
        Story::Slow => {}
        Story::Newer => held(client, env, account, Some(record(1, 30))),
        Story::NoRecord => held(client, env, account, None),
        Story::Subject => {
            let mut read = record(1, 30);
            read.subject = Some(bytes::copy_of(b"other"));
            held(client, env, account, Some(read));
        }
        Story::Cancelled => client.ask(env, accounts::Request::Cancel { account }),
        Story::Removed => client.ask(env, accounts::Request::Remove { account }),
        Story::Aborted => client.ask(env, accounts::Request::Abort),
    }
}
pub(crate) fn before_socket(client: &world::Client, request: &skein_io::Request) {
    if client.shared.is_some() {
        assert!(!matches!(request, skein_io::Request::Connect { .. }), "no exchange opens for the shared read");
    }
}
/// Runs the lock owner beside the independently scripted issuer.
#[must_use]
pub fn run(seed: u64, story: Story, memory: Memory) -> skein_world::Outcome<world::Process, private::FileSystem> {
    let mut config = skein_sim::Config::calm();
    config.wall = Wall::EPOCH;
    let mut world =
        World::new(seed, config, Judge::new(), memory).with_machine(private::FileSystem::new(private::Story::Load));
    world.spawn(|| world::issuer(Transport::Plaintext, world::Story::Rotate, ([127, 0, 0, 1], 31000).into()));
    world.spawn(|| world::Process::Client(Box::new(client(story))));
    world.run()
}
/// Runs the same scripted lock on seeded protocol streams.
#[must_use]
pub fn protocol(seed: u64, story: Story) -> (Vec<Fact>, Vec<String>) {
    let (client, trace) = world::protocol_client(seed, client(story));
    (client.facts, trace)
}
/// Reads content-free facts from the simulated owner.
#[must_use]
pub fn facts(processes: &[world::Process]) -> &[Fact] {
    processes
        .iter()
        .find_map(|process| match process {
            world::Process::Client(client) => Some(client.facts.as_slice()),
            world::Process::Issuer(_) => None,
        })
        .expect("client")
}
