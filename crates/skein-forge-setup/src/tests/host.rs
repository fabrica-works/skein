use super::fixtures;
use crate::{
    host::{Answer, Event, Forge, Host, Kind, Limits, Need, Plan, Request, Settings, Stacked, Transport, Unusable},
    item::Item,
    reconcile::{Failure, Stage, Why},
};
use skein_lib::{CryptoRng, Duration, Env, List, Queue, Time, Token, Wall};
fn limits() -> Limits {
    Limits {
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
        io: skein_io::Limits {
            sockets: 8,
            names: 0,
            refusals: 2,
            intake: 32768,
            receive: 4096,
            output: 32768,
            sends: 4,
            accepts: 1,
            backlog: 1,
            close_timeout: Duration::from_secs(1),
            retry: Duration::from_millis(10),
        },
    }
}
fn forge(number: u16) -> Forge {
    let mut given = fixtures::given();
    given.forge = number;
    Forge {
        kind: Kind::Forgejo,
        address: skein_io::Addr::Ip(([127, 0, 0, 1], 3000).into()),
        transport: Transport::Plaintext { name: None },
        given,
    }
}
fn settings(numbers: &[u16]) -> Settings {
    let mut forges = List::with_capacity(u32::try_from(numbers.len()).expect("small table"));
    for number in numbers {
        forges.push(forge(*number)).expect("forge count");
    }
    Settings { forges, user_agent: b"kit/0.0.0".as_slice().into() }
}
fn crypto() -> CryptoRng {
    CryptoRng::new(&fixtures::secret(&[42; 32]), 0).expect("32-byte seed")
}
struct World {
    host: Host,
    env: Env<Limits>,
    up: Queue<Event>,
    io: Queue<skein_io::Request>,
}
impl World {
    fn new() -> Self {
        Self::with(limits(), settings(&[1]))
    }
    fn with(limits: Limits, settings: Settings) -> Self {
        Self {
            host: Host::new(&limits, settings, crypto()).expect("host fixtures"),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits },
            up: Queue::with_capacity(4),
            io: Queue::with_capacity(4),
        }
    }
    fn down(&mut self, request: Request) {
        self.host.down(&self.env, request, &mut self.up, &mut self.io);
    }
    fn plan(&mut self, run: u64, plan: Plan) {
        self.down(Request::Run { run: Token::new(run), plan });
    }
    fn run(&mut self, run: u64, forge: u16) {
        self.down(Request::Run { run: Token::new(run), plan: Plan::Reconcile { forge, adopted: Box::new([]) } });
    }
    fn answer(&mut self, run: u64, answer: Answer) {
        self.down(Request::Answer { run: Token::new(run), answer });
    }
    fn event(&mut self, event: skein_io::Event) {
        self.host.up(&self.env, event, &mut self.up, &mut self.io);
    }
    fn fire(&mut self, millis: u64) {
        self.env.now = Time::ZERO.saturating_add(Duration::from_millis(millis));
        self.host.fire(&self.env, &mut self.up, &mut self.io);
    }
    fn read_admin(&mut self) {
        self.run(1, 1);
        assert_eq!(self.up.pop(), Some(Event::Need { run: Token::new(1), need: Need::Read(Item::AdminToken) }));
    }
    fn send(&mut self) {
        self.read_admin();
        self.answer(1, Answer::Read(Some(fixtures::secret(fixtures::ADMIN))));
    }
    fn connect(&mut self) -> Token {
        let Some(skein_io::Request::Connect { owner, addr }) = self.io.pop() else {
            panic!("Connect");
        };
        assert_eq!(addr, skein_io::Addr::Ip(([127, 0, 0, 1], 3000).into()));
        assert_eq!(self.io.len(), 0);
        owner
    }
    fn refused(&mut self) {
        let owner = self.connect();
        self.event(skein_io::Event::Connecting {
            owner,
            socket: Token::new(owner.raw().checked_add(100).expect("small connection count")),
        });
        self.event(skein_io::Event::Failed { owner, error: skein_io::Error::Refused });
        // io closes a failed connect without waiting for this request.
        let _close = self.io.pop();
        self.event(skein_io::Event::Closed { owner });
        assert_eq!(self.up.len(), 0);
    }
}
fn stop(run: u64, forge: u16, why: Why) -> Event {
    Event::Stop {
        run: Token::new(run),
        failure: Failure { forge, step: Stage::Admin, subject: b"kit-admin".as_slice().into(), why },
    }
}
#[test]
fn a_runs_given_takes_the_plans_adopted_and_the_rest_from_the_forge() {
    let mut forge = fixtures::given();
    forge.adopted = Box::new([99]);
    forge.orgs = Box::new([b"acme".as_slice().into()]);
    forge.org_owner = Some(b"ada".as_slice().into());
    forge.listener = Some(([127, 0, 0, 1], 9898).into());
    let plan = [7, 3];
    let given = crate::host::run::given_for(&forge, &plan);
    let mut expected = forge.clone();
    expected.adopted = plan.into();
    assert_eq!(given, expected);
    assert_eq!(forge.adopted.as_ref(), &[99]);
    assert!(crate::host::run::given_for(&forge, &[]).adopted.is_empty());
}
#[test]
fn settings_with_too_many_adopted_repositories_are_refused() {
    use crate::given::{Place, Refused};
    let mut s = settings(&[1]);
    let given = &mut s.forges.get_mut(0).unwrap().given;
    given.limits.repositories = 1;
    given.adopted = Box::new([1, 2]);
    assert_eq!(
        Host::new(&limits(), s, crypto()).err(),
        Some(Unusable::Given { forge: 1, refused: Refused { place: Place::Repositories, other: None } })
    );
}
#[test]
fn a_run_without_an_admin_token_stops_before_any_connection() {
    let mut w = World::new();
    w.read_admin();
    assert_eq!(w.io.len(), 0);
    w.answer(1, Answer::Read(None));
    assert_eq!(w.up.pop(), Some(stop(1, 1, Why::Missing)));
    assert_eq!(w.io.len(), 0);
}
#[test]
fn an_admin_token_that_breaks_its_item_is_absent() {
    let mut w = World::new();
    w.read_admin();
    w.answer(1, Answer::Read(Some(fixtures::secret(b"invalid token"))));
    assert_eq!(w.up.pop(), Some(stop(1, 1, Why::Missing)));
    assert_eq!(w.io.len(), 0);
}
#[test]
fn a_second_run_for_a_forge_with_one_live_stops_busy() {
    let mut w = World::new();
    w.read_admin();
    w.run(2, 1);
    assert_eq!(w.up.pop(), Some(stop(2, 1, Why::Busy)));
    w.answer(1, Answer::Read(None));
    assert_eq!(w.up.pop(), Some(stop(1, 1, Why::Missing)));
    w.run(3, 1);
    assert_eq!(w.up.pop(), Some(Event::Need { run: Token::new(3), need: Need::Read(Item::AdminToken) }));
}
#[test]
fn a_run_past_runs_stops_busy() {
    let mut l = limits();
    l.runs = 1;
    let mut w = World::with(l, settings(&[1, 2]));
    w.read_admin();
    w.run(2, 2);
    assert_eq!(w.up.pop(), Some(stop(2, 2, Why::Busy)));
}
#[test]
fn an_answer_with_no_need_waiting_is_ignored() {
    let mut w = World::new();
    w.answer(99, Answer::Written);
    assert_eq!(w.up.len(), 0);
    assert_eq!(w.io.len(), 0);
    w.send();
    let _owner = w.connect();
    w.answer(1, Answer::Written);
    assert_eq!(w.up.len(), 0);
    assert_eq!(w.io.len(), 0);
}
#[test]
fn a_run_waiting_on_the_owner_at_its_deadline_stops_down_at_its_step() {
    let mut w = World::new();
    w.read_admin();
    assert_eq!(w.host.next_deadline(), Some(Time::ZERO.saturating_add(Duration::from_secs(120))));
    w.fire(120_000);
    assert_eq!(w.up.pop(), Some(stop(1, 1, Why::Down)));
    w.answer(1, Answer::Read(None));
    assert_eq!(w.up.len(), 0);
    assert_eq!(w.host.next_deadline(), None);
}
#[test]
fn the_first_try_waits_double_from_backoff_to_backoff_max() {
    let mut w = World::new();
    w.send();
    for at in [0, 100, 300, 700, 1500, 3100, 6300, 11300, 16300] {
        if at != 0 {
            w.fire(at - 1);
            assert_eq!(w.io.len(), 0);
            w.fire(at);
        }
        w.refused();
    }
}
#[test]
fn the_deadline_cuts_the_last_wait_and_stops_the_run_down() {
    let mut s = settings(&[1]);
    let given = &mut s.forges.get_mut(0).unwrap().given;
    given.limits.deadline = Duration::from_secs(1);
    given.limits.backoff_max = Duration::from_millis(400);
    let mut w = World::with(limits(), s);
    w.send();
    for at in [0, 100, 300, 700] {
        w.fire(at);
        w.refused();
    }
    assert_eq!(w.host.next_deadline(), Some(Time::ZERO.saturating_add(Duration::from_secs(1))));
    w.fire(1000);
    assert_eq!(w.up.pop(), Some(stop(1, 1, Why::Down)));
    assert_eq!(w.io.len(), 0);
    assert_eq!(w.host.next_deadline(), None);
}
#[test]
fn close_says_closed_only_once_io_has_closed_every_connection() {
    let mut w = World::new();
    w.send();
    let owner = w.connect();
    w.down(Request::Close);
    assert_eq!(w.up.len(), 0);
    assert_eq!(w.io.len(), 0);
    let socket = Token::new(100);
    w.event(skein_io::Event::Connecting { owner, socket });
    assert_eq!(w.io.pop(), Some(skein_io::Request::Abort { entity: socket }));
    assert_eq!(w.up.len(), 0);
    w.event(skein_io::Event::Closed { owner });
    assert_eq!(w.up.pop(), Some(Event::Closed));
    w.event(skein_io::Event::Closed { owner });
    w.run(2, 1);
    assert_eq!(w.up.len(), 0);
    assert_eq!(w.io.len(), 0);
    assert_eq!(w.host.next_deadline(), None);
}
#[test]
#[expect(clippy::disallowed_macros, reason = "the redaction test inspects the host's Debug rendering")]
fn the_hosts_debug_shows_no_token_in_flight() {
    let mut w = World::new();
    w.send();
    let rendered = format!("{:?}", w.host);
    for bytes in rendered.as_bytes().windows(fixtures::ADMIN.len()) {
        assert_ne!(bytes, fixtures::ADMIN, "credentials stay out of Debug");
    }
}
#[test]
fn the_host_refuses_unusable_settings_naming_each() {
    use crate::given::{Place, Refused};
    use skein_forge::forgejo::limits::Problem;
    let mut cases = List::with_capacity(10);
    let mut l = limits();
    l.runs = 0;
    cases.push((l, settings(&[1]), Unusable::Zero)).unwrap();
    let mut l = limits();
    l.token_bytes = 63;
    cases.push((l, settings(&[1]), Unusable::TokenBytes { token_bytes: 63 })).unwrap();
    let mut l = limits();
    l.depth = 7;
    cases.push((l, settings(&[1]), Unusable::Documents(Problem::Depth))).unwrap();
    let mut l = limits();
    l.io.intake = 16;
    cases.push((l, settings(&[1]), Unusable::Stack { stacked: Stacked::HttpOverIo, demand: 4096, cap: 16 })).unwrap();
    cases.push((limits(), settings(&[1, 2, 3]), Unusable::Forges { forges: 2 })).unwrap();
    cases.push((limits(), settings(&[1, 1]), Unusable::Duplicate { forge: 1 })).unwrap();
    let mut s = settings(&[1]);
    s.forges.get_mut(0).unwrap().address = skein_io::Addr::Ip(([192, 0, 2, 1], 3000).into());
    cases.push((limits(), s, Unusable::Address { forge: 1 })).unwrap();
    let mut s = settings(&[1]);
    s.forges.get_mut(0).unwrap().transport = Transport::Plaintext { name: Some(b"".as_slice().into()) };
    cases.push((limits(), s, Unusable::PlaintextName { forge: 1 })).unwrap();
    let mut s = settings(&[1]);
    s.user_agent = b"kit\r\n".as_slice().into();
    cases.push((limits(), s, Unusable::Request { forge: 1 })).unwrap();
    let mut s = settings(&[1]);
    s.forges.get_mut(0).unwrap().given.bot_login = b"".as_slice().into();
    cases
        .push((limits(), s, Unusable::Given { forge: 1, refused: Refused { place: Place::BotLogin, other: None } }))
        .unwrap();
    for (l, s, expected) in cases.into_boxed() {
        assert_eq!(Host::new(&l, s, crypto()).err(), Some(expected));
    }
}

#[test]
fn a_connect_without_a_socket_retries_from_its_closed_time() {
    let mut w = World::new();
    w.send();
    let owner = w.connect();
    w.event(skein_io::Event::Failed { owner, error: skein_io::Error::Busy });
    assert_eq!(w.io.len(), 0);
    w.fire(200);
    assert_eq!(w.io.len(), 0);
    w.event(skein_io::Event::Closed { owner });
    assert_eq!(w.host.next_deadline(), Some(Time::ZERO.saturating_add(Duration::from_millis(300))));
    w.fire(299);
    assert_eq!(w.io.len(), 0);
    w.fire(300);
    let next = w.connect();
    assert_ne!(owner, next);
    assert_eq!(w.up.len(), 0);
}
#[test]
fn a_wrong_answer_kind_stops_the_waiting_plan_unexpected() {
    let mut w = World::new();
    w.read_admin();
    w.answer(1, Answer::Written);
    assert_eq!(w.up.pop(), Some(stop(1, 1, Why::Unexpected)));
    assert_eq!(w.io.len(), 0);
}
#[test]
fn an_empty_host_closes_once_and_ignores_later_requests() {
    let mut w = World::new();
    w.down(Request::Close);
    assert_eq!(w.up.pop(), Some(Event::Closed));
    w.down(Request::Close);
    w.run(1, 1);
    w.answer(1, Answer::Written);
    w.fire(120_000);
    assert_eq!(w.up.len(), 0);
    assert_eq!(w.io.len(), 0);
    assert_eq!(w.host.next_deadline(), None);
}
#[test]
fn deadlines_past_the_output_room_resume_at_now_until_every_run_stops() {
    let mut l = limits();
    l.runs = 10;
    l.forges = 10;
    let mut w = World::with(l, settings(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]));
    for number in 1_u16..=10 {
        w.run(u64::from(number), number);
        assert_eq!(
            w.up.pop(),
            Some(Event::Need { run: Token::new(u64::from(number)), need: Need::Read(Item::AdminToken) })
        );
    }
    let now = Time::ZERO.saturating_add(Duration::from_secs(120));
    let mut run = 1_u64;
    for count in [4_u32, 4, 2] {
        w.fire(120_000);
        assert_eq!(w.up.len(), count);
        for _ in 0..count {
            assert_eq!(w.up.pop(), Some(stop(run, u16::try_from(run).unwrap(), Why::Down)));
            run = run.checked_add(1).unwrap();
        }
        assert_eq!(w.host.next_deadline(), if run <= 10 { Some(now) } else { None });
    }
    assert_eq!(w.io.len(), 0);
}

fn check() -> Plan {
    Plan::Check { forge: 1 }
}
fn take_over(token: &[u8]) -> Plan {
    Plan::TakeOver { forge: 1, admin_token: fixtures::secret(token) }
}
fn adopt() -> Plan {
    Plan::Adopt { forge: 1, repository: core::num::NonZeroU64::new(7).unwrap() }
}
fn token_stop(why: Why) -> Event {
    Event::Stop {
        run: Token::new(1),
        failure: Failure { forge: 1, step: Stage::Token, subject: b"api".as_slice().into(), why },
    }
}
fn read(w: &mut World, item: Item) {
    assert_eq!(w.up.pop(), Some(Event::Need { run: Token::new(1), need: Need::Read(item) }));
    assert!(w.up.is_empty());
    assert!(w.io.is_empty());
}
#[test]
fn a_check_run_asks_for_the_api_token_and_a_missing_one_stops_naming_api() {
    let mut w = World::new();
    w.plan(1, check());
    read(&mut w, Item::ApiToken);
    w.answer(1, Answer::Read(None));
    assert_eq!(w.up.pop(), Some(token_stop(Why::Missing)));
    w.plan(2, check());
    assert_eq!(w.up.pop(), Some(Event::Need { run: Token::new(2), need: Need::Read(Item::ApiToken) }));
}
#[test]
fn a_check_run_waiting_on_the_owner_at_its_deadline_stops_down_naming_its_token() {
    let mut w = World::new();
    w.plan(1, check());
    read(&mut w, Item::ApiToken);
    assert_eq!(w.host.next_deadline(), Some(Time::ZERO.saturating_add(Duration::from_secs(120))));
    w.fire(120_000);
    assert_eq!(w.up.pop(), Some(token_stop(Why::Down)));
    assert_eq!(w.host.next_deadline(), None);
}
fn silent_deadline(w: &mut World, expected: Event) {
    let owner = w.connect();
    let socket = Token::new(100);
    w.event(skein_io::Event::Connecting { owner, socket });
    assert!(w.io.is_empty());
    w.fire(120_000);
    assert_eq!(w.up.pop(), Some(expected));
    assert_eq!(w.io.pop(), Some(skein_io::Request::Abort { entity: socket }));
    assert!(w.io.is_empty());
    assert!(w.up.is_empty());
    w.event(skein_io::Event::Closed { owner });
    assert_eq!(w.host.next_deadline(), None);
}
#[test]
fn a_check_whose_forge_never_answers_stops_down_naming_the_token_in_flight() {
    let mut w = World::new();
    w.plan(1, check());
    read(&mut w, Item::ApiToken);
    w.answer(1, Answer::Read(Some(fixtures::secret(b"api-token"))));
    silent_deadline(&mut w, token_stop(Why::Down));
}
#[test]
fn a_take_over_run_sends_the_handed_token_and_asks_the_owner_for_nothing() {
    let mut w = World::new();
    w.plan(1, take_over(&[b't'; 40]));
    assert!(w.up.is_empty());
    w.connect();
}
#[test]
fn a_take_over_token_that_is_empty_or_breaks_its_item_stops_missing_at_once() {
    for token in [b"".as_slice(), b"has a space", &[b't'; 65]] {
        let mut w = World::new();
        w.plan(1, take_over(token));
        assert_eq!(w.up.pop(), Some(stop(1, 1, Why::Missing)));
        assert!(w.io.is_empty());
        assert_eq!(w.host.next_deadline(), None);
        w.fire(1000);
        w.run(2, 1);
        assert_eq!(w.up.pop(), Some(Event::Need { run: Token::new(2), need: Need::Read(Item::AdminToken) }));
        assert_eq!(w.host.next_deadline(), Some(Time::ZERO.saturating_add(Duration::from_secs(121))));
    }
}
#[test]
fn a_take_over_token_of_seven_bytes_connects() {
    let mut w = World::new();
    w.plan(1, take_over(b"1234567"));
    assert!(w.up.is_empty());
    w.connect();
}
#[test]
fn a_take_over_run_whose_forge_never_answers_stops_down_at_its_step() {
    let mut w = World::new();
    w.plan(1, take_over(fixtures::TAKEN));
    silent_deadline(&mut w, stop(1, 1, Why::Down));
}
#[test]
#[expect(clippy::disallowed_macros, reason = "the redaction test inspects request and host Debug rendering")]
fn no_taken_token_is_in_a_requests_or_a_hosts_debug() {
    let mut w = World::new();
    let request = Request::Run { run: Token::new(1), plan: take_over(fixtures::TAKEN) };
    let rendered = format!("{request:?}");
    w.down(request);
    for debug in [rendered, format!("{:?}", w.host)] {
        for fragment in [fixtures::TAKEN, fixtures::TAKEN_SUFFIX] {
            for bytes in debug.as_bytes().windows(fragment.len()) {
                assert_ne!(bytes, fragment);
            }
        }
    }
}
#[test]
fn an_adopt_run_asks_for_the_admin_token_and_a_missing_one_stops_naming_the_admin() {
    let mut w = World::new();
    w.plan(1, adopt());
    read(&mut w, Item::AdminToken);
    w.answer(1, Answer::Read(None));
    assert_eq!(w.up.pop(), Some(stop(1, 1, Why::Missing)));
    assert_eq!(w.host.next_deadline(), None);
}
#[test]
fn an_adopt_run_waiting_on_the_owner_at_its_deadline_stops_down_at_its_step() {
    let mut w = World::new();
    w.plan(1, adopt());
    read(&mut w, Item::AdminToken);
    assert_eq!(w.host.next_deadline(), Some(Time::ZERO.saturating_add(Duration::from_secs(120))));
    w.fire(120_000);
    assert_eq!(w.up.pop(), Some(stop(1, 1, Why::Down)));
}
fn plans() -> [Plan; 4] {
    [Plan::Reconcile { forge: 1, adopted: Box::new([]) }, check(), take_over(fixtures::TAKEN), adopt()]
}
#[test]
fn a_second_run_of_any_plan_for_a_forge_with_one_live_stops_busy() {
    for first in plans() {
        for second in plans() {
            let mut w = World::new();
            w.plan(1, first.clone());
            let connection = match first {
                Plan::Reconcile { .. } | Plan::Check { .. } | Plan::Adopt { .. } => {
                    assert!(w.up.pop().is_some());
                    None
                }
                Plan::TakeOver { .. } => Some(w.connect()),
            };
            w.plan(2, second);
            assert_eq!(w.up.pop(), Some(stop(2, 1, Why::Busy)));
            assert!(w.up.is_empty());
            if let Some(owner) = connection {
                use skein_lib::stream::{Down, Up};
                let socket = Token::new(100);
                w.event(skein_io::Event::Connecting { owner, socket });
                w.event(skein_io::Event::Connected { owner });
                let Some(skein_io::Request::Stream { down: Down::Demand { .. }, .. }) = w.io.pop() else {
                    panic!("stream demand");
                };
                w.event(skein_io::Event::Stream { owner, up: Up::Room });
                let Some(skein_io::Request::Stream { down: Down::Send(_), .. }) = w.io.pop() else {
                    panic!("request head");
                };
                for line in [b"HTTP/1.1 401 Unauthorized\r\n".as_slice(), b"Content-Length: 0\r\n", b"\r\n"] {
                    let Some(skein_io::Request::Stream { down: Down::Demand { .. }, .. }) = w.io.pop() else {
                        panic!("stream demand");
                    };
                    w.event(skein_io::Event::Stream { owner, up: Up::Bytes(line.into()) });
                }
                assert_eq!(w.up.pop(), Some(stop(1, 1, Why::Refused(401))));
            } else {
                w.answer(1, Answer::Read(None));
                let expected = match first {
                    Plan::Check { .. } => token_stop(Why::Missing),
                    Plan::Reconcile { .. } | Plan::Adopt { .. } | Plan::TakeOver { .. } => stop(1, 1, Why::Missing),
                };
                assert_eq!(w.up.pop(), Some(expected));
            }
        }
    }
}
