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
    let mut decoder = client::decoder(name, status, Time::ZERO, limits);
    let mut pump = Pump::new(body, limits, piece);
    let mut demand = client::first(&decoder);
    loop {
        match client::step(&mut decoder, limits, &pump.next(demand)) {
            client::Step::Demand(next) => demand = next,
            outcome @ (client::Step::Decoded(_) | client::Step::Malformed) => return outcome,
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
                decode_answer(&body, name, 200, &limits, piece),
                client::Step::Decoded(Ok(answer.clone())),
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
        for piece in [usize::MAX, 1] {
            assert_eq!(
                decode_answer(&body, Name::Fetch, 422, &limits, piece),
                client::Step::Decoded(Err(error)),
                "{error:?}/{piece}"
            );
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
