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
    let id = seed_bot_tokens(world.store_mut());
    world.put(Item::ApiToken, secret(API));
    world.put(Item::GitToken, secret(GIT));
    id
}
/// Seeds the bot in either the plan world or the host's HTTP world.
pub fn seed_bot_tokens(store: &mut skein_fake_forge_protocol::setup::Store) -> u64 {
    let id = store.account(b"kit", false, b"kit:bot");
    store.token(id, b"kit-api", &[b"read:user", b"write:issue", b"write:repository"], secret(API));
    store.token(id, b"kit-git", &[b"write:repository", b"read:user"], secret(GIT));
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
    let cap = 2 + u32::try_from(rng.next_u64() & 1).expect("one bit");
    let mut world = repositories_with_cap(seed, true, &paths, cap);
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
        states: 8,
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
        tls: skein_forge_machine_world::fixtures::limits().tls,
        io: skein_forge_machine_world::fixtures::shared().io,
    }
}
#[must_use]
pub fn host_settings() -> skein_forge_setup::host::Settings {
    use skein_forge_setup::host::{Forge, Kind, Settings, Transport};
    let mut forges = List::with_capacity(2);
    forges
        .push(Forge {
            kind: Kind::Forgejo(given(false)),
            address: skein_io::Addr::Ip("127.0.0.1:3000".parse().expect("loopback")),
            transport: Transport::Plaintext { name: None },
        })
        .expect("one forge");
    Settings { forges, user_agent: b"kit/0.0.0".as_slice().into() }
}
#[must_use]
pub fn github_given() -> skein_forge_setup::github::given::Given {
    use skein_forge_setup::github::{given::Given, limits::Limits};
    Given {
        forge: 4,
        word: b"kit".as_slice().into(),
        web: b"https://github.test".as_slice().into(),
        url: b"https://kit.test/".as_slice().into(),
        redirect_url: b"https://kit.test/connect/redirect".as_slice().into(),
        setup_url: b"https://kit.test/connect/setup".as_slice().into(),
        limits: Limits {
            key_bytes: 1196,
            name_bytes: 34,
            page_bytes: 4096,
            installations: 8,
            reachable: 2,
            pages: 4,
            state_life: Duration::from_secs(600),
            deadline: Duration::from_secs(120),
            backoff: Duration::from_millis(100),
            backoff_max: Duration::from_secs(2),
        },
    }
}
#[must_use]
pub fn github_forge() -> skein_forge_setup::host::Forge {
    use skein_forge_setup::host::{Forge, Kind, Transport};
    Forge {
        kind: Kind::GitHub(github_given()),
        address: skein_io::Addr::Ip("127.0.0.1:3001".parse().expect("loopback")),
        transport: Transport::Plaintext { name: None },
    }
}
#[must_use]
pub fn github_settings() -> skein_forge_setup::host::Settings {
    let mut settings = host_settings();
    settings.forges.push(github_forge()).expect("two forges");
    settings
}
#[must_use]
pub fn crypto() -> skein_lib::CryptoRng {
    skein_lib::CryptoRng::new(&secret(&[42; 32]), 0).expect("32-byte seed")
}

/// A person's two tokens for a connected Forgejo, with no user kept yet.
pub fn connected_tokens(world: &mut crate::World) -> u64 {
    use skein_forge_setup::item::Item;
    let id = seed_connected_tokens(world.store_mut());
    world.put(Item::ApiToken, secret(API));
    world.put(Item::GitToken, secret(GIT));
    id
}

/// Seeds the person's tokens independently of the owner's item store.
pub fn seed_connected_tokens(store: &mut skein_fake_forge_protocol::setup::Store) -> u64 {
    let id = store.account(b"alice", false, b"A person");
    store.token(id, b"connected-api", &[b"read:user", b"write:issue", b"write:repository"], secret(API));
    store.token(id, b"connected-git", &[b"write:repository", b"read:user"], secret(GIT));
    id
}

/// Five older admin tokens, including the default, precede the handover token.
pub fn admin_handover(world: &mut crate::World) -> u64 {
    seed_admin_handover(world.store_mut(), 4)
}
/// The default admin token and the chosen older tokens precede the handed one.
pub fn seed_admin_handover(store: &mut skein_fake_forge_protocol::setup::Store, older: usize) -> u64 {
    let id = store.find(b"kit-admin").expect("admin fixture").id;
    for (name, value) in [
        (b"old-1".as_slice(), b"older-admin-token-1".as_slice()),
        (b"old-2", b"older-admin-token-2"),
        (b"old-3", b"older-admin-token-3"),
        (b"old-4", b"older-admin-token-4"),
    ]
    .into_iter()
    .take(older)
    {
        store.token(id, name, &[b"all"], secret(value));
    }
    store.token(id, b"handover", &[b"all"], secret(TAKEN));
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
    crate::World::with_config(seed, owned, repository_config(paths))
}
fn repositories_with_cap(seed: u64, owned: bool, paths: &[&[u8]], cap: u32) -> crate::World {
    crate::World::with_config(seed, owned, repository_config_with_cap(paths, cap))
}
/// Repository identities shared by the plan and HTTP host worlds.
#[must_use]
pub fn repository_config(paths: &[&[u8]]) -> Config {
    repository_config_with_cap(paths, 2)
}
fn repository_config_with_cap(paths: &[&[u8]], cap: u32) -> Config {
    let mut config = config_with_cap(cap);
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
    config
}

/// The connected picker has four raw rows per page, including read-only grants.
#[must_use]
pub fn connected_picker(seed: u64) -> crate::World {
    use skein_forge::forgejo::{
        document::{Access, TeamSpec, Units},
        request::Grant,
    };
    use skein_forge_setup::item::Item;
    let mut config = config_with_cap(4);
    config.repositories = List::with_capacity(10);
    for (id, owner) in [
        (1, "kit"),
        (2, "ada"),
        (3, "ada"),
        (4, "ada"),
        (5, "ada"),
        (6, "ada"),
        (7, "ada"),
        (8, "grace"),
        (9, "ada"),
        (10, "kit"),
    ] {
        config
            .repositories
            .push(skein_fake_forge_protocol::Repository {
                id,
                full: format!("{owner}/repo-{id}").into_bytes().into_boxed_slice(),
                protected: None,
            })
            .expect("ten repositories");
    }
    let mut world = crate::World::with_config(seed, false, config);
    world.store_mut().account(b"ada", false, b"Ada");
    world.store_mut().account(b"kit", false, b"kit:bot");
    for id in [1, 2, 3, 6, 9, 10] {
        world.store_mut().private(id).expect("scenario privacy");
    }
    world.store_mut().org(b"grace", b"ada");
    assert_eq!(world.reconcile(), crate::Ended::Done);
    for (id, permission) in
        [(2, Grant::Write), (3, Grant::Read), (4, Grant::Admin), (5, Grant::Write), (6, Grant::Read), (7, Grant::Read)]
    {
        world.store_mut().grant(id, b"kit", permission).expect("scenario grant");
    }
    let team = world.store_mut().add_team(
        b"grace",
        TeamSpec {
            name: b"writers".as_slice().into(),
            permission: Access::Write,
            includes_all_repositories: true,
            units: Units { code: Access::Write, issues: Access::Write, pulls: Access::Write, actions: Access::Write },
        },
    );
    let bot = world.store().find(b"kit").expect("bot fixture").id;
    world.store_mut().member(team, bot);
    world.forget(Item::AdminToken);
    world
}

/// The accepted installation ids, in ascending order.
#[must_use]
pub fn accepted(ids: &[u64]) -> Secret {
    skein_forge_setup::item::accepted_item(ids)
}
/// The App's id as its retained eight-byte item.
#[must_use]
pub fn app_id(id: u64) -> Secret {
    secret(&id.to_le_bytes())
}

/// Shared HTTP/browser stories; API requests are observed only through redacted records.
pub mod github_stories {
    use super::{Secret, github_given};
    use crate::{Connect, Heard, HostWorld};
    use skein_forge_setup::{
        github::manifest::Target,
        host::{Need, Plan},
        item::{self, Item},
        reconcile::Outcome,
    };
    use skein_lib::{Time, Token};
    use std::{collections::BTreeMap, fmt::Write};
    pub const FORGE: u16 = 4;
    pub const HOOK: &[u8] = b"https://test-app.invalid/";
    // Parse the generated elements' quoted attributes; escaped JSON stays one value.
    #[must_use]
    pub fn attributes(html: &str, element: &str) -> BTreeMap<String, String> {
        let mut tags = html.split('<').filter_map(|piece| piece.split_once('>').map(|(tag, _)| tag));
        let tag =
            tags.find(|tag| tag.strip_prefix(element).is_some_and(|rest| rest.starts_with(' '))).expect("element");
        let mut rest = &tag[element.len()..];
        let mut attributes = BTreeMap::new();
        while !rest.trim_start().is_empty() {
            rest = rest.trim_start();
            let (name, value) = rest.split_once('=').expect("attribute assignment");
            let quote = value.as_bytes()[0];
            assert!(quote == b'"' || quote == b'\'');
            let end = value.as_bytes()[1..].iter().position(|byte| *byte == quote).expect("closing quote") + 1;
            assert!(attributes.insert(name.to_owned(), entities(&value[1..end])).is_none());
            rest = &value[end + 1..];
        }
        attributes
    }
    #[must_use]
    pub fn entities(input: &str) -> String {
        let mut output = String::new();
        let mut rest = input;
        while let Some(at) = rest.find('&') {
            output.push_str(&rest[..at]);
            let (entity, tail) = rest[at + 1..].split_once(';').expect("entity terminator");
            output.push(match entity {
                "amp" => '&',
                "quot" => '"',
                "#39" => '\'',
                "lt" => '<',
                "gt" => '>',
                _ => panic!("supported HTML entity"),
            });
            rest = tail;
        }
        output.push_str(rest);
        output
    }
    #[must_use]
    pub fn encoded(input: &[u8]) -> String {
        let mut output = String::new();
        for byte in input {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(byte) {
                output.push(char::from(*byte));
            } else {
                write!(output, "%{byte:02X}").expect("String accepts text");
            }
        }
        output
    }
    #[must_use]
    pub fn unencoded(input: &str) -> Vec<u8> {
        let mut output = Vec::new();
        let mut bytes = input.bytes();
        while let Some(byte) = bytes.next() {
            output.push(match byte {
                b'+' => b' ',
                b'%' => {
                    let high = char::from(bytes.next().expect("high nibble")).to_digit(16).expect("hex");
                    let low = char::from(bytes.next().expect("low nibble")).to_digit(16).expect("hex");
                    u8::try_from(high * 16 + low).expect("byte")
                }
                byte => byte,
            });
        }
        output
    }
    #[must_use]
    pub fn query(url: &str) -> BTreeMap<String, Vec<u8>> {
        url.split_once('?')
            .expect("query")
            .1
            .split('&')
            .map(|field| {
                let (name, value) = field.split_once('=').expect("query field");
                (name.to_owned(), unencoded(value))
            })
            .collect()
    }
    #[must_use]
    pub fn request(method: &str, url: &str, body: &[u8]) -> Vec<u8> {
        let (scheme, address) = url.split_once("://").expect("absolute browser URL");
        assert!(scheme == "https" || scheme == "http");
        let slash = address.find('/').expect("path");
        let mut bytes = format!(
        "{method} {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n",
        &address[slash..], &address[..slash], body.len()
    ).into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }
    #[must_use]
    pub fn response(bytes: &[u8], status: u16) -> (&str, &str) {
        let text = core::str::from_utf8(bytes).expect("browser response UTF-8");
        let (head, body) = text.split_once("\r\n\r\n").expect("response head");
        let got: u16 = head.split_whitespace().nth(1).expect("status").parse().expect("numeric status");
        assert_eq!(got, status);
        let length: usize = header(head, "Content-Length").expect("length").parse().expect("numeric length");
        assert_eq!(length, body.len());
        (head, body)
    }
    #[must_use]
    pub fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
        head.split("\r\n")
            .skip(1)
            .filter_map(|line| line.split_once(':'))
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.trim())
    }

    /// One manifest and conversion through the browser and host, on the same fake.
    #[must_use]
    pub fn connected(seed: u64) -> (HostWorld, Secret) {
        let mut world = HostWorld::github(seed, &[]);
        for item in [Item::AppId, Item::AppKey, Item::AppSlug, Item::User, Item::Accepted] {
            world.forget(item);
        }
        world.run_plan(1, Plan::Manifest { forge: FORGE, target: Target::Personal });
        world.settle();
        let Some(Heard::Done { outcome: Outcome::Manifest { state, page }, .. }) = world.heard().last() else {
            panic!("manifest")
        };
        let state = state.clone();
        let html = core::str::from_utf8(page).expect("page text");
        let form = attributes(html, "form");
        let field = attributes(html, "input");
        let body = format!("manifest={}", encoded(field["value"].as_bytes()));
        let reply = world.github_page(&request("POST", &form["action"], body.as_bytes()));
        let (_, html) = response(&reply, 200);
        let link = attributes(html, "a");
        let mut fields = query(&link["href"]);
        assert_eq!(fields.remove("state").expect("state"), state.as_ref());
        let code = Secret::new(fields.remove("code").expect("code").into_boxed_slice());
        world.run_plan(2, Plan::Convert { forge: FORGE, code: code.clone(), state });
        world.settle();
        assert!(matches!(world.heard().last(), Some(Heard::Done { outcome: Outcome::Nothing, .. })));
        (world, code)
    }
    #[must_use]
    pub fn configured(script: &[Connect]) -> HostWorld {
        HostWorld::github_scenario(7, script, skein_fake_forge_github::service::AppFacts::Complete, HOOK)
    }
    pub fn reconcile(world: &mut HostWorld, run: u64) {
        world.run_plan(run, Plan::Reconcile { forge: FORGE, adopted: Box::new([]) });
        world.settle();
    }
    #[must_use]
    pub fn install(world: &mut HostWorld, run: u64) -> (Box<[u8]>, Box<[u8]>) {
        world.run_plan(run, Plan::Install { forge: FORGE });
        world.settle();
        let Some(Heard::Done { outcome: Outcome::Install { state, url }, .. }) = world.heard().last() else {
            panic!("install")
        };
        (state.clone(), url.clone())
    }
    pub fn accept(world: &mut HostWorld, run: u64, state: &[u8], id: u64) {
        world.run_plan(
            run,
            Plan::Accept {
                forge: FORGE,
                state: state.into(),
                installation: core::num::NonZeroU64::new(id).expect("fixture id"),
            },
        );
        world.settle();
    }
    #[must_use]
    pub fn accepted(world: &HostWorld) -> Box<[u64]> {
        world.file(Item::Accepted).map_or_else(
            || Box::from([]),
            |value| item::accepted(value.expose(), github_given().limits.installations).expect("accepted item"),
        )
    }
    #[must_use]
    pub fn writes(world: &HostWorld, since: usize) -> Vec<Item> {
        world.needs()[since..]
            .iter()
            .filter_map(|need| match need {
                Need::Write { item, .. } => Some(*item),
                Need::Read(_) => None,
            })
            .collect()
    }
    #[must_use]
    pub fn mutations(world: &HostWorld, since: usize) -> Vec<(String, String)> {
        world.github_requests()[since..]
            .iter()
            .filter(|row| row.method != "GET")
            .map(|row| (row.method.clone(), row.target.clone()))
            .collect()
    }
    pub fn done(world: &HostWorld, run: u64) {
        assert_eq!(world.heard().iter().filter(|event| matches!(event, Heard::Done { run: token, .. } | Heard::Stop { run: token, .. } if *token == Token::new(run))).count(), 1);
        let Some(Heard::Done { run: token, outcome: Outcome::Lacking { lacking }, .. }) = world.heard().last() else {
            panic!("reconcile Done: {:?}", world.heard().last())
        };
        assert_eq!(*token, Token::new(run));
        assert!(lacking.is_empty());
    }
    pub fn closed(world: &mut HostWorld) {
        let diagnostics = format!("{:?} {:?} {:?}", world.heard(), world.needs(), world.github_requests());
        let key = skein_forge_machine_world::github::app_key();
        let pem = skein_fake_forge_github::keys::pem(key).expect("fixture PEM");
        for secret in [key, pem.as_ref(), b"ghs_", b"eyJ"] {
            assert!(!diagnostics.as_bytes().windows(secret.len()).any(|part| part == secret));
            assert!(!diagnostics.contains(&format!("{secret:?}")));
        }
        let mut terminals = std::collections::BTreeSet::new();
        for event in world.heard() {
            match event {
                Heard::Done { run, .. } | Heard::Stop { run, .. } => {
                    assert!(terminals.insert(run.raw()), "one terminal per run");
                }
                Heard::Closed { .. } => panic!("close follows run terminals"),
            }
        }
        world.close();
        world.settle();
        assert!(matches!(world.heard().last(), Some(Heard::Closed { .. })));
        let counts = (world.heard().len(), world.connects().len(), world.needs().len());
        world.finish();
        assert_eq!(
            (world.heard().len(), world.connects().len(), world.needs().len()),
            counts,
            "quiescent, with no expiry wake"
        );
    }
    pub fn expire(world: &mut HostWorld) {
        world.advance_to(Time::from_nanos(github_given().limits.state_life.as_nanos()));
    }
}
