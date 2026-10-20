use super::{
    Answer, Event, Host, Kind, MAX_OUT, Plan, Request,
    states::{Purpose, States},
};
use crate::{
    item::Item,
    reconcile::{Failure, Stage, Why},
    tests::{fixtures::secret, host as fixtures},
};
use core::num::NonZeroU64;
use skein_lib::{Duration, Env, Queue, Time, Token, Wall};

#[test]
fn a_state_differing_only_in_its_last_byte_is_unknown() {
    let mut states = States::new(1);
    let state =
        states.issue(4, Purpose::Accept, &[0; 16], Time::ZERO, Duration::from_secs(600), b"install").expect("state");
    let mut wrong = state.clone();
    *wrong.last_mut().expect("32 bytes") = b'1';
    assert_eq!(
        states.take(4, Purpose::Accept, &wrong, Time::ZERO),
        Err(Failure { forge: 4, step: Stage::State, subject: b"accept".as_slice().into(), why: Why::Unknown })
    );
    assert_eq!(states.take(4, Purpose::Accept, &state, Time::ZERO), Ok(()));
}

#[test]
fn install_failures_drop_their_state() {
    let limits = fixtures::limits();
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut host = Host::new(&limits, fixtures::github_settings(), fixtures::crypto()).expect("host");
    let mut up = Queue::with_capacity(MAX_OUT.above);
    let mut io = Queue::with_capacity(MAX_OUT.below);
    let mut rng = fixtures::crypto();
    let mut random = [0; 16];
    rng.fill(&mut random);
    let mut expected = States::new(1);
    let first =
        expected.issue(4, Purpose::Accept, &random, Time::ZERO, Duration::from_secs(600), b"install").expect("state");
    for run in [1, 2] {
        assert!(up.room() >= MAX_OUT.above && io.room() >= MAX_OUT.below);
        host.down(&env, Request::Run { run: Token::new(run), plan: Plan::Install { forge: 4 } }, &mut up, &mut io);
        assert_eq!(up.pop(), Some(Event::Need { run: Token::new(run), need: super::Need::Read(Item::AppSlug) }));
        assert!(up.room() >= MAX_OUT.above && io.room() >= MAX_OUT.below);
        host.down(&env, Request::Answer { run: Token::new(run), answer: Answer::Read(None) }, &mut up, &mut io);
        assert_eq!(
            up.pop(),
            Some(Event::Stop {
                run: Token::new(run),
                failure: Failure { forge: 4, step: Stage::App, subject: b"slug".as_slice().into(), why: Why::Missing }
            })
        );
        assert!(io.is_empty());
    }
    assert!(host.states.take(4, Purpose::Accept, &first, Time::ZERO).is_err());
    assert_eq!(host.next_deadline(), None);
}

#[test]
fn wrong_forge_acceptance_preserves_the_right_binding_and_kind_checks_come_first() {
    let limits = fixtures::limits();
    let mut settings = fixtures::github_settings();
    let mut other = crate::tests::github_fixtures::given();
    other.forge = 5;
    settings.forges.get_mut(0).expect("fixture").kind = Kind::GitHub(other);
    let mut host = Host::new(&limits, settings, fixtures::crypto()).expect("host");
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut up = Queue::with_capacity(MAX_OUT.above);
    let mut io = Queue::with_capacity(MAX_OUT.below);
    let state = host
        .states
        .issue(4, Purpose::Accept, &[0; 16], Time::ZERO, Duration::from_secs(600), b"install")
        .expect("state");
    assert!(up.room() >= MAX_OUT.above && io.room() >= MAX_OUT.below);
    host.down(
        &env,
        Request::Run {
            run: Token::new(1),
            plan: Plan::Accept { forge: 5, installation: NonZeroU64::new(2).expect("id"), state: state.clone() },
        },
        &mut up,
        &mut io,
    );
    assert_eq!(
        up.pop(),
        Some(Event::Stop {
            run: Token::new(1),
            failure: Failure { forge: 5, step: Stage::State, subject: b"accept".as_slice().into(), why: Why::Unknown }
        })
    );
    assert_eq!(host.states.take(4, Purpose::Accept, &state, Time::ZERO), Ok(()));
    assert!(io.is_empty());
    assert_eq!(host.next_deadline(), None);
    let mut plain = Host::new(&env.limits, fixtures::github_settings(), fixtures::crypto()).expect("host");
    for plan in [
        Plan::Install { forge: 1 },
        Plan::Accept {
            forge: 1,
            installation: NonZeroU64::new(2).expect("id"),
            state: secret(b"unknown").expose().into(),
        },
    ] {
        assert!(up.room() >= MAX_OUT.above && io.room() >= MAX_OUT.below);
        plain.down(&env, Request::Run { run: Token::new(1), plan }, &mut up, &mut io);
        let Some(Event::Stop { failure, .. }) = up.pop() else { panic!("kind refusal") };
        assert_eq!(failure.why, Why::Unexpected);
        assert!(io.is_empty());
    }
}
