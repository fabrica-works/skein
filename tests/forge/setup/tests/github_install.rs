use core::num::NonZeroU64;
use skein_forge_setup::{
    github::manifest::Target,
    host::{Kind, Need, Plan},
    item::{Item, accepted_item},
    reconcile::{Failure, Outcome, Stage, Why},
};
use skein_forge_setup_world::{Connect, Heard, HostWorld, fixtures};
use skein_lib::{Duration, Time, Token};

const FORGE: u16 = 4; // fixtures::github_forge.
const INSTALLATION: u64 = 2; // The fake's acme installation.
const SLUG: &[u8] = b"kit-app";
fn world(script: &[Connect]) -> HostWorld {
    let mut world = HostWorld::github(7, script);
    world.put(Item::AppSlug, fixtures::secret(SLUG));
    world.forget(Item::Accepted);
    world
}
fn install(world: &mut HostWorld, run: u64) -> Box<[u8]> {
    world.run_plan(run, Plan::Install { forge: FORGE });
    world.settle();
    match world.heard().last().expect("Install finished") {
        Heard::Done { outcome: Outcome::Install { state, url }, .. } => {
            let mut expected = b"https://github.test/apps/kit-app/installations/new?state=".to_vec();
            expected.extend_from_slice(state);
            assert_eq!(url.as_ref(), expected);
            state.clone()
        }
        event @ (Heard::Done { .. } | Heard::Stop { .. } | Heard::Closed { .. }) => {
            panic!("install outcome: {event:?}")
        }
    }
}
fn installed() -> (HostWorld, Box<[u8]>) {
    let mut world = world(&[]);
    let state = install(&mut world, 1);
    (world, state)
}
fn accept(world: &mut HostWorld, run: u64, state: &[u8], installation: u64) {
    world.run_plan(
        run,
        Plan::Accept {
            forge: FORGE,
            installation: NonZeroU64::new(installation).expect("nonzero fixture"),
            state: state.into(),
        },
    );
    world.settle();
}
fn stopped(run: u64, step: Stage, subject: &[u8], why: Why, at: Duration) -> Heard {
    Heard::Stop { run: Token::new(run), failure: Failure { forge: FORGE, step, subject: subject.into(), why }, at }
}
fn done(run: u64) -> Heard {
    Heard::Done { run: Token::new(run), outcome: Outcome::Nothing, at: Duration::ZERO }
}
fn closed(world: &mut HostWorld) {
    world.close();
    world.settle();
    assert!(matches!(world.heard().last(), Some(Heard::Closed { .. })));
    let count = world.heard().len();
    world.finish();
    assert_eq!(world.heard().len(), count);
}
#[test]
fn the_host_accepts_an_installation_from_its_install_state() {
    let mut world = world(&[]);
    let state = install(&mut world, 1);
    let second = install(&mut world, 2);
    assert_ne!(state, second);
    world.pause_writes();
    accept(&mut world, 3, &state, INSTALLATION);
    assert_eq!(world.heard().len(), 2, "Accepted is not durable yet");
    assert!(world.file(Item::Accepted).is_none());
    assert_eq!(world.reads(), &[Item::AppSlug, Item::AppSlug, Item::AppId, Item::AppKey, Item::Accepted]);
    assert_eq!(
        world.needs().last(),
        Some(&Need::Write { item: Item::Accepted, bytes: accepted_item(&[INSTALLATION]) })
    );
    world.write_pending();
    world.settle();
    assert_eq!(world.heard().last(), Some(&done(3)));
    assert_eq!(world.file(Item::Accepted), Some(&accepted_item(&[INSTALLATION])));
    assert_eq!(world.connects(), &[Duration::ZERO]);
    closed(&mut world);
}
#[test]
fn a_reused_or_wrong_install_state_sends_nothing() {
    let mut convert = world(&[]);
    convert.run_plan(1, Plan::Manifest { forge: FORGE, target: Target::Personal });
    convert.settle();
    let Some(Heard::Done { outcome: Outcome::Manifest { state: convert_state, .. }, .. }) = convert.heard().last()
    else {
        panic!("manifest")
    };
    let convert_state = convert_state.clone();
    let (mut expired, expired_state) = installed();
    let life = fixtures::github_given().limits.state_life;
    expired.advance_to(Time::from_nanos(life.as_nanos()));
    let (mut previous, restart_state) = installed();
    closed(&mut previous);
    let (mut reused, reused_state) = installed();
    accept(&mut reused, 2, &reused_state, INSTALLATION);
    for (mut world, state, at) in [
        (world(&[]), b"00000000000000000000000000000000".as_slice().into(), Duration::ZERO),
        (convert, convert_state, Duration::ZERO),
        (expired, expired_state, life),
        (world(&[]), restart_state, Duration::ZERO),
        (reused, reused_state, Duration::ZERO),
    ] {
        let reads = world.reads().len();
        let sends = world.connects().len();
        accept(&mut world, 3, &state, INSTALLATION);
        assert_eq!(world.heard().last(), Some(&stopped(3, Stage::State, b"accept", Why::Unknown, at)));
        assert_eq!(world.reads().len(), reads);
        assert_eq!(world.connects().len(), sends);
        closed(&mut world);
    }
    // A configured second GitHub forge cannot consume the first forge's binding.
    let mut settings = fixtures::github_settings();
    let Kind::GitHub(mut other) = fixtures::github_forge().kind else { panic!("GitHub fixture") };
    other.forge = 5;
    settings.forges.get_mut(0).expect("second forge").kind = Kind::GitHub(other);
    let mut world = HostWorld::new_with(7, &[], settings, fixtures::host_limits());
    world.put(Item::AppSlug, fixtures::secret(SLUG));
    let state = install(&mut world, 1);
    world.run_plan(2, Plan::Accept { forge: 5, installation: NonZeroU64::new(INSTALLATION).expect("fixture"), state });
    world.settle();
    let Some(Heard::Stop { failure, .. }) = world.heard().last() else { panic!("wrong forge") };
    assert_eq!(
        failure,
        &Failure { forge: 5, step: Stage::State, subject: b"accept".as_slice().into(), why: Why::Unknown }
    );
    assert_eq!(world.reads(), &[Item::AppSlug]);
    assert!(world.connects().is_empty());
    closed(&mut world);
}
#[test]
fn a_failed_accept_still_uses_its_state() {
    let missing = world(&[Connect::Answers(404, b"{}")]);
    let mut no_id = world(&[]);
    no_id.forget(Item::AppId);
    let mut full = world(&[]);
    full.put(Item::Accepted, accepted_item(&[1, 3, 4, 5, 6, 7, 8, 9]));
    for (mut world, step, subject, why, connections) in [
        (missing, Stage::Installation, b"2".as_slice(), Why::Missing, 1),
        (no_id, Stage::App, b"id".as_slice(), Why::Missing, 0),
        (full, Stage::Installation, b"2".as_slice(), Why::Unexpected, 1),
    ] {
        let before = world.file(Item::Accepted).cloned();
        let state = install(&mut world, 1);
        accept(&mut world, 2, &state, INSTALLATION);
        assert_eq!(world.heard().last(), Some(&stopped(2, step, subject, why, Duration::ZERO)));
        let reads = world.reads().len();
        accept(&mut world, 3, &state, INSTALLATION);
        assert_eq!(world.heard().last(), Some(&stopped(3, Stage::State, b"accept", Why::Unknown, Duration::ZERO)));
        assert_eq!(world.reads().len(), reads);
        assert_eq!(world.connects().len(), connections);
        assert_eq!(world.file(Item::Accepted), before.as_ref());
        closed(&mut world);
    }
}
#[test]
fn a_busy_accept_leaves_its_state_usable() {
    let mut world = world(&[]);
    let state = install(&mut world, 1);
    world.pause_reads();
    world.run_plan(2, Plan::Install { forge: FORGE });
    world.settle();
    accept(&mut world, 3, &state, INSTALLATION);
    assert_eq!(world.heard().last(), Some(&stopped(3, Stage::App, b"accept", Why::Busy, Duration::ZERO)));
    world.read_pending(2);
    world.settle();
    accept(&mut world, 4, &state, INSTALLATION);
    assert_eq!(world.pending_reads(), &[(Token::new(4), Item::AppId)]);
    assert!(world.connects().is_empty());
    closed(&mut world);
}
#[test]
fn accept_keeps_no_key_or_state_in_its_trace() {
    let mut world = world(&[Connect::Answers(404, b"{}")]);
    let state = install(&mut world, 1);
    accept(&mut world, 2, &state, INSTALLATION);
    let trace = format!("{:?} {:?}", &world.heard()[1..], world.needs());
    for secret in [skein_forge_machine_world::github::app_key(), state.as_ref(), b"ghs_", b"eyJ"] {
        assert!(!trace.as_bytes().windows(secret.len()).any(|bytes| bytes == secret));
        assert!(!trace.contains(&format!("{secret:?}")));
    }
    assert_eq!(world.file(Item::AppKey), Some(&fixtures::secret(skein_forge_machine_world::github::app_key())));
    for item in [
        Item::AppId,
        Item::AppSlug,
        Item::Accepted,
        Item::User,
        Item::ApiToken,
        Item::GitToken,
        Item::WebhookSecret,
        Item::Pasted,
    ] {
        if let Some(value) = world.file(item) {
            assert!(!value.expose().windows(state.len()).any(|bytes| bytes == state.as_ref()));
        }
    }
    closed(&mut world);
}
