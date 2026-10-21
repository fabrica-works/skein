use super::fixtures::{self, boxed};
use crate::{
    Limits,
    git::{self, Answer, Call, Name, client, server},
};
use skein_fake_forge_domain::api;
use skein_json::tokenizer::{self as json, Tokenizer};
use skein_lib::{
    Env, Intake, Queue, Time, Wall,
    stream::{Down, Up},
};

// The fixture starts with commit 1, and preserves arbitrary file/message bytes.
const FIRST: u64 = 1;
const SECOND: u64 = 2;
fn tree() -> Box<[api::File]> {
    Box::new([api::File { path: boxed(b"\xff\0path"), content: boxed(b"\xfe\0content") }])
}
fn calls() -> [Call; 14] {
    [
        Call::Clone { repository: boxed(b"org/repo") },
        Call::Fetch { repository: boxed(b"org/repo"), want: api::Want::Default },
        Call::Fetch { repository: boxed(b"org/repo"), want: api::Want::Branch(boxed(b"main")) },
        Call::Fetch { repository: boxed(b"org/repo"), want: api::Want::Commit(FIRST) },
        Call::Create { repository: boxed(b"org/repo"), branch: boxed(b"work"), commit: FIRST },
        Call::Push { repository: boxed(b"org/repo"), branch: boxed(b"work"), commit: SECOND, expected: Some(FIRST) },
        Call::Push { repository: boxed(b"org/repo"), branch: boxed(b"work"), commit: SECOND, expected: None },
        Call::Branch { repository: boxed(b"org/repo"), branch: boxed(b"work") },
        Call::Objects { commits: Box::new([FIRST, SECOND]) },
        Call::Commit { parent: FIRST, tree: tree(), message: boxed(b"\xffmessage") },
        Call::Merge { parent: FIRST, merge_parent: SECOND, tree: tree(), message: boxed(b"\xffmessage") },
        Call::CommitTree { parent: FIRST, merging: Some(SECOND), tree: tree(), message: boxed(b"\xffmessage") },
        Call::CommitTree { parent: FIRST, merging: None, tree: tree(), message: boxed(b"\xffmessage") },
        Call::Ancestor { ancestor: FIRST, commit: SECOND },
    ]
}
fn answers() -> [(Name, Answer); 14] {
    [
        (
            Name::Clone,
            Answer::Cloned {
                default: boxed(b"main"),
                branches: Box::new([api::Head { branch: boxed(b"main"), commit: FIRST }]),
            },
        ),
        (Name::Fetch, Answer::Commit(FIRST)),
        (Name::Branch, Answer::Commit(SECOND)),
        (Name::Create, Answer::Created(api::Created::Created)),
        (Name::Create, Answer::Created(api::Created::Exists)),
        (Name::Push, Answer::Pushed(api::Pushed::Pushed)),
        (Name::Push, Answer::Pushed(api::Pushed::Rejected)),
        (Name::Commit, Answer::Stored(None)),
        (Name::Commit, Answer::Stored(Some(SECOND))),
        (Name::Merge, Answer::Stored(Some(SECOND))),
        (Name::CommitTree, Answer::Stored(Some(SECOND))),
        (Name::Ancestor, Answer::Ancestor(true)),
        (Name::Ancestor, Answer::Ancestor(false)),
        (
            Name::Objects,
            Answer::Objects {
                objects: Box::new([git::Object {
                    commit: SECOND,
                    parent: Some(FIRST),
                    merge_parent: None,
                    tree: tree(),
                    message: boxed(b"\xfemessage"),
                }]),
                graph: Box::new([git::Parents { commit: FIRST, parent: None, merge_parent: None }]),
            },
        ),
    ]
}
struct Pump {
    tokenizer: Tokenizer,
    env: Env<json::Limits>,
    above: Queue<json::Event>,
    below: Queue<Down>,
    intake: Intake,
    body: Box<[u8]>,
    at: usize,
    piece: usize,
}
impl Pump {
    fn new(body: &[u8], limits: &Limits, piece: usize) -> Self {
        Self {
            tokenizer: Tokenizer::new(&limits.documents.json),
            env: Env { now: Time::ZERO, wall: Wall::EPOCH, limits: limits.documents.json },
            above: Queue::with_capacity(1),
            below: Queue::with_capacity(1),
            intake: Intake::with_capacity(limits.documents.json.length),
            body: boxed(body),
            at: 0,
            piece,
        }
    }
    fn next(&mut self, request: json::Request) -> json::Event {
        json::down(&mut self.tokenizer, &self.env, request, &mut self.above, &mut self.below);
        loop {
            if let Some(event) = self.above.pop() {
                return event;
            }
            let Some(Down::Demand { read, .. }) = self.below.pop() else {
                panic!("tokenizer read");
            };
            let event = loop {
                if let Some(bytes) = self.intake.meet(read) {
                    break Up::Bytes(bytes);
                }
                if self.at == self.body.len() {
                    break Up::End;
                }
                let end = self.at.saturating_add(self.piece).min(self.body.len());
                self.intake.append(&self.body[self.at..end]).expect("body fits");
                self.at = end;
            };
            json::up(&mut self.tokenizer, &self.env, event, &mut self.above, &mut self.below);
        }
    }
}
fn decode_call(body: &[u8], name: Name, limits: &Limits, piece: usize) -> server::BodyStep {
    let mut decoder = server::decoder(name, limits);
    let mut pump = Pump::new(body, limits, piece);
    let mut demand = server::body_first(&decoder);
    loop {
        match server::body(&mut decoder, limits, &pump.next(demand)) {
            server::BodyStep::Demand(next) => demand = next,
            outcome @ (server::BodyStep::Decoded(_) | server::BodyStep::Malformed) => return outcome,
        }
    }
}
fn decode_answer(body: &[u8], name: Name, status: u16, limits: &Limits, piece: usize) -> client::Step {
    decoded(body, name, status, limits, piece).0
}
fn decoded(body: &[u8], name: Name, status: u16, limits: &Limits, piece: usize) -> (client::Step, bool) {
    let mut decoder = client::decoder(name, status, Time::ZERO, limits);
    let mut pump = Pump::new(body, limits, piece);
    let mut demand = client::first(&decoder);
    loop {
        match client::step(&mut decoder, limits, &pump.next(demand)) {
            client::Step::Demand(next) => demand = next,
            outcome @ (client::Step::Decoded(_) | client::Step::Malformed) => {
                return (outcome, client::named(&decoder));
            }
        }
    }
}
#[test]
fn git_calls_round_trip_whole_and_bytewise() {
    let limits = fixtures::limits();
    for call in calls() {
        let encoded = client::encode(&call, &limits).expect("call encoding");
        assert_eq!(git::name(&encoded.target), Some(Ok(call.name())));
        for piece in [usize::MAX, 1] {
            assert_eq!(
                decode_call(&encoded.body, call.name(), &limits, piece),
                server::BodyStep::Decoded(call.clone()),
                "{:?}/{piece}",
                call.name()
            );
        }
    }
}
#[test]
fn git_answers_round_trip_whole_and_bytewise() {
    let limits = fixtures::limits();
    for (name, answer) in answers() {
        let body = server::encode(&answer, &limits).expect("answer encoding");
        for piece in [usize::MAX, 1] {
            assert_eq!(
                decoded(&body, name, 200, &limits, piece),
                (client::Step::Decoded(Ok(answer.clone())), false),
                "{name:?}/{piece}"
            );
        }
    }
}
#[test]
fn git_errors_round_trip_whole_and_bytewise() {
    use api::Error as E;
    let limits = fixtures::limits();
    let mut errors = skein_lib::List::with_capacity(28);
    for error in [
        E::Unavailable,
        E::Timeout,
        E::RateLimited { reset: Time::from_nanos(123) },
        E::Forbidden,
        E::TooLarge,
        E::Full,
        E::Exists,
        E::AlreadyQueued(api::QueueEntry { number: FIRST, head: SECOND, position: 3 }),
        E::Circular,
        E::NothingToMerge,
        E::Empty,
        E::Closed,
        E::Stale,
        E::Conflict,
        E::Protected,
        E::Unreachable,
        E::Refused,
    ] {
        errors.push(error).expect("error rows");
    }
    for what in [
        api::What::Repository,
        api::What::Item,
        api::What::Pull,
        api::What::Comment,
        api::What::Label,
        api::What::Branch,
        api::What::Commit,
        api::What::File,
        api::What::Page,
        api::What::Review,
        api::What::Job,
    ] {
        errors.push(E::Missing(what)).expect("missing rows");
    }
    for error in errors.into_boxed() {
        let body = server::error(error, &limits).expect("error encoding");
        for status in [404, 403, 401, 422] {
            for piece in [usize::MAX, 1] {
                assert_eq!(
                    decoded(&body, Name::Fetch, status, &limits, piece),
                    (client::Step::Decoded(Err(error)), true),
                    "{error:?}/{status}/{piece}"
                );
            }
        }
    }
}
fn repeated(count: u32, byte: u8) -> Box<[u8]> {
    let mut writer = skein_lib::Writer::new(count.try_into().expect("test size"));
    for _ in 0..count {
        writer.put(&[byte]).expect("sized bytes");
    }
    writer.finish()
}
fn maximal_tree() -> (Call, Answer) {
    let domain = fixtures::fake_config().limits;
    let mut files = skein_lib::List::with_capacity(domain.files);
    for _ in 0..domain.files {
        files
            .push(api::File { path: repeated(domain.name_bytes, 0xff), content: repeated(domain.content_bytes, 0xfe) })
            .expect("max files");
    }
    let tree = files.into_boxed();
    let message = repeated(domain.title_bytes.saturating_add(domain.body_bytes).saturating_add(2), 0xff);
    (
        Call::CommitTree { parent: u64::MAX, merging: Some(u64::MAX), tree: tree.clone(), message: message.clone() },
        Answer::Objects {
            objects: Box::new([git::Object {
                commit: u64::MAX,
                parent: Some(u64::MAX),
                merge_parent: Some(u64::MAX),
                tree,
                message,
            }]),
            graph: Box::new([]),
        },
    )
}
#[test]
fn git_fits_matches_real_encoding_at_each_bound() {
    let domain = fixtures::fake_config().limits;
    let bound = git::tree_bound(&domain).expect("representable");
    let (call, answer) = maximal_tree();
    let limits = fixtures::limits();
    assert_eq!(client::encode(&call, &limits).expect("call").body.len(), usize::try_from(bound.request).expect("size"));
    assert_eq!(server::encode(&answer, &limits).expect("answer").len(), usize::try_from(bound.answer).expect("size"));
    for delta in [0, 1] {
        let mut limits = limits;
        limits.documents.body_bytes = bound.answer.saturating_sub(delta);
        limits.documents.json.length = bound.answer.saturating_sub(delta);
        limits.documents.writer.length = bound.answer.saturating_sub(delta);
        limits.documents.json.string = bound.string;
        limits.http.body = u64::from(bound.request);
        assert_eq!(git::fits(&limits, &domain), delta == 0, "room delta {delta}");
        assert_eq!(server::encode(&answer, &limits).is_ok(), delta == 0, "encoding delta {delta}");
        limits = fixtures::limits();
        limits.http.body = u64::from(bound.request.saturating_sub(delta));
        assert_eq!(git::fits(&limits, &domain), delta == 0, "request delta {delta}");
        assert_eq!(client::encode(&call, &limits).is_ok(), delta == 0, "call encoding delta {delta}");
    }
}

fn basic(login: &[u8], token: &[u8]) -> Box<[u8]> {
    use skein_lib::{Writer, base64};
    let mut clear = Writer::new(login.len().saturating_add(1).saturating_add(token.len()));
    clear.put(login).expect("login");
    clear.put(b":").expect("colon");
    clear.put(token).expect("token");
    let clear = clear.finish();
    let size = base64::encoded_len(base64::Alphabet::Standard, clear.len().try_into().expect("credential"))
        .expect("credential");
    let mut out = Writer::new(6_usize.saturating_add(size.try_into().expect("credential")));
    out.put(b"Basic ").expect("scheme");
    base64::encode(base64::Alphabet::Standard, &clear, &mut out).expect("credential");
    out.finish()
}
fn ask_raw(h: &mut fixtures::Harness, method: &[u8], path: &[u8], body: &[u8], authorization: Option<&[u8]>) {
    use skein_lib::{Decimal, Writer};
    let authorization = authorization.unwrap_or(b"");
    let digits = Decimal::of(body.len().try_into().expect("body length"));
    let parts = [
        method,
        b" ",
        path,
        b" HTTP/1.1\r\nHost: localhost\r\nAuthorization: ",
        authorization,
        b"\r\nContent-Length: ",
        digits.as_bytes(),
        b"\r\n\r\n",
        body,
    ];
    let mut length = 0_usize;
    for part in parts {
        length = length.saturating_add(part.len());
    }
    let mut out = Writer::new(length);
    for part in parts {
        out.put(part).expect("sized request");
    }
    h.request(&out.finish());
    h.run();
}
fn ask(h: &mut fixtures::Harness, call: &Call) {
    let encoded = client::encode(call, &h.env.limits).expect("call");
    ask_raw(h, b"POST", &encoded.target, &encoded.body, Some(&basic(b"engine", fixtures::TOKEN)));
}
fn answer(h: &fixtures::Harness, name: Name) -> Answer {
    let client::Step::Decoded(Ok(answer)) = decode_answer(&h.body(), name, h.status(), &h.env.limits, usize::MAX)
    else {
        panic!("git answer: {} {:?}", h.status(), h.body());
    };
    answer
}
#[test]
fn git_branch_moves_share_the_domain() {
    let mut h = fixtures::Harness::new(fixtures::config());
    let rows = [
        (
            Call::CommitTree { parent: FIRST, merging: None, tree: tree(), message: boxed(b"\xffmessage") },
            Answer::Stored(Some(SECOND)),
        ),
        (
            Call::Create { repository: boxed(b"org/repo"), branch: boxed(b"work"), commit: FIRST },
            Answer::Created(api::Created::Created),
        ),
        (
            Call::Push {
                repository: boxed(b"org/repo"),
                branch: boxed(b"work"),
                commit: SECOND,
                expected: Some(FIRST),
            },
            Answer::Pushed(api::Pushed::Pushed),
        ),
        (Call::Branch { repository: boxed(b"org/repo"), branch: boxed(b"work") }, Answer::Commit(SECOND)),
    ];
    for (call, expected) in rows {
        ask(&mut h, &call);
        assert_eq!(answer(&h, call.name()), expected);
    }
    assert_eq!(h.domain_calls(), 3);
    assert_eq!(h.service.tally().total, 4);
    assert_eq!(h.service.tally().unauthorized, 0);
    assert_eq!(
        h.domain.inspect(&h.fake_env.limits, b"org/repo", &api::Read::Branch { branch: boxed(b"work") }),
        Ok(api::Answer::Commit(SECOND))
    );
}
fn route_calls() -> [Call; 10] {
    [
        Call::Clone { repository: boxed(b"org/repo") },
        Call::Fetch { repository: boxed(b"org/repo"), want: api::Want::Default },
        Call::Create { repository: boxed(b"org/repo"), branch: boxed(b"work"), commit: 1 },
        Call::Push { repository: boxed(b"org/repo"), branch: boxed(b"main"), commit: 1, expected: None },
        Call::Branch { repository: boxed(b"org/repo"), branch: boxed(b"main") },
        Call::Objects { commits: Box::new([1]) },
        Call::Commit { parent: 1, tree: tree(), message: boxed(b"changed") },
        Call::Merge { parent: 1, merge_parent: 1, tree: Box::new([]), message: boxed(b"merge") },
        Call::CommitTree { parent: 1, merging: None, tree: Box::new([]), message: boxed(b"commit-tree") },
        Call::Ancestor { ancestor: 1, commit: 1 },
    ]
}
#[test]
fn git_method_and_unknown_route_statuses() {
    for call in route_calls() {
        let mut h = fixtures::Harness::new(fixtures::config());
        let encoded = client::encode(&call, &h.env.limits).expect("call");
        ask_raw(&mut h, b"GET", &encoded.target, b"", None);
        assert_eq!(h.status(), 405, "{:?}", call.name());
        assert_eq!(h.domain_calls(), 0);
    }
    for path in [b"/.fake/git/unknown".as_slice(), b"/.fake/git/", b"/.fake/git/fetch/", b"/.fake/git/fetch?x=1"] {
        let mut h = fixtures::Harness::new(fixtures::config());
        ask_raw(&mut h, b"POST", path, b"{}", None);
        assert_eq!(h.status(), 404, "{path:?}");
    }
    assert_eq!(git::name(b"/api/v1/user"), None);
    assert_eq!(git::name(b"/.fake/git"), None);
}
#[test]
fn git_malformed_call_table() {
    let rows: [(Name, &[u8]); 27] = [
        (Name::Clone, b"[]"),
        (Name::Clone, b"null"),
        (Name::Clone, b"{}{}"),
        (Name::Clone, b"{\"repository\":\"org/repo\"} false"),
        (Name::Clone, br#"{"repository":"org/repo","unknown":0}"#),
        (Name::Clone, br#"{"repository":"org/repo","repository":"other"}"#),
        (Name::Clone, b"{}"),
        (Name::Clone, br#"{"repository":1}"#),
        (Name::Fetch, br#"{"repository":"org/repo","branch":"main","commit":1}"#),
        (Name::Fetch, br#"{"repository":"org/repo","commit":null}"#),
        (Name::Fetch, br#"{"repository":"org/repo","commit":1.0}"#),
        (Name::Create, br#"{"repository":"org/repo","branch":"work"}"#),
        (Name::Push, br#"{"repository":"org/repo","branch":"main","commit":1,"expected":null}"#),
        (Name::Branch, br#"{"repository":"org/repo","branch":false}"#),
        (Name::Objects, br#"{"commits":{}}"#),
        (Name::Objects, br#"{"commits":["1"]}"#),
        (Name::Commit, br#"{"parent":1,"tree":[],"message":"/w"}"#),
        (Name::Commit, br#"{"parent":1,"tree":[],"message":"_w=="}"#),
        (Name::Commit, br#"{"parent":1,"tree":[],"message":"/x=="}"#),
        (Name::Commit, br#"{"parent":1,"tree":[{"path":"!","content":""}],"message":""}"#),
        (Name::Commit, br#"{"parent":1,"tree":[{"path":""}],"message":""}"#),
        (Name::Commit, br#"{"parent":1,"tree":[{"path":"","content":"","other":0}],"message":""}"#),
        (Name::Commit, br#"{"parent":1,"tree":[{"path":"","path":"","content":""}],"message":""}"#),
        (Name::Merge, br#"{"parent":1,"tree":[],"message":""}"#),
        (Name::CommitTree, br#"{"parent":1,"merging":null,"tree":[],"message":""}"#),
        (Name::Ancestor, br#"{"ancestor":true,"commit":1}"#),
        (Name::Ancestor, br#"{"ancestor":1,"commit":18446744073709551616}"#),
    ];
    for (name, body) in rows {
        for piece in [usize::MAX, 1] {
            assert_eq!(
                decode_call(body, name, &fixtures::limits(), piece),
                server::BodyStep::Malformed,
                "{name:?}/{body:?}/{piece}"
            );
        }
        let mut h = fixtures::Harness::new(fixtures::config());
        let mut path = skein_lib::Writer::new(11_usize.saturating_add(name.segment().len()));
        path.put(b"/.fake/git/").expect("prefix");
        path.put(name.segment()).expect("name");
        ask_raw(&mut h, b"POST", &path.finish(), body, Some(&basic(b"engine", fixtures::TOKEN)));
        assert_eq!(h.status(), 422, "{name:?}/{body:?}");
        assert_eq!(h.domain_calls(), 0);
    }
}
#[test]
fn git_members_arrive_in_any_order() {
    let rows: [(Name, &[u8]); 5] = [
        (Name::Push, br#"{"expected":1,"commit":1,"branch":"main","repository":"org/repo"}"#),
        (Name::CommitTree, br#"{"message":"/w==","tree":[{"content":"/g==","path":"/w=="}],"merging":1,"parent":1}"#),
        (Name::Merge, br#"{"message":"","tree":[],"merge_parent":1,"parent":1}"#),
        (Name::Ancestor, br#"{"commit":1,"ancestor":1}"#),
        (Name::Fetch, br#"{"branch":"main","repository":"org/repo"}"#),
    ];
    for (name, body) in rows {
        let server::BodyStep::Decoded(call) = decode_call(body, name, &fixtures::limits(), 1) else {
            panic!("order {name:?}")
        };
        let mut h = fixtures::Harness::new(fixtures::config());
        ask(&mut h, &call);
        assert_eq!(h.status(), 200);
    }
}
struct CredentialCase {
    label: &'static [u8],
    login: &'static [u8],
    token: &'static [u8],
    push: bool,
    expected: u16,
}
#[test]
fn git_basic_credentials_bind_login_and_scopes() {
    use skein_lib::Secret;
    let rows: [CredentialCase; 14] = [
        CredentialCase { label: b"config right", login: b"engine", token: fixtures::TOKEN, push: false, expected: 200 },
        CredentialCase { label: b"config wrong", login: b"person", token: fixtures::TOKEN, push: false, expected: 401 },
        CredentialCase { label: b"setup right", login: b"kit", token: b"setup-write", push: false, expected: 200 },
        CredentialCase { label: b"setup wrong", login: b"engine", token: b"setup-write", push: false, expected: 401 },
        CredentialCase { label: b"unknown token", login: b"engine", token: b"unknown", push: false, expected: 401 },
        CredentialCase { label: b"read fetch", login: b"kit", token: b"setup-read", push: false, expected: 200 },
        CredentialCase { label: b"read push", login: b"kit", token: b"setup-read", push: true, expected: 403 },
        CredentialCase { label: b"write push", login: b"kit", token: b"setup-write", push: true, expected: 200 },
        CredentialCase { label: b"user scope", login: b"kit", token: b"setup-user", push: false, expected: 403 },
        CredentialCase { label: b"empty login", login: b"", token: fixtures::TOKEN, push: false, expected: 401 },
        CredentialCase { label: b"empty token", login: b"engine", token: b"", push: false, expected: 401 },
        CredentialCase {
            label: b"config login is exact",
            login: b"Engine",
            token: fixtures::TOKEN,
            push: false,
            expected: 401,
        },
        CredentialCase {
            label: b"setup login is exact",
            login: b"Kit",
            token: b"setup-read",
            push: false,
            expected: 401,
        },
        CredentialCase { label: b"read clone", login: b"kit", token: b"setup-read", push: false, expected: 200 },
    ];
    for CredentialCase { label, login, token, push, expected } in rows {
        let mut h = fixtures::Harness::new(fixtures::config());
        let id = h.service.setup_mut().account(b"kit", false, b"");
        for (secret, scope) in [
            (b"setup-read".as_slice(), b"read:repository".as_slice()),
            (b"setup-write", b"write:repository"),
            (b"setup-user", b"read:user"),
        ] {
            h.service.setup_mut().token(id, secret, &[scope], Secret::new(boxed(secret)));
        }
        let call = if push {
            Call::Push { repository: boxed(b"org/repo"), branch: boxed(b"main"), commit: 1, expected: None }
        } else if label == b"read clone" {
            Call::Clone { repository: boxed(b"org/repo") }
        } else {
            Call::Fetch { repository: boxed(b"org/repo"), want: api::Want::Default }
        };
        let encoded = client::encode(&call, &h.env.limits).expect("call");
        ask_raw(&mut h, b"POST", &encoded.target, &encoded.body, Some(&basic(login, token)));
        assert_eq!(h.status(), expected, "{label:?}");
        assert_eq!(h.domain_calls(), u32::from(expected == 200));
    }
    for credential in [
        None,
        Some(b"token 1111111111111111111111111111111111111111".as_slice()),
        Some(b"Basic !"),
        Some(b"Basic ZW5naW5l"),
        Some(b"basic ZW5naW5lOg=="),
    ] {
        let mut h = fixtures::Harness::new(fixtures::config());
        ask_raw(&mut h, b"POST", b"/.fake/git/clone", br#"{"repository":"org/repo"}"#, credential);
        assert_eq!(h.status(), 401, "{credential:?}");
    }
}
#[test]
fn git_repository_calls_take_domain_latency_and_faults() {
    use skein_lib::Duration;
    for call in route_calls() {
        if !call.name().repository() {
            continue;
        }
        for (label, status) in [("healthy", 200), ("rate", 429), ("unavailable", 503), ("timeout", 500)] {
            let mut h = fixtures::Harness::new(fixtures::config());
            match label {
                "rate" => {
                    h.fake_env.limits.rate_limit = 1;
                    h.fake_env.limits.rate_window = Duration::from_secs(1000);
                    h.call(1, api::Op::Read(api::Read::Branch { branch: boxed(b"main") })).expect("first rate slot");
                }
                "unavailable" => h.fake_env.limits.unavailable = 1000,
                "timeout" => h.fake_env.limits.timeouts = 1000,
                _ => {}
            }
            let before = h.fake_env.now;
            ask(&mut h, &call);
            assert_eq!(h.status(), status, "{:?}/{label}", call.name());
            assert_eq!(h.domain_calls(), 1);
            assert!(h.fake_env.now > before, "domain latency advances fake time");
            if status == 429 {
                assert!(skein_lib::bytes::find(&h.head(), b"retry-after:").is_some());
            }
        }
    }
}
#[test]
fn git_repository_outcome_table() {
    use skein_fake_forge_domain as domain;
    let rows = [
        (Call::Clone { repository: boxed(b"missing") }, 404, b"missing".as_slice()),
        (Call::Fetch { repository: boxed(b"org/repo"), want: api::Want::Branch(boxed(b"missing")) }, 404, b"missing"),
        (Call::Fetch { repository: boxed(b"org/repo"), want: api::Want::Commit(99) }, 404, b"missing"),
        (Call::Create { repository: boxed(b"org/repo"), branch: boxed(b"work"), commit: 99 }, 404, b"missing"),
        (
            Call::Push { repository: boxed(b"org/repo"), branch: boxed(b"main"), commit: 99, expected: None },
            404,
            b"missing",
        ),
        (Call::Branch { repository: boxed(b"org/repo"), branch: boxed(b"missing") }, 404, b"missing"),
        (Call::Create { repository: boxed(b"org/repo"), branch: boxed(b"work"), commit: 1 }, 403, b"refused"),
        (
            Call::Push { repository: boxed(b"org/repo"), branch: boxed(b"main"), commit: 1, expected: None },
            403,
            b"refused",
        ),
    ];
    for (call, status, error) in rows {
        let mut h = fixtures::Harness::new(fixtures::config());
        if error == b"refused" {
            domain::set_refusing(&mut h.domain, b"org/repo", true);
        }
        ask(&mut h, &call);
        assert_eq!(h.status(), status, "{call:?}");
        assert!(skein_lib::bytes::find(&h.body(), error).is_some());
    }
    for call in route_calls() {
        if !call.name().repository() {
            continue;
        }
        let mut h = fixtures::Harness::new(fixtures::config());
        domain::grant(&mut h.domain, b"org/repo", 1, api::Permission::None);
        ask(&mut h, &call);
        assert_eq!(h.status(), 403, "{:?}", call.name());
        if call.name() != Name::Branch {
            let mut h = fixtures::Harness::new(fixtures::config());
            domain::set_reachable(&mut h.domain, b"org/repo", false);
            ask(&mut h, &call);
            assert_eq!(h.status(), 502, "{:?}", call.name());
        }
    }
    let mut h = fixtures::Harness::new(fixtures::config());
    for (call, result) in [
        (
            Call::Create { repository: boxed(b"org/repo"), branch: boxed(b"main"), commit: 1 },
            Answer::Created(api::Created::Exists),
        ),
        (
            Call::Push { repository: boxed(b"org/repo"), branch: boxed(b"main"), commit: 1, expected: Some(99) },
            Answer::Pushed(api::Pushed::Rejected),
        ),
    ] {
        ask(&mut h, &call);
        assert_eq!(answer(&h, call.name()), result);
    }
}
#[test]
fn git_store_calls_ignore_credentials_latency_rate_and_domain_faults() {
    use skein_lib::Duration;
    for call in route_calls() {
        if call.name().repository() {
            continue;
        }
        for fault in [0_u8, 1, 2, 3] {
            let mut h = fixtures::Harness::new(fixtures::config());

            match fault {
                0 => h.fake_env.limits.unavailable = 1000,
                1 => h.fake_env.limits.timeouts = 1000,
                2 => h.fake_env.limits.landing = 1000,
                _ => {
                    h.fake_env.limits.rate_limit = 1;
                    h.fake_env.limits.rate_window = Duration::from_secs(1000);
                    h.call(1, api::Op::Read(api::Read::Branch { branch: boxed(b"main") })).expect("first rate slot");
                }
            }
            h.fake_env.limits.latency_min = Duration::from_secs(100);
            h.fake_env.limits.latency_max = Duration::from_secs(100);
            let encoded = client::encode(&call, &h.env.limits).expect("call");
            let before = h.fake_env.now;
            ask_raw(&mut h, b"POST", &encoded.target, &encoded.body, None);
            assert_eq!(h.status(), 200, "{:?}/{fault}", call.name());
            assert_eq!(h.domain_calls(), 0);
            assert!(h.fake_env.now.saturating_since(before) < Duration::from_secs(1));
            assert_eq!(h.service.tally().total, 1);
            assert_eq!(h.service.tally().unauthorized, 0);
            assert!(h.service.tally().users.is_empty());
        }
    }
}
#[test]
fn git_store_missing_and_oversize_table() {
    let mut large = tree();
    large.first_mut().expect("file").content = repeated(129, 0xff);
    let mut longpath = tree();
    longpath.first_mut().expect("file").path = repeated(65, 0xff);
    let rows = [
        (Call::Objects { commits: Box::new([99]) }, 404),
        (Call::Commit { parent: 99, tree: tree(), message: boxed(b"") }, 404),
        (Call::Merge { parent: 1, merge_parent: 99, tree: Box::new([]), message: boxed(b"") }, 404),
        (Call::CommitTree { parent: 99, merging: None, tree: Box::new([]), message: boxed(b"") }, 404),
        (Call::Ancestor { ancestor: 99, commit: 1 }, 404),
        (Call::Ancestor { ancestor: 1, commit: 99 }, 404),
        (Call::Commit { parent: 1, tree: large.clone(), message: boxed(b"") }, 413),
        (Call::Merge { parent: 1, merge_parent: 1, tree: longpath, message: boxed(b"") }, 413),
        (Call::CommitTree { parent: 1, merging: None, tree: large, message: boxed(b"") }, 413),
        (Call::CommitTree { parent: 1, merging: None, tree: tree(), message: repeated(1091, 0xff) }, 413),
    ];
    for (call, status) in rows {
        let mut h = fixtures::Harness::new(fixtures::config());
        ask(&mut h, &call);
        assert_eq!(h.status(), status, "{call:?}");
        assert!(
            skein_lib::bytes::find(&h.body(), if status == 404 { b"\"what\":\"commit\"" } else { b"too_large" })
                .is_some()
        );
        assert_eq!(h.domain_calls(), 0);
    }
}
#[test]
fn git_store_unchanged_merge_and_full_table() {
    use skein_fake_forge_domain as domain;
    let mut h = fixtures::Harness::new(fixtures::config());
    for (call, want) in [
        (Call::Commit { parent: 1, tree: Box::new([]), message: boxed(b"same") }, Answer::Stored(None)),
        (
            Call::Merge { parent: 1, merge_parent: 1, tree: Box::new([]), message: boxed(b"same") },
            Answer::Stored(Some(2)),
        ),
        (
            Call::CommitTree { parent: 1, merging: None, tree: Box::new([]), message: boxed(b"same") },
            Answer::Stored(Some(3)),
        ),
    ] {
        ask(&mut h, &call);
        assert_eq!(answer(&h, call.name()), want);
    }
    for _ in 3..h.fake_env.limits.limits.commits {
        domain::commit_tree(&mut h.domain, &h.fake_env.limits, 1, None, Box::new([]), b"").expect("fill store");
    }
    for call in [
        Call::Commit { parent: 1, tree: tree(), message: boxed(b"") },
        Call::Merge { parent: 1, merge_parent: 1, tree: Box::new([]), message: boxed(b"") },
        Call::CommitTree { parent: 1, merging: None, tree: Box::new([]), message: boxed(b"") },
    ] {
        ask(&mut h, &call);
        assert_eq!(h.status(), 507);
        assert_eq!(h.body().as_ref(), br#"{"error":"full"}"#);
    }
}
fn history(h: &mut fixtures::Harness) {
    use skein_fake_forge_domain as domain;
    for (parent, merge) in [(1, None), (1, None), (2, Some(3)), (4, None), (5, Some(3))] {
        domain::commit_tree(&mut h.domain, &h.fake_env.limits, parent, merge, Box::new([]), b"").expect("history");
    }
}
fn ids(answer: Answer) -> (Box<[u64]>, Box<[u64]>) {
    let Answer::Objects { objects, graph } = answer else { panic!("objects") };
    let mut whole = skein_lib::List::with_capacity(16);
    let mut parents = skein_lib::List::with_capacity(16);
    for object in objects {
        whole.push(object.commit).expect("ids");
    }
    for row in graph {
        parents.push(row.commit).expect("ids");
    }
    (whole.into_boxed(), parents.into_boxed())
}
#[test]
fn git_objects_walk_both_parents_newest_first_without_asked_repeats() {
    for (asked, whole, graph) in [
        (boxed_ids(&[6]), boxed_ids(&[6]), boxed_ids(&[5, 4, 3, 2, 1])),
        (boxed_ids(&[3, 6]), boxed_ids(&[3, 6]), boxed_ids(&[5, 4, 2, 1])),
        (boxed_ids(&[6, 4]), boxed_ids(&[6, 4]), boxed_ids(&[5, 3, 2, 1])),
        (boxed_ids(&[6, 6]), boxed_ids(&[6, 6]), boxed_ids(&[5, 4, 3, 2, 1])),
        (boxed_ids(&[]), boxed_ids(&[]), boxed_ids(&[])),
    ] {
        let mut config = fixtures::config();
        config.max_response_items = 8;
        let mut h = fixtures::Harness::new(config);
        history(&mut h);
        ask(&mut h, &Call::Objects { commits: asked });
        assert_eq!(ids(answer(&h, Name::Objects)), (whole, graph));
    }
}
fn boxed_ids(ids: &[u64]) -> Box<[u64]> {
    let mut list = skein_lib::List::with_capacity(u32::try_from(ids.len()).expect("ids"));
    for id in ids {
        list.push(*id).expect("ids");
    }
    list.into_boxed()
}
#[test]
fn git_objects_items_and_room_bound_table() {
    for (cap, asked, status, whole, graph) in [
        (2, boxed_ids(&[6]), 200, boxed_ids(&[6]), boxed_ids(&[5])),
        (2, boxed_ids(&[3, 6]), 200, boxed_ids(&[3, 6]), boxed_ids(&[])),
        (2, boxed_ids(&[6, 5, 4]), 422, boxed_ids(&[]), boxed_ids(&[])),
        (2, boxed_ids(&[6, 99]), 404, boxed_ids(&[]), boxed_ids(&[])),
    ] {
        let mut config = fixtures::config();
        config.max_response_items = cap;
        let mut h = fixtures::Harness::new(config);
        history(&mut h);
        ask(&mut h, &Call::Objects { commits: asked });
        assert_eq!(h.status(), status);
        if status == 200 {
            assert_eq!(ids(answer(&h, Name::Objects)), (whole, graph));
        }
    }
    let mut config = fixtures::config();
    config.max_response_items = 8;
    let mut h = fixtures::Harness::new(config);
    history(&mut h);
    // A whole object plus exactly two graph rows; the third row is one byte past.
    let whole =
        git::Object { commit: 6, parent: Some(5), merge_parent: Some(3), tree: Box::new([]), message: boxed(b"") };
    let graph = Box::new([
        git::Parents { commit: 5, parent: Some(4), merge_parent: None },
        git::Parents { commit: 4, parent: Some(2), merge_parent: Some(3) },
    ]);
    let encoded = server::encode(&Answer::Objects { objects: Box::new([whole.clone()]), graph }, &h.env.limits)
        .expect("reference answer");
    h.env.limits.documents.writer.length = u32::try_from(encoded.len()).expect("room");
    ask(&mut h, &Call::Objects { commits: Box::new([6]) });
    assert_eq!(ids(answer(&h, Name::Objects)), (boxed_ids(&[6]), boxed_ids(&[5, 4])));
    h.env.limits.documents.writer.length = h.env.limits.documents.writer.length.saturating_sub(1);
    ask(&mut h, &Call::Objects { commits: Box::new([6]) });
    assert_eq!(ids(answer(&h, Name::Objects)), (boxed_ids(&[6]), boxed_ids(&[5])));
    // Selecting whole objects stops in asked order, even if a later one is smaller.
    let encoded =
        server::encode(&Answer::Objects { objects: Box::new([whole]), graph: Box::new([]) }, &fixtures::limits())
            .expect("reference whole");
    h.env.limits.documents.writer.length = u32::try_from(encoded.len()).expect("room");
    ask(&mut h, &Call::Objects { commits: Box::new([6, 5]) });
    assert_eq!(ids(answer(&h, Name::Objects)), (boxed_ids(&[6]), boxed_ids(&[])));
    ask(&mut h, &Call::Objects { commits: Box::new([6, 99]) });
    assert_eq!(h.status(), 404);
}
#[test]
fn git_route_tally_counts_every_name_and_only_repository_users() {
    for call in route_calls() {
        let mut h = fixtures::Harness::new(fixtures::config());
        ask(&mut h, &call);
        assert_eq!(h.status(), 200);
        let tally = h.service.tally();
        assert_eq!(tally.total, 1);
        assert_eq!(tally.unauthorized, 0);
        assert_eq!(tally.routes.get(&crate::Route::Git(call.name())), Some(&1));
        assert_eq!(tally.users.get(&1), if call.name().repository() { Some(&1) } else { None });
    }
    for unauthorized in [0, 7, u64::MAX] {
        let mut tally = crate::Tally::new(0);
        tally.unauthorized = unauthorized;
        tally.count_store(Some(crate::Route::Git(Name::Objects)));
        assert_eq!(tally.unauthorized, unauthorized);
        assert_eq!(tally.total, 1);
        assert!(tally.users.is_empty());
    }
}
#[test]
fn git_byte_faults_reach_repository_and_store_calls() {
    for store in [false, true] {
        for fault in [0_u8, 1, 2] {
            let mut config = fixtures::config();
            match fault {
                0 => config.faults.server_error = 1000,
                1 => config.faults.drop = 1000,
                _ => config.faults.reset = 1000,
            }
            let mut h = fixtures::Harness::new(config);
            let call = if store {
                Call::Ancestor { ancestor: 1, commit: 1 }
            } else {
                Call::Clone { repository: boxed(b"org/repo") }
            };
            ask(&mut h, &call);
            match fault {
                0 => assert_eq!(h.status(), 500),
                _ => assert!(h.closed),
            }
        }
    }
}
#[test]
fn git_encoder_refusal_uses_the_table_413_path() {
    for call in [
        Call::Clone { repository: boxed(b"org/repo") },
        Call::Ancestor { ancestor: 1, commit: 1 },
        Call::Objects { commits: Box::new([1]) },
    ] {
        let mut h = fixtures::Harness::new(fixtures::config());
        h.env.limits.documents.writer.length = 1;
        let encoded = client::encode(&call, &fixtures::limits()).expect("request outside tiny answer bound");
        ask_raw(&mut h, b"POST", &encoded.target, &encoded.body, Some(&basic(b"engine", fixtures::TOKEN)));
        assert_eq!(h.status(), 413);
    }
}
#[test]
fn git_store_without_a_lent_domain_is_not_found() {
    for call in route_calls() {
        if call.name().repository() {
            continue;
        }
        let mut h = fixtures::Harness::new(fixtures::config());
        h.lend_domain = false;
        ask(&mut h, &call);
        assert_eq!(h.status(), 404, "{:?}", call.name());
        assert_eq!(h.domain_calls(), 0);
    }
}
#[test]
fn git_protected_branches_refuse_create_and_push() {
    for call in [
        Call::Create { repository: boxed(b"org/repo"), branch: boxed(b"main"), commit: 1 },
        Call::Push { repository: boxed(b"org/repo"), branch: boxed(b"main"), commit: 1, expected: None },
    ] {
        let mut h = fixtures::Harness::new(fixtures::config());
        h.call(
            1,
            api::Op::Write(api::Write::SetProtection {
                protection: Some(api::Protection {
                    linear: false,
                    code_owners: false,
                    bypassing_users: Box::new([]),
                    queue: false,
                    branch: boxed(b"main"),
                    contexts: Box::new([]),
                    approvals: 0,
                    dismiss_stale: false,
                }),
            }),
        )
        .expect("protection");
        ask(&mut h, &call);
        assert_eq!(h.status(), 403);
        assert_eq!(h.body().as_ref(), br#"{"error":"protected"}"#);
    }
}
#[test]
fn git_client_refuses_malformed_success() {
    let rows: [(Name, &[u8]); 10] = [
        (Name::Clone, br#"{"default":"main","branches":[{"branch":"main","commit":null}]}"#),
        (Name::Fetch, br#"{"commit":1,"commit":2}"#),
        (Name::Create, br#"{"result":"pushed"}"#),
        (Name::Push, br#"{"result":"created"}"#),
        (Name::Branch, br#"{"commit":"1"}"#),
        (Name::Objects, br#"{"objects":[],"graph":[{"commit":1,"parent":null}]}"#),
        (Name::Commit, br#"{"commit":true}"#),
        (Name::Merge, b"[]"),
        (Name::CommitTree, br#"{"commit":1,"extra":0}"#),
        (Name::Ancestor, br#"{"ancestor":0}"#),
    ];
    for (name, body) in rows {
        for piece in [usize::MAX, 1] {
            assert_eq!(decode_answer(body, name, 200, &fixtures::limits(), piece), client::Step::Malformed, "{name:?}");
        }
    }
}
#[test]
fn git_client_uses_unnamed_error_status() {
    for (status, error) in [
        (401, api::Error::Forbidden),
        (403, api::Error::Forbidden),
        (404, api::Error::Missing(api::What::Repository)),
        (413, api::Error::TooLarge),
        (429, api::Error::RateLimited { reset: Time::ZERO }),
        (500, api::Error::Timeout),
        (502, api::Error::Unreachable),
        (503, api::Error::Unavailable),
        (507, api::Error::Full),
    ] {
        for body in [b"".as_slice(), b"{}", b"not json", br#"{"message":"Not Found"}"#] {
            assert_eq!(
                decoded(body, Name::Fetch, status, &fixtures::limits(), 1),
                (client::Step::Decoded(Err(error)), false),
                "{status}/{body:?}"
            );
        }
    }
}
#[test]
fn git_repository_missing_names_and_branch_storage_limits() {
    for call in [
        Call::Clone { repository: boxed(b"absent") },
        Call::Fetch { repository: boxed(b"absent"), want: api::Want::Default },
        Call::Create { repository: boxed(b"absent"), branch: boxed(b"work"), commit: 1 },
        Call::Push { repository: boxed(b"absent"), branch: boxed(b"work"), commit: 1, expected: None },
        Call::Branch { repository: boxed(b"absent"), branch: boxed(b"work") },
    ] {
        let mut h = fixtures::Harness::new(fixtures::config());
        ask(&mut h, &call);
        assert_eq!(h.status(), 404);
        assert_eq!(h.body().as_ref(), br#"{"error":"missing","what":"repository"}"#);
    }
    for push in [false, true] {
        let mut h = fixtures::Harness::new(fixtures::config());
        let name = repeated(h.fake_env.limits.limits.name_bytes.saturating_add(1), b'x');
        let call = if push {
            Call::Push { repository: boxed(b"org/repo"), branch: name, commit: 1, expected: None }
        } else {
            Call::Create { repository: boxed(b"org/repo"), branch: name, commit: 1 }
        };
        ask(&mut h, &call);
        assert_eq!(h.status(), 413);
        for branch in [b"one".as_slice(), b"two", b"three"] {
            h.branch(branch, 1);
        }
        let call = if push {
            Call::Push { repository: boxed(b"org/repo"), branch: boxed(b"full"), commit: 1, expected: None }
        } else {
            Call::Create { repository: boxed(b"org/repo"), branch: boxed(b"full"), commit: 1 }
        };
        ask(&mut h, &call);
        assert_eq!(h.status(), 413);
        assert_eq!(h.body().as_ref(), br#"{"error":"full"}"#);
    }
}
#[test]
fn git_ancestor_reads_both_parents_and_answers_false() {
    let mut h = fixtures::Harness::new(fixtures::config());
    history(&mut h);
    for (ancestor, commit, want) in
        [(1, 6, true), (2, 6, true), (3, 6, true), (6, 3, false), (2, 3, false), (6, 6, true)]
    {
        ask(&mut h, &Call::Ancestor { ancestor, commit });
        assert_eq!(answer(&h, Name::Ancestor), Answer::Ancestor(want));
    }
    assert_eq!(h.domain_calls(), 0);
}
