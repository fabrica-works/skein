use super::{fixtures::secret, github_fixtures as fixtures};
use crate::{
    github::{
        given::{Given, Place, Refused},
        manifest::{self, Manifest, Target},
    },
    reconcile::{self, Event, Failure, Need, Outcome, Reply, Stage, Why},
};
use skein_json::{Token, tokenizer};
use skein_lib::{
    Env, Intake, List, Queue, Time, Wall, bytes,
    stream::{Down, Up},
};

const STATE: &[u8] = b"0123456789abcdef0123456789abcdef";
const RANDOM: &[u8] = &[0x00, 0x8a, 0xff];
const SECRET: &[u8] = b"hunter2-manifest-secret";
const NAME_BYTES: u32 = 10;
const PERSONAL_PAGE_BYTES: u32 = 1409;
const ORG_PAGE_BYTES: u32 = 1428;

#[expect(clippy::disallowed_macros, reason = "test checks Debug against secret byte spellings")]
fn diagnostic(text: &str) {
    assert!(bytes::find(text.as_bytes(), SECRET).is_none());
    for spelling in [format!("{SECRET:?}"), format!("{SECRET:x?}"), format!("{SECRET:X?}")] {
        assert!(!text.contains(&spelling));
    }
}
#[expect(clippy::disallowed_macros, reason = "test checks machine, event and need Debug for secret bytes")]
fn go(machine: &mut Manifest, event: Event) -> Need {
    diagnostic(&format!("{machine:?} {event:?}"));
    let mut out = Queue::with_capacity(reconcile::MAX_OUT);
    manifest::step(machine, event, &mut out);
    assert_eq!(out.len(), 1);
    let need = out.pop().expect("one need");
    diagnostic(&format!("{machine:?} {need:?}"));
    need
}
#[expect(clippy::disallowed_macros, reason = "test checks ended machine Debug for secret bytes")]
fn silent(machine: &mut Manifest) {
    for event in events() {
        let mut out = Queue::with_capacity(reconcile::MAX_OUT);
        manifest::step(machine, event, &mut out);
        assert!(out.is_empty());
        diagnostic(&format!("{machine:?}"));
    }
}
fn events() -> [Event; 6] {
    [
        Event::Begin,
        Event::Read(None),
        Event::Read(Some(secret(SECRET))),
        Event::Reply(Reply::Down),
        Event::Written,
        Event::Drawn(secret(SECRET)),
    ]
}
fn make(given: &Given, target: &Target, random: &[u8]) -> Need {
    let mut machine = Manifest::new(given, target, STATE).expect("checked inputs");
    assert_eq!(go(&mut machine, Event::Begin), Need::Draw(3));
    let need = go(&mut machine, Event::Drawn(secret(random)));
    silent(&mut machine);
    need
}
fn page(given: &Given, target: &Target) -> Box<[u8]> {
    match make(given, target, RANDOM) {
        Need::Done(Outcome::Manifest { state, page }) => {
            assert_eq!(state.as_ref(), STATE);
            page
        }
        Need::Send { .. }
        | Need::Read(_)
        | Need::Write { .. }
        | Need::Draw(_)
        | Need::Stop(_)
        | Need::Done(
            Outcome::Nothing | Outcome::Identity { .. } | Outcome::Reachable { .. } | Outcome::Lacking { .. },
        ) => {
            panic!("manifest outcome")
        }
    }
}
fn stopped() -> Need {
    Need::Stop(Failure { forge: 4, step: Stage::App, subject: b"manifest".as_slice().into(), why: Why::Unexpected })
}

// Read the attribute independently, decoding exactly one layer of HTML entities.
#[expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "test independently decodes bounded HTML attributes as text"
)]
fn attribute(page: &[u8], marker: &[u8]) -> Box<[u8]> {
    let start = bytes::find(page, marker).expect("attribute marker").checked_add(marker.len()).expect("page offset");
    let rest = page.get(start..).expect("after marker");
    let end = bytes::find(rest, b"\"").expect("closing attribute quote");
    let text = core::str::from_utf8(rest.get(..end).expect("attribute slice")).expect("ASCII HTML");
    let mut decoded = String::new();
    for (at, part) in text.split('&').enumerate() {
        if at == 0 {
            decoded.push_str(part);
            continue;
        }
        let (entity, tail) = part.split_once(';').expect("whole HTML entity");
        decoded.push(match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "#39" => '\'',
            _ => panic!("unknown HTML entity"),
        });
        decoded.push_str(tail);
    }
    decoded.into_bytes().into_boxed_slice()
}
fn tokens(input: &[u8]) -> List<Token> {
    let bound = u32::try_from(input.len()).expect("small document");
    let limits = tokenizer::Limits { depth: 4, string: bound, number: 20, chunk: bound, length: bound };
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits };
    let mut machine = tokenizer::Tokenizer::new(&limits);
    let mut above = Queue::with_capacity(1);
    let mut below = Queue::with_capacity(1);
    let mut intake = Intake::with_capacity(bound.max(tokenizer::largest_demand(&limits)));
    intake.append(input).expect("input capacity");
    let mut tokens = List::with_capacity(bound);
    tokenizer::down(&mut machine, &env, tokenizer::Request::Next, &mut above, &mut below);
    for _tick in 0..input.len().saturating_mul(8).saturating_add(64) {
        match above.pop() {
            Some(tokenizer::Event::Token(token)) => {
                tokens.push(token).expect("token capacity");
                tokenizer::down(&mut machine, &env, tokenizer::Request::Next, &mut above, &mut below);
            }
            Some(tokenizer::Event::Done) => return tokens,
            Some(
                tokenizer::Event::Failed(_)
                | tokenizer::Event::Long(_)
                | tokenizer::Event::Skipped(_)
                | tokenizer::Event::Closed,
            ) => panic!("valid manifest JSON"),
            None => {
                let Some(Down::Demand { read, room: 0 }) = below.pop() else { panic!("JSON read demand") };
                let delivery = match intake.meet(read) {
                    Some(bytes) => Up::Bytes(bytes),
                    None => Up::End,
                };
                tokenizer::up(&mut machine, &env, delivery, &mut above, &mut below);
            }
        }
    }
    panic!("bounded JSON decode")
}
fn key(value: &[u8]) -> Token {
    Token::Key(value.into())
}
fn string(value: &[u8]) -> Token {
    Token::String(value.into())
}
fn wanted(given: &Given) -> List<Token> {
    let mut wanted = List::with_capacity(100);
    for token in [
        Token::ObjectStart,
        key(b"name"),
        string(b"kit-008aff"),
        key(b"url"),
        string(&given.url),
        key(b"public"),
        Token::True,
        key(b"redirect_url"),
        string(&given.redirect_url),
        key(b"setup_url"),
        string(&given.setup_url),
        key(b"setup_on_update"),
        Token::True,
        key(b"hook_attributes"),
        Token::ObjectStart,
        key(b"url"),
        string(b"https://kit-008aff.invalid/"),
        key(b"active"),
        Token::True,
        Token::ObjectEnd,
        key(b"default_permissions"),
        Token::ObjectStart,
    ] {
        wanted.push(token).expect("expected token capacity");
    }
    for (name, access) in [
        (b"contents".as_slice(), b"write".as_slice()),
        (b"pull_requests", b"write"),
        (b"issues", b"write"),
        (b"merge_queues", b"write"),
        (b"checks", b"read"),
        (b"statuses", b"read"),
        (b"actions", b"read"),
        (b"administration", b"read"),
        (b"metadata", b"read"),
    ] {
        wanted.push(key(name)).expect("expected key capacity");
        wanted.push(string(access)).expect("expected value capacity");
    }
    for token in [Token::ObjectEnd, key(b"default_events"), Token::ArrayStart] {
        wanted.push(token).expect("expected token capacity");
    }
    for event in [
        b"issues".as_slice(),
        b"issue_comment",
        b"pull_request",
        b"pull_request_review",
        b"pull_request_review_comment",
        b"push",
        b"create",
        b"delete",
        b"check_run",
        b"check_suite",
        b"status",
        b"workflow_job",
        b"workflow_run",
        b"merge_group",
    ] {
        wanted.push(string(event)).expect("expected event capacity");
    }
    for token in [Token::ArrayEnd, Token::ObjectEnd] {
        wanted.push(token).expect("expected end capacity");
    }
    wanted
}
fn check_json(page: &[u8], given: &Given) {
    let json = attribute(page, b"name=\"manifest\" value=\"");
    assert_eq!(tokens(&json).as_slice(), wanted(given).as_slice());
}

#[test]
fn each_manifest_target_posts_the_wanted_fields() {
    let given = fixtures::given();
    for (target, action) in [
        (Target::Personal, b"https://github.test/settings/apps/new?state=0123456789abcdef0123456789abcdef".as_slice()),
        (
            Target::Org(b"acme".as_slice().into()),
            b"https://github.test/organizations/acme/settings/apps/new?state=0123456789abcdef0123456789abcdef",
        ),
    ] {
        let page = page(&given, &target);
        assert_eq!(attribute(&page, b"action=\"").as_ref(), action);
        assert!(bytes::find(&page, b"<form method=\"post\"").is_some());
        check_json(&page, &given);
        assert_eq!(page.as_ref(), self::page(&given, &target).as_ref());
    }
}
#[test]
fn manifest_attributes_escape_the_given_urls() {
    let mut given = fixtures::given();
    given.web = b"https://git\"&'<>.test".as_slice().into();
    given.url = b"https://kit.test/\"&'<>\\?x=1&y=2".as_slice().into();
    given.redirect_url = given.url.clone();
    given.setup_url = given.url.clone();
    let page = page(&given, &Target::Personal);
    assert_eq!(
        attribute(&page, b"action=\"").as_ref(),
        b"https://git\"&'<>.test/settings/apps/new?state=0123456789abcdef0123456789abcdef"
    );
    check_json(&page, &given);
}
#[test]
fn the_name_and_page_hold_at_the_bound() {
    let mut given = fixtures::given();
    given.limits.name_bytes = NAME_BYTES;
    for (target, length) in
        [(Target::Personal, PERSONAL_PAGE_BYTES), (Target::Org(b"acme".as_slice().into()), ORG_PAGE_BYTES)]
    {
        given.limits.page_bytes = length;
        let page = page(&given, &target);
        assert_eq!(page.len(), usize::try_from(length).expect("page bound fits"));
        check_json(&page, &given);
    }
    let login = b"A0-bcdefgh";
    Manifest::new(&given, &Target::Org(login.as_slice().into()), STATE).expect("login at its bound");
}
#[test]
fn a_name_or_page_one_byte_past_stops_without_items() {
    let given = fixtures::given();
    for (name_bytes, page_bytes) in [
        (NAME_BYTES.checked_sub(1).expect("positive name bound"), given.limits.page_bytes),
        (given.limits.name_bytes, PERSONAL_PAGE_BYTES.checked_sub(1).expect("positive page bound")),
        (given.limits.name_bytes, 1),
        (given.limits.name_bytes, 25),
        (given.limits.name_bytes, 100),
    ] {
        let mut given = given.clone();
        given.limits.name_bytes = name_bytes;
        given.limits.page_bytes = page_bytes;
        assert_eq!(make(&given, &Target::Personal, RANDOM), stopped());
    }
}
#[test]
#[expect(clippy::disallowed_macros, reason = "test checks refusal Debug for secret bytes")]
fn a_manifest_refuses_a_bad_target_or_state() {
    let given = fixtures::given();
    for (target, state, place) in [
        (Target::Org(Box::new([])), STATE, Place::Target),
        (Target::Org(Box::new([b'a'; 35])), STATE, Place::Target),
        (Target::Org(b"a/b".as_slice().into()), STATE, Place::Target),
        (Target::Org(b"a?b".as_slice().into()), STATE, Place::Target),
        (Target::Org(b"a_b".as_slice().into()), STATE, Place::Target),
        (Target::Org(b"a\x80".as_slice().into()), STATE, Place::Target),
        (Target::Personal, b"", Place::State),
        (Target::Personal, b"0123456789abcdef0123456789abcde", Place::State),
        (Target::Personal, b"0123456789abcdef0123456789abcdef0", Place::State),
        (Target::Personal, b"0123456789abcdef0123456789abcdeF", Place::State),
        (Target::Personal, b"0123456789abcdef0123456789abcdeg", Place::State),
    ] {
        let refused = Manifest::new(&given, &target, state).expect_err("invalid input");
        assert_eq!(refused, Refused { place, other: None });
        diagnostic(&format!("{refused:?}"));
    }
    let mut invalid = given;
    invalid.url = SECRET.into();
    assert_eq!(
        Manifest::new(&invalid, &Target::Org(Box::new([])), b"").expect_err("given checked first"),
        Refused { place: Place::Url, other: None }
    );
}
#[test]
fn a_manifest_requires_exactly_three_drawn_bytes() {
    let given = fixtures::given();
    for random in [b"".as_slice(), &[0], &[0, 0x8a], &[0, 0x8a, 0xff, 0], SECRET] {
        assert_eq!(make(&given, &Target::Personal, random), stopped());
    }
}
#[test]
fn a_manifest_rejects_an_out_of_order_event_and_ends_once() {
    let given = fixtures::given();
    for begun in [false, true] {
        for event in events() {
            if !begun && event == Event::Begin {
                continue;
            }
            let mut machine = Manifest::new(&given, &Target::Personal, STATE).expect("checked inputs");
            if begun {
                assert_eq!(go(&mut machine, Event::Begin), Need::Draw(3));
            }
            assert_eq!(go(&mut machine, event), stopped());
            silent(&mut machine);
        }
    }
}
