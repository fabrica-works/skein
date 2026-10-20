//! Configuration and credentials shared by the world's stories.
use skein_fake_forge_protocol::{Config, Faults};
use skein_forge_setup::{given::Given, limits::Limits};
use skein_lib::Duration;
use skein_lib::{List, Secret};
pub const ADMIN: &[u8] = b"admin-token-for-the-setup-world";
pub const API: &[u8] = b"api-token-issued-to-the-bot";
pub const GIT: &[u8] = b"git-token-issued-to-the-bot";
pub const TAKEN: &[u8] = b"handover-token-0123456789abcdef";
#[must_use]
pub fn given(owned: bool) -> Given {
    Given {
        forge: 1,
        bot_login: b"kit".as_slice().into(),
        bot_email: b"kit@example.invalid".as_slice().into(),
        admin_name: b"kit-admin".as_slice().into(),
        api_name: b"kit-api".as_slice().into(),
        git_name: b"kit-git".as_slice().into(),
        orgs: if owned { Box::new([b"acme".as_slice().into(), b"work".as_slice().into()]) } else { Box::new([]) },
        org_owner: owned.then(|| b"ada".as_slice().into()),
        adopted: Box::new([]),
        listener: None,
        word: b"kit".as_slice().into(),
        limits: Limits {
            orgs: 8,
            repositories: 8,
            reachable: 4,
            deadline: Duration::from_secs(120),
            backoff: Duration::from_millis(100),
            backoff_max: Duration::from_secs(5),
        },
    }
}
#[must_use]
pub fn config() -> Config {
    config_with_cap(2)
}
#[must_use]
pub fn config_with_cap(cap: u32) -> Config {
    Config {
        hooks: None,
        base: b"http://127.0.0.1:3000".as_slice().into(),
        tokens: List::with_capacity(0),
        users: List::with_capacity(0),
        repositories: List::with_capacity(0),
        max_response_items: cap,
        default_paging_num: 30,
        faults: Faults { reset: 0, drop: 0, server_error: 0 },
    }
}
#[must_use]
pub fn secret(bytes: &[u8]) -> Secret {
    Secret::new(bytes.into())
}
/// A bot with both valid token files; changing one file isolates its repair.
pub fn bot_tokens(world: &mut crate::World) -> u64 {
    use skein_forge_setup::item::Item;
    let id = world.store_mut().account(b"kit", false, b"kit:bot");
    world.store_mut().token(id, b"kit-api", &[b"read:user", b"write:issue", b"write:repository"], secret(API));
    world.store_mut().token(id, b"kit-git", &[b"write:repository", b"read:user"], secret(GIT));
    world.put(Item::ApiToken, secret(API));
    world.put(Item::GitToken, secret(GIT));
    id
}
/// A settled forge for perturbing one behavior on its next run.
#[must_use]
pub fn converged(seed: u64) -> crate::World {
    let mut world = crate::World::owned(seed);
    assert_eq!(world.reconcile(), crate::Ended::Done);
    world
}
/// The HTTP mutations since a given request, including any unintended route.
#[must_use]
pub fn write_routes(world: &crate::World, since: usize) -> Vec<skein_fake_forge_protocol::Route> {
    use skein_forge::forgejo::{client, request::Method};
    let limits = skein_forge_machine_world::fixtures::fake_limits();
    let mut routes = Vec::new();
    for request in &world.requests()[since..] {
        let encoded = client::encode(request, &limits.documents).expect("world request");
        if encoded.method != Method::Get {
            routes.push(skein_fake_forge_protocol::route(request));
        }
    }
    routes
}
#[must_use]
pub fn file_writes(world: &crate::World) -> usize {
    world.trace().iter().filter(|line| line.starts_with("Write {")).count()
}
pub fn assert_bot(world: &crate::World) {
    use skein_forge_setup::item::Item;
    let bot = world.store().find(b"kit").expect("bot made");
    assert_eq!((bot.id, bot.admin, bot.full_name.as_ref()), (3, false, b"kit:bot".as_slice()));
    assert_eq!(world.file(Item::User).expect("id file").expose(), &3u64.to_le_bytes());
    for file in [Item::ApiToken, Item::GitToken] {
        assert_eq!(world.store().token_user(world.file(file).expect("token file").expose()), Some(3));
    }
    assert_eq!(
        world.store().scopes(3, b"kit-api"),
        Some(
            [b"read:user".as_slice().into(), b"write:issue".as_slice().into(), b"write:repository".as_slice().into()]
                .as_slice()
        )
    );
    assert_eq!(
        world.store().scopes(3, b"kit-git"),
        Some([b"write:repository".as_slice().into(), b"read:user".as_slice().into()].as_slice())
    );
}
pub fn assert_org(world: &crate::World, org: &[u8]) {
    assert_eq!(world.store().organization(org).expect("org").owner.as_ref(), b"ada");
}
/// Seed each part independently; stale credentials answer as another user or nobody.
#[must_use]
pub fn random(seed: u64) -> crate::World {
    use skein_forge_setup::item::Item;
    use skein_lib::Rng;
    let mut rng = Rng::new(seed);
    let permissions = [rng.next_u64() % 5, rng.next_u64() % 5, rng.next_u64() % 5];
    let paths = [
        if permissions[0] == 4 { b"kit/one".as_slice() } else { b"alice/one" },
        if permissions[1] == 4 { b"kit/two".as_slice() } else { b"alice/two" },
        if permissions[2] == 4 { b"kit/three".as_slice() } else { b"alice/three" },
    ];
    let mut world = repositories(seed, true, &paths);
    let bot = (rng.next_u64() & 1 == 1).then(|| world.store_mut().account(b"kit", false, b"kit:bot"));
    for (file, name, scopes, value) in [
        (
            Item::ApiToken,
            b"kit-api".as_slice(),
            [b"read:user".as_slice(), b"write:issue", b"write:repository"].as_slice(),
            API,
        ),
        (Item::GitToken, b"kit-git".as_slice(), [b"write:repository".as_slice(), b"read:user"].as_slice(), GIT),
    ] {
        seed_token(&mut world, &mut rng, bot, file, name, scopes, value);
    }
    if rng.next_u64() & 1 == 1 {
        world.put(Item::User, secret(&if rng.next_u64() & 1 == 1 { 3u64 } else { 1u64 }.to_le_bytes()));
    }
    for org in [b"acme".as_slice(), b"work"] {
        seed_org(&mut world, &mut rng, org);
    }
    let mut adopted = Vec::new();
    for id in 1..=4 {
        if rng.next_u64() & 1 == 1 {
            adopted.push(id);
        }
        if let Some(permission) = permissions.get(usize::try_from(id - 1).expect("small id")) {
            use skein_forge::forgejo::request::Grant;
            match permission {
                0 | 4 => (),
                1 => world.store_mut().grant(u32::try_from(id).expect("small id"), b"kit", Grant::Read).expect("grant"),
                2 => {
                    world.store_mut().grant(u32::try_from(id).expect("small id"), b"kit", Grant::Write).expect("grant");
                }
                3 => {
                    world.store_mut().grant(u32::try_from(id).expect("small id"), b"kit", Grant::Admin).expect("grant");
                }
                _ => unreachable!(),
            }
        }
    }
    world.adopted(&adopted);
    world
}
fn seed_token(
    world: &mut crate::World,
    rng: &mut skein_lib::Rng,
    bot: Option<u64>,
    file: skein_forge_setup::item::Item,
    name: &[u8],
    scopes: &[&[u8]],
    value: &[u8],
) {
    let mode = rng.next_u64();
    if mode & 1 == 1 {
        let user = if rng.next_u64() & 1 == 1 { bot.unwrap_or(1) } else { 1 };
        world.store_mut().token(user, name, scopes, secret(value));
        world.put(file, secret(value));
    } else if mode & 2 == 2 {
        world.put(file, secret(b"unknown-file-token"));
    }
}
fn seed_org(world: &mut crate::World, rng: &mut skein_lib::Rng, org: &[u8]) {
    if rng.next_u64() & 1 == 1 {
        world.store_mut().org(org, b"ada");
    }
}

#[must_use]
pub fn host_limits() -> skein_forge_setup::host::Limits {
    skein_forge_setup::host::Limits {
        forges: 2,
        runs: 2,
        token_bytes: 64,
        head_bytes: 4096,
        headers: 32,
        body_bytes_in: 65536,
        depth: 16,
        chunk: 1024,
        string_bytes: 512,
        body_bytes: 16384,
        inner: 32,
        target_bytes: 4096,
        io: skein_forge_machine_world::fixtures::shared().io,
    }
}
#[must_use]
pub fn host_settings() -> skein_forge_setup::host::Settings {
    use skein_forge_setup::host::{Forge, Kind, Settings, Transport};
    let mut forges = List::with_capacity(2);
    forges
        .push(Forge {
            kind: Kind::Forgejo,
            address: skein_io::Addr::Ip("127.0.0.1:3000".parse().expect("loopback")),
            transport: Transport::Plaintext { name: None },
            given: given(false),
        })
        .expect("one forge");
    Settings { forges, user_agent: b"kit/0.0.0".as_slice().into() }
}
#[must_use]
pub fn crypto() -> skein_lib::CryptoRng {
    skein_lib::CryptoRng::new(&secret(&[42; 32]), 0).expect("32-byte seed")
}

/// A person's two tokens for a connected Forgejo, with no user kept yet.
pub fn connected_tokens(world: &mut crate::World) -> u64 {
    use skein_forge_setup::item::Item;
    let id = world.store_mut().account(b"alice", false, b"A person");
    world.store_mut().token(id, b"connected-api", &[b"read:user", b"write:issue", b"write:repository"], secret(API));
    world.store_mut().token(id, b"connected-git", &[b"write:repository", b"read:user"], secret(GIT));
    world.put(Item::ApiToken, secret(API));
    world.put(Item::GitToken, secret(GIT));
    id
}

/// Five older admin tokens, including the default, precede the handover token.
pub fn admin_handover(world: &mut crate::World) -> u64 {
    let id = world.store().find(b"kit-admin").expect("admin fixture").id;
    for (name, value) in [
        (b"old-1".as_slice(), b"older-admin-token-1".as_slice()),
        (b"old-2", b"older-admin-token-2"),
        (b"old-3", b"older-admin-token-3"),
        (b"old-4", b"older-admin-token-4"),
    ] {
        world.store_mut().token(id, name, &[b"all"], secret(value));
    }
    world.store_mut().token(id, b"handover", &[b"all"], secret(TAKEN));
    id
}

/// Ten repositories in reverse id order, of a person, the bot and another person.
pub fn picker_repositories(config: &mut Config) {
    config.repositories = List::with_capacity(10);
    for id in (1..=10).rev() {
        let owner = match id % 3 {
            1 => "ada",
            2 => "kit",
            0 => "grace",
            _ => unreachable!("remainder"),
        };
        config
            .repositories
            .push(skein_fake_forge_protocol::Repository {
                id,
                full: format!("{owner}/repo-{id}").into_bytes().into_boxed_slice(),
                protected: None,
            })
            .expect("ten repositories");
    }
}
/// The configured owners and scenario privacy share the fake's setup store.
pub fn picker_owners(store: &mut skein_fake_forge_protocol::setup::Store) {
    store.account(b"ada", false, b"Ada");
    store.account(b"kit", false, b"kit:bot");
    store.account(b"grace", false, b"Grace");
    for id in [2, 3, 6, 10] {
        store.private(id).expect("bounded privacy metadata");
    }
}

/// Repository identities are scenario configuration; grants live in the setup store.
#[must_use]
pub fn repositories(seed: u64, owned: bool, paths: &[&[u8]]) -> crate::World {
    let mut config = config();
    config.repositories = List::with_capacity(u32::try_from(paths.len()).expect("scenario count"));
    for (index, path) in paths.iter().enumerate() {
        config
            .repositories
            .push(skein_fake_forge_protocol::Repository {
                id: u32::try_from(index + 1).expect("scenario id"),
                full: (*path).into(),
                protected: None,
            })
            .expect("scenario capacity");
    }
    crate::World::with_config(seed, owned, config)
}
