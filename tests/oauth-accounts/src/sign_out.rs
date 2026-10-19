//! Sign-out over the shared simulated and protocol worlds (oauth.md, section 6.8).
use crate::{private, world};
use skein_fake_oauth as fake;
use skein_fake_peers::Transport;
use skein_io::{file, kernel};
use skein_lib::{Duration, Env, List, Token, Wall, bytes};
use skein_oauth as oauth;
use skein_oauth_accounts as accounts;
use skein_world::{Memory, World};
use std::net::Ipv4Addr;

/// The issuer's revocation answer, or an owner shutdown during it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Story {
    Confirmed,
    Unconfirmed,
    NoRevocation,
    Close,
    Abort,
}
pub(crate) struct Owner {
    pub(crate) story: Story,
    pub(crate) facts: Vec<world::Fact>,
    private: bool,
    record: Option<oauth::SavedToken>,
    closed: bool,
}
const ROOT: Token = Token::new(100);
const REVOKE: &[u8] = b"http://127.0.0.1:31000/revoke";
pub(crate) fn status(story: Story) -> u16 {
    if story == Story::Unconfirmed { 503 } else { 200 }
}
pub(crate) fn issuer_terms(_story: Story) -> fake::Terms {
    fake::Terms {
        issued: Box::new([]),
        omit_issued: false,
        revocation_endpoint: Some(bytes::copy_of(REVOKE)),
        varying_port: false,
        openid: None,
    }
}
fn record() -> oauth::SavedToken {
    oauth::SavedToken {
        key: 0,
        generation: 0,
        access_token: bytes::copy_of(b"old"),
        refresh_token: Some(bytes::copy_of(b"seed")),
        metadata: None,
        expires_at: Wall::from_nanos(Duration::from_secs(30).as_nanos()),
        issued_client_id: None,
        granted_scopes: None,
        subject: None,
        id_token: None,
        earliest_refresh_at: None,
    }
}
fn client(story: Story, private: bool) -> world::Client {
    let mut client = world::Client::new(world::Story::Rotate, Transport::Plaintext);
    client.sign_out = Some(Owner { story, facts: Vec::new(), private, record: Some(record()), closed: false });
    client
}
pub(crate) fn start(client: &mut world::Client, env: &Env<accounts::Limits>) {
    let owner = client.sign_out.as_ref().expect("sign-out story");
    let mut registration = world::registration(Transport::Plaintext);
    registration.terms = Some(oauth::Terms {
        parameters: Box::new([]),
        openid: None,
        issued_client_id: false,
        varying_port: false,
        resource: None,
        required_scopes: None,
        revocation_endpoint: (owner.story != Story::NoRevocation).then(|| bytes::copy_of(REVOKE)),
        sign_in_again: Box::new([]),
    });
    let keeper = if owner.private {
        accounts::Keeper::Private {
            root: client.file_driver.as_ref().map_or(ROOT, skein_world::files::Driver::root),
            directory: bytes::copy_of(b"secret"),
            file: bytes::copy_of(b"record"),
        }
    } else {
        accounts::Keeper::Owner { kept: owner.record.clone() }
    };
    let mut configured = List::with_capacity(1);
    assert!(
        configured
            .push(accounts::Account::SignIn {
                registration,
                endpoint: accounts::Endpoint {
                    address: client.address.expect("issuer address"),
                    transport: accounts::Transport::Plaintext
                },
                keeper
            })
            .is_ok()
    );
    client.component = Some(
        accounts::Component::new(
            configured,
            &env.limits,
            skein_lib::CryptoRng::new(&skein_world::crypto_seed(17), 0).expect("seed"),
        )
        .expect("sign-out account"),
    );
    client.ask(env, accounts::Request::SignOut { account: 0 });
    match client.sign_out.as_ref().expect("story").story {
        Story::Close => client.close(env),
        Story::Abort => {
            client.close(env);
            client.ask(env, accounts::Request::Abort);
        }
        Story::Confirmed | Story::Unconfirmed | Story::NoRevocation => {}
    }
}
pub(crate) fn observe(client: &mut world::Client, env: &Env<accounts::Limits>) {
    let Some(event) = client.above.pop() else { return };
    let owner = client.sign_out.as_mut().expect("sign-out story");
    assert!(!owner.closed, "Closed once and last");
    match event {
        accounts::Event::SignedOut { revoked, .. } => {
            owner.record = None;
            owner.facts.push(world::Fact::SignedOut(revoked));
            client.close(env);
        }
        accounts::Event::Failed { failure, .. } => {
            owner.facts.push(world::Fact::Failed(failure));
            client.close(env);
        }
        accounts::Event::Closed => {
            owner.facts.push(world::Fact::Closed);
            owner.closed = true;
            client.closed = true;
            if let Some(driver) = &mut client.file_driver {
                driver.close_root(env.now);
            }
        }
        accounts::Event::Removed { .. }
        | accounts::Event::Visit { .. }
        | accounts::Event::SignedIn { .. }
        | accounts::Event::Granted { .. }
        | accounts::Event::Expiring { .. }
        | accounts::Event::Keep { .. }
        | accounts::Event::Refused { .. } => panic!("unexpected sign-out outcome"),
    }
}
pub(crate) fn protocol_files(client: &mut world::Client, env: &Env<accounts::Limits>) {
    if client.sign_out.is_none() {
        return;
    }
    if let Some(lower) = client.files.pop() {
        let event = match lower {
            accounts::FileLower::Request { request, .. } => match request {
                file::Request::OpenPrivate { owner, .. } => file::Event::Opened { owner, file: ROOT, len: 0 },
                file::Request::Load { owner, .. } => file::Event::Loaded {
                    owner,
                    bytes: oauth::encode_record(&record(), &env.limits.client.document).expect("kept record"),
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
                | file::Request::ListPage { .. } => panic!("sign-out loads and closes only"),
            },
            accounts::FileLower::Remove { owner, .. } => file::Event::Removed { owner },
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
/// Runs sign-out against an issuer process and the private-file machine.
#[must_use]
pub fn run(
    seed: u64,
    story: Story,
    private: bool,
    memory: Memory,
) -> skein_world::Outcome<world::Process, private::FileSystem> {
    let mut files = private::FileSystem::new(private::Story::Load);
    let root = files.startup();
    let mut config = skein_sim::Config::calm();
    config.wall = Wall::EPOCH;
    let mut world = World::new(seed, config, crate::accounts::Judge::new(), memory).with_machine(files);
    world.spawn(|| {
        world::issuer_with_terms(
            Transport::Plaintext,
            world::Story::Rotate,
            (Ipv4Addr::LOCALHOST, 31000).into(),
            Some(issuer_terms(story)),
            status(story),
        )
    });
    world.spawn_root(root, |fd: kernel::Fd| {
        let mut client = client(story, private);
        client.file_driver = Some(skein_world::files::Driver::new(
            fd,
            4,
            client.limits.client.document.record_bytes,
            client.limits.file_stall,
            1234,
        ));
        world::Process::Client(Box::new(client))
    });
    let mut outcome = world.run();
    if story != Story::Abort && private {
        assert!(outcome.machine.record_absent(), "signed-out private record removed");
    }
    outcome
}
/// Runs the same owner over seeded in-memory HTTP streams.
#[must_use]
pub fn protocol(seed: u64, story: Story) -> (Vec<world::Fact>, Vec<String>) {
    let (client, trace) = world::protocol_client(seed, client(story, false));
    (client.sign_out.expect("story").facts, trace)
}
/// Reads the content-free owner facts from a simulated outcome.
#[must_use]
pub fn facts(processes: &[world::Process]) -> &[world::Fact] {
    processes
        .iter()
        .find_map(|process| match process {
            world::Process::Client(client) => Some(client.sign_out.as_ref().expect("story").facts.as_slice()),
            world::Process::Issuer(_) => None,
        })
        .expect("client")
}
