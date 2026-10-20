//! Configuration and credentials shared by the world's stories.
use skein_fake_forge_protocol::{Config, Faults};
use skein_forge_setup::{given::Given, limits::Limits};
use skein_lib::Duration;
use skein_lib::{List, Secret};
pub const ADMIN: &[u8] = b"admin-token-for-the-setup-world";
pub const API: &[u8] = b"api-token-issued-to-the-bot";
pub const GIT: &[u8] = b"git-token-issued-to-the-bot";
#[must_use]
pub fn given(owned: bool) -> Given {
    Given {
        forge: 1,
        bot_login: b"kit".as_slice().into(),
        bot_email: b"kit@example.invalid".as_slice().into(),
        admin_name: b"kit-admin".as_slice().into(),
        api_name: b"kit-api".as_slice().into(),
        git_name: b"kit-git".as_slice().into(),
        orgs: Box::new([b"acme".as_slice().into(), b"work".as_slice().into()]),
        org_owner: owned.then(|| b"ada".as_slice().into()),
        listener: None,
        word: b"kit".as_slice().into(),
        limits: Limits {
            orgs: 8,
            deadline: Duration::from_secs(120),
            backoff: Duration::from_millis(100),
            backoff_max: Duration::from_secs(5),
        },
    }
}
#[must_use]
pub fn config() -> Config {
    Config {
        hooks: None,
        base: b"http://127.0.0.1:3000".as_slice().into(),
        tokens: List::with_capacity(0),
        users: List::with_capacity(0),
        repositories: List::with_capacity(0),
        // Three teams require a second page; all other fake limits are shipped.
        max_response_items: 2,
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
#[must_use]
pub fn team(name: &[u8]) -> skein_forge::forgejo::document::TeamSpec {
    use skein_forge::forgejo::document::{Access, TeamSpec, Units};
    TeamSpec {
        name: name.into(),
        permission: Access::Write,
        includes_all_repositories: true,
        units: Units { code: Access::Write, issues: Access::Write, pulls: Access::Write, actions: Access::Write },
    }
}
#[must_use]
pub fn listener() -> std::net::SocketAddr {
    "127.0.0.1:9898".parse().expect("listener")
}
pub const URL: &[u8] = b"http://127.0.0.1:9898/hooks/1";
pub const WEBHOOK: &[u8] = b"webhook-secret-present-before-reconcile";
/// A settled forge for perturbing one behavior on its next run.
#[must_use]
pub fn converged(seed: u64) -> crate::World {
    let mut world = crate::World::owned(seed);
    world.listening(listener());
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
/// Namespaced differently from the listener, so reconcile must leave it alone.
pub const OTHER_URL: &[u8] = b"http://elsewhere.invalid/hook";
pub const OTHER_SECRET: &[u8] = b"somebody-elses-hook-secret";
#[must_use]
pub fn stale_team() -> skein_forge::forgejo::document::TeamSpec {
    use skein_forge::forgejo::document::{Access, Units};
    let mut spec = team(b"kit");
    spec.permission = Access::Read;
    spec.includes_all_repositories = false;
    spec.units = Units { code: Access::Read, issues: Access::None, pulls: Access::None, actions: Access::None };
    spec
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
    use skein_forge::forgejo::delivery::SUBSCRIBED;
    use skein_forge_setup::item::Item;
    assert_eq!(world.store().organization(org).expect("org").owner.as_ref(), b"ada");
    let team = world.store().team(org, b"kit").expect("team");
    assert_eq!(team.spec, self::team(b"kit"));
    assert_eq!(team.members.as_slice(), &[3]);
    let hooks: Vec<_> = world.store().subscriptions(org).filter(|hook| hook.url.as_ref() == URL).collect();
    assert_eq!(hooks.len(), 1);
    assert_eq!(&hooks[0].secret, world.file(Item::WebhookSecret).expect("webhook file"));
    assert_eq!(hooks[0].events.iter().map(AsRef::as_ref).collect::<Vec<&[u8]>>(), SUBSCRIBED);
    assert!(hooks[0].active);
}
pub fn assert_other_hooks(world: &crate::World, expected: &[(Box<[u8]>, u64)]) {
    for (org, id) in expected {
        let hook = world.store().subscriptions(org).find(|hook| hook.id == *id).expect("other hook kept");
        assert_eq!((hook.url.as_ref(), hook.secret.expose(), hook.active), (OTHER_URL, OTHER_SECRET, false));
        assert!(hook.events.is_empty());
    }
}
pub type OtherHooks = Vec<(Box<[u8]>, u64)>;
/// Seed each part independently; stale credentials answer as another user or nobody.
#[must_use]
pub fn random(seed: u64) -> (crate::World, OtherHooks) {
    use skein_forge_setup::item::Item;
    use skein_lib::Rng;
    let mut rng = Rng::new(seed);
    let mut world = crate::World::owned(seed);
    world.listening(listener());
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
    if rng.next_u64() & 1 == 1 {
        world.put(Item::WebhookSecret, secret(WEBHOOK));
    }
    let mut other = Vec::new();
    for org in [b"acme".as_slice(), b"work"] {
        seed_org(&mut world, &mut rng, bot, org, &mut other);
    }
    (world, other)
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
fn seed_org(
    world: &mut crate::World,
    rng: &mut skein_lib::Rng,
    bot: Option<u64>,
    org: &[u8],
    other: &mut Vec<(Box<[u8]>, u64)>,
) {
    if rng.next_u64() & 1 == 0 {
        return;
    }
    world.store_mut().org(org, b"ada");
    if rng.next_u64() & 1 == 1 {
        let spec = if rng.next_u64() & 1 == 1 { team(b"kit") } else { stale_team() };
        let team = world.store_mut().add_team(org, spec);
        if let Some(bot) = bot
            && rng.next_u64() & 1 == 1
        {
            world.store_mut().member(team, bot);
        }
    }
    if rng.next_u64() & 1 == 1 {
        world.store_mut().hook(org, URL, secret(b"stale-hook-secret"));
    }
    if rng.next_u64() & 1 == 1 {
        world.store_mut().hook(org, URL, secret(b"stale-second-hook"));
    }
    if rng.next_u64() & 1 == 1 {
        let id = world.store_mut().hook(org, OTHER_URL, secret(OTHER_SECRET));
        other.push((org.into(), id));
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
