use core::num::NonZeroU64;
use skein_fake_forge_github::{keys, service::AppFacts};
use skein_forge_machine_world::github::app_key;
use skein_forge_setup::{
    github::Lacking,
    host::{Need, Plan},
    item::Item,
    reconcile::{Outcome, Stage, Why},
};
use skein_forge_setup_world::{
    Connect, Heard, HostWorld,
    fixtures::{
        self,
        github_stories::{FORGE, accepted, closed, install, mutations, writes},
    },
};
use skein_lib::Secret;
const APP: u64 = 7;
const BOT: u64 = 9;
const SLUG: &[u8] = b"test-app";
const ITEMS: [Item; 4] = [Item::AppId, Item::AppSlug, Item::AppKey, Item::User];
fn fixture(script: &[Connect]) -> HostWorld {
    let mut world = HostWorld::github_scenario(7, script, AppFacts::Omitted, b"https://wrong.test/");
    world.paste(fixtures::secret(&keys::pem(app_key()).expect("public PEM")));
    world
}
fn paste(world: &mut HostWorld) {
    world.run_plan(1, Plan::Paste { forge: FORGE, app: NonZeroU64::new(APP).expect("App id") });
    world.settle();
}
fn done(world: &HostWorld) {
    diagnostics(world, &keys::pem(app_key()).expect("PEM"), app_key());
    let [Heard::Done { outcome: Outcome::Lacking { lacking }, .. }] = world.heard() else { panic!("Lacking") };
    assert!(
        lacking
            .iter()
            .any(|entry| matches!(entry, Lacking::Permission { name, .. } if name.as_ref() == b"pull_requests"))
    );
}
fn identity_items() -> [(Item, Secret); 4] {
    [
        (Item::AppId, fixtures::secret(&APP.to_le_bytes())),
        (Item::AppSlug, fixtures::secret(SLUG)),
        (Item::AppKey, fixtures::secret(app_key())),
        (Item::User, fixtures::secret(&BOT.to_le_bytes())),
    ]
}
fn identity(world: &HostWorld) {
    for (item, bytes) in identity_items() {
        assert_eq!(world.file(item), Some(&bytes));
    }
    assert!(world.file(Item::Pasted).is_none());
    assert!(world.file(Item::WebhookSecret).is_none());
}
#[test]
fn the_host_connects_a_pasted_app_and_names_what_it_lacks() {
    // Three signed requests plus three fake checks: App, hook GET and PATCH.
    let mut world = fixture(&[]);
    for item in ITEMS {
        world.forget(item);
    }
    let before = accepted(&world);
    world.pause_writes();
    paste(&mut world);
    for (item, value) in identity_items() {
        assert!(world.heard().is_empty(), "Done waits for User Written");
        assert_eq!(world.needs().last(), Some(&Need::Write { item, bytes: value.clone() }));
        assert!(world.file(item).is_none());
        world.write_pending();
        world.settle();
        assert_eq!(world.file(item), Some(&value));
    }
    done(&world);
    identity(&world);
    assert_eq!(accepted(&world), before);
    assert_eq!(world.reads(), [Item::Pasted, Item::AppId, Item::User]);
    assert_eq!(writes(&world, 0), ITEMS);
    assert_eq!(mutations(&world, 0), [("PATCH".into(), "/app/hook/config".into())]);
    let routes: Vec<_> =
        world.github_requests().iter().map(|row| (row.method.as_str(), row.target.as_str(), row.auth)).collect();
    assert_eq!(
        routes,
        [
            ("GET", "/app", "App"),
            ("GET", "/users/test-app%5Bbot%5D", "None"),
            ("GET", "/app/hook/config", "App"),
            ("PATCH", "/app/hook/config", "App")
        ]
    );
    let (state, url) = install(&mut world, 2);
    let expected = format!(
        "https://github.test/apps/test-app/installations/new?state={}",
        core::str::from_utf8(&state).expect("state")
    );
    assert_eq!(url.as_ref(), expected.as_bytes());
    assert_eq!(world.github_installation_ids(), [1, 2, 3]);
    closed(&mut world);
}
#[test]
fn a_paste_replaces_the_key_for_the_same_kept_identity() {
    // Six signings including fake checks; no key rotation scenario is required.
    let mut world = fixture(&[]);
    world.put(Item::AppKey, fixtures::secret(b"corrupt-kept-key"));
    world.put(Item::User, fixtures::secret(&BOT.to_le_bytes()));
    let before = accepted(&world);
    paste(&mut world);
    done(&world);
    identity(&world);
    assert_eq!(writes(&world, 0), ITEMS);
    assert_eq!(accepted(&world), before);
    closed(&mut world);
}
#[test]
fn a_pasted_different_app_or_bot_changes_no_hook_or_item() {
    // Other App signs nothing; other bot signs only the App proof and fake check.
    for (item, id, why, targets) in [(Item::AppId, 8_u64, Why::OtherApp, 0), (Item::User, 10_u64, Why::OtherUser, 2)] {
        let mut world = fixture(&[]);
        world.put(item, fixtures::secret(&id.to_le_bytes()));
        let before: Vec<_> =
            ITEMS.into_iter().chain([Item::Accepted]).map(|item| (item, world.file(item).cloned())).collect();
        paste(&mut world);
        let [Heard::Stop { failure, .. }] = world.heard() else { panic!("Stop") };
        assert_eq!(failure.step, Stage::App);
        assert_eq!(failure.why, why);
        diagnostics(&world, &keys::pem(app_key()).expect("PEM"), app_key());
        assert_eq!(world.github_requests().len(), targets);
        assert!(writes(&world, 0).is_empty());
        assert!(mutations(&world, 0).is_empty());
        for (item, value) in before {
            assert_eq!(world.file(item), value.as_ref());
        }
        closed(&mut world);
    }
}
fn diagnostics(world: &HostWorld, pem: &[u8], der: &[u8]) {
    let debug = format!("{:?} {:?} {:?}", world.github_requests(), world.needs(), world.heard());
    for secret in [pem, der, b"pasted-sentinel", b"eyJ"] {
        assert!(!debug.as_bytes().windows(secret.len()).any(|part| part == secret));
        for spelling in [format!("{secret:?}"), format!("{secret:x?}"), format!("{secret:X?}")] {
            assert!(!debug.contains(&spelling));
        }
    }
}
#[test]
fn a_pasted_key_is_never_in_failure_or_trace() {
    // Only the scripted 401 signs; all three failures avoid fake signature checks.
    let unsigned = b"\x30\x10pasted-sentinel!"; // Valid DER envelope, no RSA private key.
    for (pem, der, script, why) in [
        (
            b"-----BEGIN PRIVATE KEY-----\npasted-sentinel\n-----END PRIVATE KEY-----".to_vec().into_boxed_slice(),
            b"pasted-sentinel".as_slice(),
            &[][..],
            Why::KeyForm,
        ),
        (keys::pem(unsigned).expect("PEM"), unsigned.as_slice(), &[][..], Why::KeyForm),
        (keys::pem(app_key()).expect("PEM"), app_key(), &[Connect::Answers(401, b"{}")][..], Why::Refused(401)),
    ] {
        let mut world = fixture(script);
        world.paste(fixtures::secret(&pem));
        paste(&mut world);
        let [Heard::Stop { failure, .. }] = world.heard() else { panic!("Stop") };
        assert_eq!(failure.why, why);
        diagnostics(&world, &pem, der);
        assert!(writes(&world, 0).is_empty());
        closed(&mut world);
    }
    // Success uses six signings including fake checks, and keeps DER only in AppKey.
    let mut world = fixture(&[]);
    paste(&mut world);
    done(&world);
    diagnostics(&world, &keys::pem(app_key()).expect("PEM"), app_key());
    identity(&world);
    for item in [Item::Accepted, Item::AppId, Item::AppSlug, Item::User, Item::Pasted, Item::WebhookSecret] {
        if let Some(value) = world.file(item) {
            assert!(!value.expose().windows(app_key().len()).any(|part| part == app_key()));
        }
    }
    closed(&mut world);
}
