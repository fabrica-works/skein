//! Domain fixtures and piece transitions.
use crate::Config;
use crate::api::{Finish, Line, Message, Part, Query, Role, Script, ToolSpec, Turn};
use alloc::boxed::Box;
use skein_lib::Duration;
use skein_lib::bytes::copy_of;

pub(super) const CONFIG: Config = Config {
    calls: 4,
    query_bytes: 1 << 20,
    script_bytes: 1 << 20,
    answer_bytes: 1 << 20,
    latency_min: Duration::ZERO,
    latency_max: Duration::ZERO,
    overloaded: 0,
    rate_limited: 0,
    retry_after: Duration::ZERO,
    unavailable: 0,
    too_long: 0,
    unauthorized: 0,
    refused: 0,
    no_calls: 0,
    answer_tokens: 1,
    calls_per_answer: 1,
    malformed: 0,
    outside_choice: 0,
    tool_rounds: 1,
    cache_lifetime: Duration::from_secs(300),
    cache_entries: 8,
    unscoped_reads: 0,
};

pub(super) fn user(parts: Box<[Part]>) -> Message {
    Message { role: Role::User, parts }
}

pub(super) fn query(messages: Box<[Message]>) -> Query {
    let tool = ToolSpec { name: copy_of(b"ls"), description: copy_of(b""), parameters: copy_of(b"{}") };
    Query {
        credential: None,
        caching: crate::api::Caching::Unscoped,
        model: copy_of(b"fake"),
        system: copy_of(b""),
        tools: Box::new([tool]),
        messages,
        max_tokens: 100,
        choice: crate::api::ToolChoice::Auto,
    }
}

pub(super) fn script(cue: &[u8], turns: Box<[Turn]>) -> Script {
    Script { cue: copy_of(cue), turns }
}

pub(super) fn says(text: &[u8]) -> Turn {
    Turn { lines: Box::new([Line::Text { text: copy_of(text) }]), finish: Finish::Stop, tokens: 2 }
}

use crate::api::Piece;
use crate::{Domain, Event, MAX_OUT, Request, fire, step};
use skein_lib::{Env, Queue, ReplyTo, Time, Token, Wall};

const CALL: Token = Token::new(7);
const OTHER_CALL: Token = Token::new(8);
const MAX_HOLD_BYTES: usize = 64; // fake-llm.md 2.2: admitted hold names.
const PAST_HOLD_BYTES: usize = MAX_HOLD_BYTES + 1;
const INVALID_SCRIPT_CASES: usize = 5; // Placement, count, text, empty hold and long hold.
const LATENCY: Duration = Duration::from_secs(1);
const STREAM_CONFIG: Config = Config { latency_min: LATENCY, latency_max: LATENCY, ..CONFIG };

fn piece(text: &[u8], hold: Option<&[u8]>) -> Piece {
    let text = copy_of(text);
    let Some(name) = hold else {
        return Piece { text, hold: None };
    };
    Piece { text, hold: Some(copy_of(name)) }
}

fn pieces_turn(pieces: Box<[Piece]>) -> Turn {
    Turn { lines: Box::new([Line::Pieces { pieces }]), finish: Finish::Stop, tokens: 2 }
}

fn request() -> Query {
    let mut query = query(Box::new([user(Box::new([Part::Text { text: copy_of(b"hi") }]))]));
    query.system = copy_of(b"@main");
    query
}

struct Playback {
    domain: Domain,
    env: Env<Config>,
    out: Queue<Request>,
}
impl Playback {
    fn new(config: Config, pieces: Box<[Piece]>) -> Self {
        Self::from_query(config, pieces, request())
    }
    fn from_query(config: Config, pieces: Box<[Piece]>, query: Query) -> Self {
        Self::from_turn(config, pieces_turn(pieces), query)
    }
    fn from_turn(config: Config, turn: Turn, query: Query) -> Self {
        let domain = Domain::scripted(&config, 1, Box::new([script(b"@main", Box::new([turn]))]));
        let mut playback = Self {
            domain,
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: config },
            out: Queue::with_capacity(MAX_OUT),
        };
        playback.call(CALL, query);
        playback
    }
    fn call(&mut self, call: Token, query: Query) {
        step(&mut self.domain, &self.env, Event::Call { reply_to: ReplyTo::new(call), call, query }, &mut self.out);
    }
    fn fire(&mut self, seconds: u64) -> Option<Request> {
        self.env.now = Time::ZERO.saturating_add(Duration::from_secs(seconds));
        fire(&mut self.domain, &self.env, &mut self.out);
        self.out.pop()
    }
    fn release(&mut self, name: &[u8]) {
        step(&mut self.domain, &self.env, Event::Release { name: copy_of(name) }, &mut self.out);
    }
}

#[test]
fn a_held_piece_waits_with_no_timer_until_released() {
    let mut play = Playback::new(STREAM_CONFIG, Box::new([piece(b"Hel", None), piece(b"lo", Some(b"h"))]));
    play.fire(1);
    let held = play.fire(2);
    let Some(Request::Hold { call, .. }) = held else { panic!("the held piece emits a hold") };
    assert_eq!(call, CALL);
    assert_eq!(play.domain.next_deadline(), None, "a held call has no timer");
    play.release(b"h");
    assert!(play.out.is_empty(), "a release sends nothing");
    let Some(Request::Reply { result: Ok(answer), .. }) = play.fire(3) else { panic!("the reply ends the call") };
    assert_eq!(&*answer.parts, &[Part::Text { text: copy_of(b"Hello") }]);
}

#[test]
fn each_piece_is_sent_at_its_own_latency() {
    let mut play = Playback::new(STREAM_CONFIG, Box::new([piece(b"a", None), piece(b"b", None), piece(b"c", None)]));
    for (seconds, text) in [(1, b"a"), (2, b"b")] {
        assert_eq!(play.domain.next_deadline(), Some(Time::ZERO.saturating_add(Duration::from_secs(seconds))));
        let Some(Request::Piece { call, text: sent, .. }) = play.fire(seconds) else {
            panic!("a text piece at its latency")
        };
        assert_eq!((call, sent.as_ref()), (CALL, text.as_slice()));
    }
    let Some(Request::Reply { result: Ok(answer), .. }) = play.fire(3) else { panic!("the last piece ends the call") };
    assert_eq!(&*answer.parts, &[Part::Text { text: copy_of(b"abc") }]);
}

#[test]
fn a_release_before_its_hold_changes_nothing() {
    let mut play = Playback::new(STREAM_CONFIG, Box::new([piece(b"a", Some(b"h"))]));
    play.release(b"h");
    play.fire(1);
    assert_eq!(play.domain.next_deadline(), None);
    assert_eq!(play.fire(2), None);
}

#[test]
fn one_release_frees_every_call_held_at_its_name() {
    let mut play = Playback::new(STREAM_CONFIG, Box::new([piece(b"a", Some(b"h"))]));
    play.call(OTHER_CALL, request());
    play.fire(1);
    play.fire(1);
    play.release(b"h");
    let mut calls = [None, None];
    for call in &mut calls {
        let Some(Request::Reply { to, .. }) = play.fire(2) else { panic!("both held calls resume") };
        *call = Some(to.into_token());
    }
    calls.sort();
    assert_eq!(calls, [Some(CALL), Some(OTHER_CALL)]);
}

#[test]
fn a_faulted_reply_in_pieces_goes_whole_in_one_reply() {
    let mut play = Playback::new(Config { overloaded: 1000, ..STREAM_CONFIG }, Box::new([piece(b"a", Some(b"h"))]));
    let Some(Request::Reply { result, .. }) = play.fire(1) else { panic!("a fault is a terminal") };
    assert_eq!(result, Err(crate::api::Error::Overloaded.into()));
}

#[test]
fn a_reply_cut_by_max_tokens_goes_whole() {
    let mut short = request();
    short.max_tokens = 1; // The scripted turn takes two tokens.
    let mut play = Playback::from_query(STREAM_CONFIG, Box::new([piece(b"abc", Some(b"h"))]), short);
    let Some(Request::Reply { result: Ok(answer), .. }) = play.fire(1) else { panic!("the cut answer goes whole") };
    assert_eq!(answer.finish, Finish::Length);
    assert_eq!(&*answer.parts, &[Part::Text { text: copy_of(b"do") }]);
}

#[test]
fn a_script_breaking_the_pieces_rule_is_refused() {
    let cases: [(&str, Box<[Line]>); INVALID_SCRIPT_CASES] = [
        (
            "second",
            Box::new([Line::Text { text: copy_of(b"a") }, Line::Pieces { pieces: Box::new([piece(b"b", None)]) }]),
        ),
        ("empty list", Box::new([Line::Pieces { pieces: Box::new([]) }])),
        ("empty text", Box::new([Line::Pieces { pieces: Box::new([piece(b"", None)]) }])),
        ("empty hold", Box::new([Line::Pieces { pieces: Box::new([piece(b"a", Some(b""))]) }])),
        ("long hold", Box::new([Line::Pieces { pieces: Box::new([piece(b"a", Some(&[b'h'; PAST_HOLD_BYTES]))]) }])),
    ];
    for (label, lines) in cases {
        let turn = Turn { lines, finish: Finish::Stop, tokens: 2 };
        let result = Domain::try_scripted(&CONFIG, 1, Box::new([script(b"@main", Box::new([turn]))]));
        assert_eq!(result.err(), Some(crate::api::Error::InvalidRequest), "{label}");
    }
}

#[test]
fn a_hold_name_at_64_bytes_is_admitted() {
    let turn = pieces_turn(Box::new([piece(b"a", Some(&[b'h'; MAX_HOLD_BYTES]))]));
    Domain::try_scripted(&CONFIG, 1, Box::new([script(b"@main", Box::new([turn]))]))
        .expect("a hold at the bound is admitted");
}

#[test]
fn a_line_after_a_failing_line_naming_none_is_refused() {
    let fail =
        Line::Fail { error: crate::api::Error::Refused, report: crate::api::Report::none(), credentials: Box::new([]) };
    let turn = Turn { lines: Box::new([fail, Line::Text { text: copy_of(b"later") }]), ..says(b"") };
    let got = Domain::try_scripted(&CONFIG, 1, Box::new([script(b"@main", Box::new([turn]))]));
    assert_eq!(got.err(), Some(crate::api::Error::InvalidRequest));
}

#[test]
fn a_failing_lines_bytes_count_against_the_script_cap() {
    use crate::api::{Error, Report};
    // Stored wrappers, the cue, the three report values, and two u32 indices.
    const CAP: usize = size_of::<Script>()
        + size_of::<Turn>()
        + size_of::<Line>()
        + b"@main".len()
        + b"spent".len()
        + b"req_42".len()
        + b"top_p".len()
        + 2 * size_of::<u32>();
    let scripts = Box::new([script(
        b"@main",
        Box::new([Turn {
            lines: Box::new([Line::Fail {
                error: Error::Refused,
                report: Report { code: copy_of(b"spent"), request: copy_of(b"req_42"), field: copy_of(b"top_p") },
                credentials: Box::new([0, 1]),
            }]),
            ..says(b"")
        }]),
    )]);
    for (cap, expected) in [(CAP, None), (CAP - 1, Some(Error::ContextTooLong))] {
        let got = Domain::try_scripted(
            &Config { script_bytes: u32::try_from(cap).expect("fixture fits"), ..CONFIG },
            1,
            scripts.clone(),
        );
        assert_eq!(got.err(), expected, "{cap}");
    }
}

#[test]
fn a_failing_line_after_pieces_ends_the_walk_with_its_failure() {
    let failure = crate::api::Failure {
        error: crate::api::Error::Refused,
        report: crate::api::Report::none(),
        parts: Box::new([Part::Text { text: copy_of(b"before") }]),
    };
    let turn = Turn {
        lines: Box::new([
            Line::Pieces { pieces: Box::new([piece(b"be", None), piece(b"fore", Some(b"h"))]) },
            Line::Fail { error: failure.error, report: failure.report.clone(), credentials: Box::new([]) },
        ]),
        finish: Finish::Stop,
        tokens: 2,
    };
    let mut play = Playback::from_turn(STREAM_CONFIG, turn, request());
    let first = match play.fire(1) {
        Some(Request::Piece { text, .. }) => Some(text),
        Some(Request::Reply { .. } | Request::Hold { .. }) | None => None,
    };
    assert_eq!(first.as_deref(), Some(b"be".as_slice()));
    let held = match play.fire(2) {
        Some(Request::Hold { call, .. }) => Some(call),
        Some(Request::Piece { .. } | Request::Reply { .. }) | None => None,
    };
    assert_eq!(held, Some(CALL));
    assert_eq!(play.fire(3), None);
    play.release(b"h");
    assert_eq!(play.fire(4), Some(Request::Reply { to: ReplyTo::new(CALL), result: Err(failure) }));
}
