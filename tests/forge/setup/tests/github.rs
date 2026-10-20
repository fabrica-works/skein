use core::num::NonZeroU32;
use skein_forge_machine_world::github::app_key;
use skein_forge_setup::{
    host::{Need, Plan},
    item::{self, Item},
    reachable::Reached,
    reconcile::{Failure, Outcome, Stage, Why},
};
use skein_forge_setup_world::{Connect, Heard, HostWorld, fixtures};
use skein_lib::{Duration, Token};

#[test]
fn a_forgejo_reconcile_converges_beside_a_configured_github_forge() {
    let settings = fixtures::github_settings();
    let github_address = settings.forges.get(1).expect("GitHub forge").address;
    let mut world = HostWorld::new_with(7, &[], settings, fixtures::host_limits());
    world.run(1);
    world.finish();
    assert_eq!(world.heard(), &[Heard::Done { run: Token::new(1), outcome: Outcome::Nothing, at: Duration::ZERO }]);
    let rendered = format!("{:?}", world.heard());
    for secret in [fixtures::ADMIN, fixtures::API, fixtures::GIT] {
        for bytes in rendered.as_bytes().windows(secret.len()) {
            assert_ne!(bytes, secret, "credentials stay out of Debug");
        }
    }
    assert_eq!(world.file(Item::User).expect("User written").expose(), &2u64.to_le_bytes());
    for key in [Item::ApiToken, Item::GitToken] {
        assert!(item::token(world.file(key).expect("token written").expose(), 64).is_some());
    }
    assert!(!world.connection_addresses().is_empty());
    assert!(!world.connection_addresses().contains(&github_address));
    let mut plain = HostWorld::new(7, &[]);
    plain.run(1);
    plain.finish();
    for key in [Item::User, Item::AdminToken, Item::ApiToken, Item::GitToken, Item::WebhookSecret] {
        assert_eq!(world.file(key), plain.file(key));
    }
}

fn picker(installation: Option<u64>, page: u32) -> Plan {
    Plan::Reachable { forge: 4, installation, page: NonZeroU32::new(page).expect("page from one") }
}
fn row(id: u64, name: &[u8], private: bool) -> Reached {
    Reached { id, owner_id: 22, owner: b"acme".as_slice().into(), name: name.into(), private }
}
fn done(run: u64, rows: &[Reached], more: bool) -> Heard {
    Heard::Done { run: Token::new(run), outcome: Outcome::Reachable { rows: rows.into(), more }, at: Duration::ZERO }
}
fn stop(run: u64, step: Stage, subject: &[u8], why: Why, at: Duration) -> Heard {
    Heard::Stop { run: Token::new(run), failure: Failure { forge: 4, step, subject: subject.into(), why }, at }
}
fn hidden(world: &HostWorld) {
    let rendered = format!("{:?} {:?}", world.heard(), world.needs());
    for secret in [app_key(), skein_fake_forge_github::keys::KEYS[1], b"ghs_", b"eyJ"] {
        assert!(!rendered.as_bytes().windows(secret.len()).any(|bytes| bytes == secret), "Debug redacts credentials");
    }
}
fn assert_heard(world: &HostWorld, expected: &[Heard]) {
    hidden(world);
    assert_eq!(world.heard(), expected);
}
#[test]
fn the_host_lists_the_first_page_of_an_installations_repositories() {
    let mut world = HostWorld::github(7, &[]);
    world.github_all_repositories(1);
    world.run_plan(1, picker(Some(1), 1));
    world.finish();
    assert_heard(&world, &[done(1, &[row(2, b"widgets", true), row(4, b"tools", false)], true)]);
    assert_eq!(world.reads(), &[Item::Accepted, Item::AppId, Item::AppKey]);
    assert!(world.needs().iter().all(|need| matches!(need, Need::Read(_))));
    assert_eq!(world.connects(), &[Duration::ZERO; 2]);
    assert_eq!(world.connection_addresses(), &[fixtures::github_forge().address; 2]);
}
#[test]
fn the_host_lists_the_last_page_of_an_installations_repositories() {
    let mut world = HostWorld::github(11, &[]);
    world.github_all_repositories(1);
    world.run_plan(1, picker(Some(1), 2));
    world.finish();
    assert_heard(&world, &[done(1, &[row(6, b"absent", false)], false)]);
}
#[test]
fn a_full_page_says_more_even_when_it_is_the_last() {
    let mut world = HostWorld::github(13, &[]);
    world.run_plan(1, picker(Some(1), 1));
    world.finish();
    assert_heard(&world, &[done(1, &[row(2, b"widgets", true), row(4, b"tools", false)], true)]);
}
#[test]
fn an_installation_no_state_accepted_stops_missing_and_sends_nothing() {
    let mut world = HostWorld::github(17, &[]);
    world.run_plan(1, picker(Some(3), 1));
    world.finish();
    assert_heard(&world, &[stop(1, Stage::Installation, b"3", Why::Missing, Duration::ZERO)]);
    assert_eq!(world.reads(), &[Item::Accepted]);
    assert!(world.connects().is_empty());
}
#[test]
fn an_accepted_installation_the_app_does_not_have_stops_missing() {
    let mut world = HostWorld::github(19, &[]);
    world.put(Item::Accepted, fixtures::accepted(&[9]));
    world.run_plan(1, picker(Some(9), 1));
    world.finish();
    assert_heard(&world, &[stop(1, Stage::Installation, b"9", Why::Missing, Duration::ZERO)]);
    assert_eq!(world.connects(), &[Duration::ZERO]);
}
#[test]
fn a_key_the_fake_does_not_accept_stops_refused_401_at_the_installation() {
    let mut world = HostWorld::github(23, &[]);
    world.put(Item::AppKey, fixtures::secret(skein_fake_forge_github::keys::KEYS[1]));
    world.run_plan(1, picker(Some(2), 1));
    world.finish();
    assert_heard(&world, &[stop(1, Stage::Installation, b"2", Why::Refused(401), Duration::ZERO)]);
    assert_eq!(world.connects(), &[Duration::ZERO]);
}
#[test]
fn a_key_that_cannot_sign_stops_key_form_and_sends_nothing() {
    let mut world = HostWorld::github(29, &[]);
    world.put(Item::AppKey, fixtures::secret(b"\x30\x03\x02\x01\x07"));
    world.run_plan(1, picker(Some(2), 1));
    world.finish();
    assert_heard(&world, &[stop(1, Stage::App, b"key", Why::KeyForm, Duration::ZERO)]);
    assert!(world.connects().is_empty());
}
#[test]
fn a_missing_app_id_or_key_stops_missing_at_the_app() {
    for (item, subject) in [(Item::AppId, b"id".as_slice()), (Item::AppKey, b"key")] {
        let mut world = HostWorld::github(31, &[]);
        world.forget(item);
        world.run_plan(1, picker(Some(2), 1));
        world.finish();
        assert_heard(&world, &[stop(1, Stage::App, subject, Why::Missing, Duration::ZERO)]);
        assert!(world.connects().is_empty());
    }
}
#[test]
fn a_github_reachable_with_no_installation_stops_at_once_in_the_world() {
    let mut world = HostWorld::github(37, &[]);
    world.run_plan(1, picker(None, 1));
    world.finish();
    assert_heard(&world, &[stop(1, Stage::Installation, b"none", Why::Unexpected, Duration::ZERO)]);
    assert!(world.reads().is_empty());
    assert!(world.connects().is_empty());
}
#[test]
fn a_second_github_picker_stops_busy_while_the_first_is_held() {
    let mut world = HostWorld::github(41, &[Connect::Silent]);
    world.run_plan(1, picker(Some(2), 1));
    world.settle();
    assert!(world.heard().is_empty());
    world.run_plan(2, picker(Some(2), 1));
    world.settle();
    assert_heard(&world, &[stop(2, Stage::App, b"reachable", Why::Busy, Duration::ZERO)]);
    world.finish();
    assert_heard(
        &world,
        &[
            stop(2, Stage::App, b"reachable", Why::Busy, Duration::ZERO),
            stop(1, Stage::Installation, b"2", Why::Down, Duration::from_secs(120)),
        ],
    );
    assert_eq!(world.connects(), &[Duration::ZERO]);
    assert_eq!(world.aborts(), 1);
}
#[test]
fn a_github_picker_and_a_forgejo_reconcile_run_at_once_on_their_forges() {
    let mut world = HostWorld::github(43, &[]);
    world.github_all_repositories(1);
    world.run(1);
    world.run_plan(2, picker(Some(1), 1));
    world.finish();
    hidden(&world);
    assert_eq!(world.heard().len(), 2);
    assert!(world.heard().contains(&Heard::Done { run: Token::new(1), outcome: Outcome::Nothing, at: Duration::ZERO }));
    assert!(world.heard().contains(&done(2, &[row(2, b"widgets", true), row(4, b"tools", false)], true)));
    let mut plain = HostWorld::new(43, &[]);
    plain.run(1);
    plain.finish();
    for item in [Item::AdminToken, Item::User, Item::ApiToken, Item::GitToken, Item::WebhookSecret] {
        assert_eq!(world.file(item), plain.file(item));
    }
}
#[test]
fn no_token_or_jwt_is_heard_or_written() {
    let mut world = HostWorld::github(47, &[]);
    world.github_all_repositories(1);
    world.run_plan(1, picker(Some(1), 1));
    world.finish();
    assert_heard(&world, &[done(1, &[row(2, b"widgets", true), row(4, b"tools", false)], true)]);
    assert!(world.needs().iter().all(|need| matches!(need, Need::Read(_))));
    for (item, expected) in [
        (Item::AdminToken, fixtures::secret(fixtures::ADMIN)),
        (Item::AppId, fixtures::app_id(7)),
        (Item::AppKey, fixtures::secret(app_key())),
        (Item::Accepted, fixtures::accepted(&[1, 2])),
    ] {
        assert_eq!(world.file(item), Some(&expected));
    }
    for item in [Item::User, Item::ApiToken, Item::GitToken, Item::WebhookSecret, Item::AppSlug, Item::Pasted] {
        assert!(world.file(item).is_none());
    }
}
