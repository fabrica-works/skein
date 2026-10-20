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

const CONVERT_FORGE: u16 = 4;
const CONVERT_CODE: &[u8] = b"private-conversion-code";
const STATE_LIFE: Duration = Duration::from_secs(600);

struct ConvertHost {
    host: Host,
    env: Env<super::Limits>,
    up: Queue<Event>,
    io: Queue<skein_io::Request>,
}
impl ConvertHost {
    fn new() -> Self {
        let limits = fixtures::limits();
        Self {
            host: Host::new(&limits, fixtures::github_settings(), fixtures::crypto()).expect("host"),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            up: Queue::with_capacity(MAX_OUT.above),
            io: Queue::with_capacity(MAX_OUT.below),
        }
    }
    fn state(&mut self, forge: u16, purpose: Purpose) -> Box<[u8]> {
        self.host.states.issue(forge, purpose, &[0; 16], Time::ZERO, STATE_LIFE, b"manifest").expect("state")
    }
    fn down(&mut self, request: Request) {
        assert!(self.up.room() >= MAX_OUT.above && self.io.room() >= MAX_OUT.below);
        self.host.down(&self.env, request, &mut self.up, &mut self.io);
        assert!(self.up.len() <= 1, "one outstanding owner need");
    }
    fn convert(&mut self, run: u64, forge: u16, state: &[u8]) {
        self.down(Request::Run {
            run: Token::new(run),
            plan: Plan::Convert { forge, code: secret(CONVERT_CODE), state: state.into() },
        });
    }
    fn unknown(&mut self, run: u64) {
        assert_eq!(
            self.up.pop(),
            Some(Event::Stop {
                run: Token::new(run),
                failure: Failure {
                    forge: CONVERT_FORGE,
                    step: Stage::State,
                    subject: b"convert".as_slice().into(),
                    why: Why::Unknown,
                },
            })
        );
        assert!(self.up.is_empty() && self.io.is_empty());
    }
    fn close(&mut self) {
        let connection = match self.io.pop() {
            Some(skein_io::Request::Connect { owner, .. }) => Some(owner),
            None => None,
            Some(_) => panic!("expected connect"),
        };
        assert!(self.io.is_empty());
        self.down(Request::Close);
        if let Some(owner) = connection {
            assert!(self.up.room() >= MAX_OUT.above && self.io.room() >= MAX_OUT.below);
            self.host.up(&self.env, skein_io::Event::Closed { owner }, &mut self.up, &mut self.io);
        }
        assert_eq!(self.up.pop(), Some(Event::Closed));
        assert!(self.up.is_empty() && self.io.is_empty());
        assert_eq!(self.host.next_deadline(), None);
        assert!(self.up.room() >= MAX_OUT.above && self.io.room() >= MAX_OUT.below);
        self.host.fire(&self.env, &mut self.up, &mut self.io);
        assert!(self.up.is_empty() && self.io.is_empty(), "ended host is silent");
    }
}

#[test]
fn convert_takes_its_state_before_the_first_send() {
    for (state, now) in [
        (b"unknown".as_slice(), Time::ZERO),
        (b"00000000000000000000000000000001", Time::ZERO),
        (b"00000000000000000000000000000000", Time::from_nanos(STATE_LIFE.as_nanos())),
    ] {
        let mut world = ConvertHost::new();
        world.state(CONVERT_FORGE, Purpose::Convert);
        world.env.now = now;
        world.convert(1, CONVERT_FORGE, state);
        world.unknown(1);
        world.close();
    }
    let mut world = ConvertHost::new();
    let state = world.state(CONVERT_FORGE, Purpose::Convert);
    world.convert(1, CONVERT_FORGE, &state);
    assert!(world.up.is_empty());
    assert_eq!(world.io.len(), 1, "first send opens one connection without an item read");
    assert!(world.host.states.take(CONVERT_FORGE, Purpose::Convert, &state, Time::ZERO).is_err());
    let send = world.host.runs.get(&Token::new(1)).expect("running conversion").send.as_ref().expect("first send");
    match &send.call {
        crate::call::Call::GitHub(skein_forge::github::request::Request::Conversion { code }) => {
            assert!(code.as_ref() == CONVERT_CODE);
        }
        crate::call::Call::GitHub(_) | crate::call::Call::Forgejo(_) => panic!("conversion request"),
    }
    world.close();
}

#[test]
fn a_wrong_binding_or_busy_convert_does_not_take_a_valid_state() {
    for (forge, purpose, callback, step, why) in [
        (5, Purpose::Convert, CONVERT_FORGE, Stage::State, Why::Unknown),
        (CONVERT_FORGE, Purpose::Accept, CONVERT_FORGE, Stage::State, Why::Unknown),
        (CONVERT_FORGE, Purpose::Convert, 1, Stage::Admin, Why::Unexpected),
    ] {
        let mut world = ConvertHost::new();
        let state = world.state(forge, purpose);
        world.convert(1, callback, &state);
        let Some(Event::Stop { failure, .. }) = world.up.pop() else { panic!("admission refusal") };
        assert_eq!(failure.step, step);
        assert_eq!(failure.why, why);
        assert!(world.io.is_empty());
        assert_eq!(world.host.states.take(forge, purpose, &state, Time::ZERO), Ok(()));
        world.close();
    }
    // Both the forge's writing slot and the host's total run cap precede take.
    for (plan, runs) in [(Plan::Install { forge: CONVERT_FORGE }, 2), (Plan::Check { forge: 1 }, 1)] {
        let mut world = ConvertHost::new();
        world.env.limits.runs = runs;
        world.host = Host::new(&world.env.limits, fixtures::github_settings(), fixtures::crypto()).expect("host");
        let state = world.state(CONVERT_FORGE, Purpose::Convert);
        world.down(Request::Run { run: Token::new(1), plan });
        match world.up.pop() {
            Some(Event::Need { .. }) => (),
            Some(Event::Done { .. } | Event::Stop { .. } | Event::Closed) | None => panic!("held owner need"),
        }
        world.convert(2, CONVERT_FORGE, &state);
        assert_eq!(
            world.up.pop(),
            Some(Event::Stop {
                run: Token::new(2),
                failure: Failure {
                    forge: CONVERT_FORGE,
                    step: Stage::App,
                    subject: b"convert".as_slice().into(),
                    why: Why::Busy,
                },
            })
        );
        assert!(world.io.is_empty());
        assert_eq!(world.host.states.take(CONVERT_FORGE, Purpose::Convert, &state, Time::ZERO), Ok(()));
        world.close();
    }
}

#[test]
fn a_started_failed_convert_cannot_reuse_its_state() {
    for (reply, why) in [(Some(crate::reconcile::Reply::Status(404)), Why::Refused(404)), (None, Why::Down)] {
        let mut world = ConvertHost::new();
        let state = world.state(CONVERT_FORGE, Purpose::Convert);
        // A terminal scripted reply or the deadline halts at the conversion step.
        let forge = super::configured(&world.host.settings, CONVERT_FORGE);
        assert!(world.host.plan_out.room() >= crate::reconcile::MAX_OUT);
        let mut run = super::run::Run::new(
            forge,
            Plan::Convert { forge: CONVERT_FORGE, code: secret(CONVERT_CODE), state: state.clone() },
            super::bounds(&world.env.limits, forge),
            &world.env,
            &mut world.host.crypto,
            &mut world.host.states,
            &mut world.host.plan_out,
        )
        .expect("admitted conversion");
        match reply {
            Some(reply) => run.reply(reply, &mut world.host.plan_out),
            None => run.halt(why, &mut world.host.plan_out),
        }
        assert_eq!(
            run.pending.take(),
            Some(crate::reconcile::Need::Stop(Failure {
                forge: CONVERT_FORGE,
                step: Stage::App,
                subject: b"convert".as_slice().into(),
                why,
            }))
        );
        world.convert(2, CONVERT_FORGE, &state);
        world.unknown(2);
        world.close();
    }
}

const RECONCILE_RUN: Token = Token::new(1);
const PICKER_RUN: Token = Token::new(2);
const OTHER_WRITER: Token = Token::new(3);

struct ReconcileHost {
    host: Host,
    env: Env<super::Limits>,
    up: Queue<Event>,
    io: Queue<skein_io::Request>,
}
impl ReconcileHost {
    fn new() -> Self {
        let limits = fixtures::limits();
        let mut settings = fixtures::github_settings();
        // Distinct deadlines prove the GitHub forge supplies its own times.
        match &mut settings.forges.get_mut(1).expect("GitHub forge").kind {
            Kind::GitHub(given) => given.limits.deadline = Duration::from_secs(7),
            Kind::Forgejo(_) => panic!("GitHub fixture"),
        }
        Self {
            host: Host::new(&limits, settings, fixtures::crypto()).expect("host"),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            up: Queue::with_capacity(MAX_OUT.above),
            io: Queue::with_capacity(MAX_OUT.below),
        }
    }
    fn down(&mut self, request: Request) {
        assert!(self.up.room() >= MAX_OUT.above && self.io.room() >= MAX_OUT.below);
        self.host.down(&self.env, request, &mut self.up, &mut self.io);
        assert!(self.io.is_empty(), "paused owner reads send nothing");
    }
    fn reconcile(&mut self, run: Token, adopted: &[u64]) {
        self.down(Request::Run { run, plan: Plan::Reconcile { forge: 4, adopted: adopted.into() } });
    }
    fn read(&mut self, run: Token, item: Item) {
        assert_eq!(self.up.pop(), Some(Event::Need { run, need: super::Need::Read(item) }));
        assert!(self.up.is_empty());
    }
    fn fire(&mut self, seconds: u64) {
        self.env.now = Time::ZERO.saturating_add(Duration::from_secs(seconds));
        assert!(self.up.room() >= MAX_OUT.above && self.io.room() >= MAX_OUT.below);
        self.host.fire(&self.env, &mut self.up, &mut self.io);
        assert!(self.io.is_empty());
    }
    fn close(&mut self) {
        self.down(Request::Close);
        assert_eq!(self.up.pop(), Some(Event::Closed));
        assert!(self.up.is_empty());
        assert_eq!(self.host.next_deadline(), None);
    }
}

#[test]
fn a_github_reconcile_asks_for_app_id_instead_of_stopping_unexpected() {
    let mut world = ReconcileHost::new();
    world.reconcile(RECONCILE_RUN, &[]);
    world.read(RECONCILE_RUN, Item::AppId);
    world.close();
}

#[test]
fn a_github_reconcile_with_adopted_ids_sends_nothing() {
    let mut world = ReconcileHost::new();
    world.reconcile(RECONCILE_RUN, &[1]);
    assert_eq!(
        world.up.pop(),
        Some(Event::Stop {
            run: RECONCILE_RUN,
            failure: Failure {
                forge: 4,
                step: Stage::App,
                subject: b"reconcile".as_slice().into(),
                why: Why::Unexpected
            },
        })
    );
    assert!(world.up.is_empty());
    assert_eq!(world.host.next_deadline(), None);
    world.close();
}

#[test]
fn a_second_github_writer_is_busy_beside_a_running_picker() {
    let mut world = ReconcileHost::new();
    world.reconcile(RECONCILE_RUN, &[]);
    world.read(RECONCILE_RUN, Item::AppId);
    world.down(Request::Run {
        run: PICKER_RUN,
        plan: Plan::Reachable { forge: 4, installation: Some(2), page: core::num::NonZeroU32::new(1).expect("page") },
    });
    world.read(PICKER_RUN, Item::Accepted);
    world.reconcile(OTHER_WRITER, &[]);
    assert_eq!(
        world.up.pop(),
        Some(Event::Stop {
            run: OTHER_WRITER,
            failure: Failure { forge: 4, step: Stage::App, subject: b"reconcile".as_slice().into(), why: Why::Busy },
        })
    );
    assert!(world.up.is_empty());
    world.close();
}

#[test]
fn a_github_reconcile_uses_its_forges_deadline_and_ends_once() {
    let mut world = ReconcileHost::new();
    world.reconcile(RECONCILE_RUN, &[]);
    world.read(RECONCILE_RUN, Item::AppId);
    assert_eq!(world.host.next_deadline(), Some(Time::ZERO.saturating_add(Duration::from_secs(7))));
    world.fire(6);
    assert!(world.up.is_empty());
    world.fire(7);
    assert_eq!(
        world.up.pop(),
        Some(Event::Stop {
            run: RECONCILE_RUN,
            failure: Failure { forge: 4, step: Stage::App, subject: b"id".as_slice().into(), why: Why::Down },
        })
    );
    assert!(world.up.is_empty());
    assert_eq!(world.host.next_deadline(), None);
    world.down(Request::Answer { run: RECONCILE_RUN, answer: Answer::Read(Some(secret(&19u64.to_le_bytes()))) });
    assert!(world.up.is_empty());
    world.fire(30);
    assert!(world.up.is_empty());
    world.close();
}

#[test]
fn github_app_metadata_must_fit_in_the_hosts_documents() {
    // Fourteen subscriptions; pull_request_review_comment is 27 bytes.
    const INNER: u32 = 14;
    const STRING: u32 = 27;
    for (inner, string_bytes, expected) in [
        (INNER, STRING, None),
        (
            INNER.checked_sub(1).expect("nonzero bound"),
            STRING,
            Some(super::Unusable::GitHubMetadata { inner: INNER, string_bytes: STRING }),
        ),
        (
            INNER,
            STRING.checked_sub(1).expect("nonzero bound"),
            Some(super::Unusable::GitHubMetadata { inner: INNER, string_bytes: STRING }),
        ),
    ] {
        let mut limits = fixtures::limits();
        limits.inner = inner;
        limits.string_bytes = string_bytes;
        assert_eq!(Host::new(&limits, fixtures::github_settings(), fixtures::crypto()).err(), expected);
    }
}
