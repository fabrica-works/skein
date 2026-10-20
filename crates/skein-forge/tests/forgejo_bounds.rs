#![cfg(test)]
#![expect(clippy::disallowed_macros, reason = "the boundary harness formats bounded nesting fixtures")]
pub mod common;
use common::{admitted, forgejo, generated};
use skein_forge::forgejo::{
    client,
    decode::Malformed,
    document::{Body, Comment, Document, User},
    limits::{self, Limits, Problem},
    request::{Method, Request},
    server,
};
use skein_json::{tokenizer, writer};

// The focused fixture's admitted bounds come from its shared source.
const STRING: u32 = admitted::SMALL_STRING_BYTES;
const BODY: u32 = admitted::SMALL_BODY_BYTES;
const INNER: u32 = 2;
const MIN_DEPTH: u32 = 8;
const SEEDS: [u64; 3] = [0, 7, 31];
const ONE_TERMINAL: usize = 1;
const EMPTY_ROWS: &[skein_forge::forgejo::document::Row] = &[];

#[test]
#[expect(clippy::type_complexity, reason = "the reference table pairs each named limit with its mutator")]
fn limits_accept_the_profile_and_reject_each_problem() {
    let good = forgejo::profile_limits();
    assert_eq!(limits::check(&good), Ok(()));
    let zeros: [(&str, fn(&mut Limits)); 8] = [
        ("string_bytes", |l| l.string_bytes = 0),
        ("body_bytes", |l| l.body_bytes = 0),
        ("inner", |l| l.inner = 0),
        ("target_bytes", |l| l.target_bytes = 0),
        ("chunk", |l| l.json.chunk = 0),
        ("number", |l| l.json.number = 0),
        ("json.length", |l| l.json.length = 0),
        ("writer.length", |l| l.writer.length = 0),
    ];
    for (field, zero) in zeros {
        let mut value = good;
        zero(&mut value);
        assert_eq!(limits::check(&value), Err(Problem::Zero), "{field}");
    }
    for (field, bound) in [("string", good.string_bytes), ("body", good.body_bytes)] {
        let mut value = good;
        value.json.string = bound - 1;
        assert_eq!(limits::check(&value), Err(Problem::TokenizerString), "{field}");
    }
    let depths: [(&str, fn(&mut Limits)); 2] = [
        ("json", |l: &mut Limits| l.json.depth = MIN_DEPTH - 1),
        ("writer", |l: &mut Limits| l.writer.depth = MIN_DEPTH - 1),
    ];
    for (field, change) in depths {
        let mut value = good;
        change(&mut value);
        assert_eq!(limits::check(&value), Err(Problem::Depth), "{field}");
    }
}
fn small() -> Limits {
    Limits { string_bytes: STRING, body_bytes: BODY, inner: INNER, ..admitted::small() }
}
fn user(login: &[u8]) -> User {
    admitted::user(1, login)
}
fn answer(request: &Request, body: &[u8], limits: &Limits) -> forgejo::Decoded {
    forgejo::decode_answer(client::decoder(request, limits).expect("JSON answer"), body, limits)
}
#[test]
fn document_bounds_hold_at_the_limit_and_one_past() {
    let limits = small();
    for (name, body, expected) in [
        ("login at string_bytes", br#"{"id":1,"login":"12345678"}"#.as_slice(), Ok(Document::User(user(b"12345678")))),
        (
            "login past string_bytes",
            br#"{"id":1,"login":"123456789"}"#.as_slice(),
            Err(Malformed::TooLong { field: b"login" }),
        ),
    ] {
        assert_eq!(answer(&Request::User, body, &limits).document, expected, "{name}");
    }
    for (name, body, expected) in [
        (
            "body at body_bytes",
            br#"{"id":1,"user":{"id":1,"login":"fixture"},"body":"1234567890123456"}"#.as_slice(),
            Body::Kept(Box::from(*b"1234567890123456")),
        ),
        (
            "body past body_bytes",
            br#"{"id":1,"user":{"id":1,"login":"fixture"},"body":"12345678901234567"}"#.as_slice(),
            Body::Long(u64::from(BODY + 1)),
        ),
    ] {
        assert_eq!(
            answer(&Request::PostComment { at: forgejo::specimen(), number: 1, body: Box::from([]) }, body, &limits)
                .document,
            Ok(Document::Comment(Comment {
                id: 1,
                user: user(b"fixture"),
                created_at: None,
                updated_at: None,
                body: expected
            })),
            "{name}"
        );
    }
    let scopes: Box<[Box<[u8]>]> = Box::from([Box::from(*b"read"), Box::from(*b"read")]);
    let accepted = Request::CreateToken { login: Box::from(*b"bot"), name: Box::from(*b"kit-api"), scopes };
    for (name, body, expected) in [
        ("scopes at inner", br#"{"name":"kit-api","scopes":["read","read"]}"#.as_slice(), Ok(accepted)),
        (
            "scopes past inner",
            br#"{"name":"kit-api","scopes":["read","read","read"]}"#.as_slice(),
            Err(Malformed::Bound { field: b"scopes" }),
        ),
    ] {
        let result = admitted_request(body, &limits);
        assert_eq!(result, expected, "{name}");
    }
}
fn admitted_request(body: &[u8], limits: &Limits) -> Result<Request, Malformed> {
    // Decode through the shared request driver to preserve structured refusals.
    let target = b"/api/v1/admin/users/bot/tokens";
    if let Ok((request, _)) = forgejo::decode_route(Method::Post, target, body, limits) {
        return Ok(request);
    }
    Err(forgejo::decode_refusal(Method::Post, target, body, limits))
}
#[test]
fn stack_depth_accepts_the_limit_and_refuses_one_past() {
    let mut limits = small();
    for (name, depth, expected) in [
        ("at depth", limits.json.depth, Ok(Document::User(user(b"fixture")))),
        ("past depth", limits.json.depth + 1, Err(Malformed::Json(tokenizer::Error::TooDeep))),
    ] {
        // The user object is one container; unknown arrays supply the remaining levels.
        let arrays = usize::try_from(depth - 1).expect("fixture depth");
        let body = format!(r#"{{"id":1,"login":"fixture","unknown":{}0{}}}"#, "[".repeat(arrays), "]".repeat(arrays));
        assert_eq!(answer(&Request::User, body.as_bytes(), &limits).document, expected, "{name}");
    }
    limits.writer.depth = MIN_DEPTH;
    assert_eq!(limits::check(&limits), Ok(()));
    assert_eq!(
        server::encode(&server::Answer::User(user(b"fixture")), &limits),
        Ok(Box::from(br#"{"id":1,"login":"fixture","is_admin":false,"full_name":""}"#.as_slice()))
    );
    limits.writer.depth = MIN_DEPTH - 1;
    assert_eq!(limits::check(&limits), Err(Problem::Depth));
}
#[test]
fn strings_cut_at_every_byte_decode_once() {
    let body = r#"{"id":1,"login":"kité\"\uD834\uDD1E"}"#.as_bytes();
    let limits = admitted::limits();
    let expected = Document::User(user("kité\"𝄞".as_bytes()));
    for split in 0..=body.len() {
        let (decoded, terminals) = forgejo::decode_split(&Request::User, body, split, &limits);
        assert_eq!(decoded.document, Ok(expected.clone()), "split {split}");
        assert_eq!(decoded.rows, EMPTY_ROWS, "split {split}");
        assert_eq!(terminals, ONE_TERMINAL, "split {split}");
    }
    for end in 0..body.len() {
        let decoded = answer(&Request::User, &body[..end], &limits);
        assert_eq!(decoded.document, Err(Malformed::Json(tokenizer::Error::Truncated)), "prefix {end}");
    }
}

#[test]
fn seeded_documents_round_trip_on_both_sides() {
    let limits = generated::limits();
    for (name, request) in generated::request_documents() {
        let encoded = client::encode(&request, &limits).expect("generated request");
        let Some(body) = encoded.body.as_deref() else {
            continue;
        };
        assert_eq!(
            forgejo::decode_route(encoded.method, &encoded.target, body, &limits).expect("request decoding").0,
            request,
            "{name}"
        );
    }
    for seed in SEEDS {
        for reply in generated::documents(seed) {
            let bytes = server::encode(&reply.answer, &limits).expect("generated answer");
            let reader = match &reply.request {
                Some(request) => client::decoder(request, &limits).expect("answer decoder"),
                None => client::error_decoder(&limits),
            };
            let decoded = forgejo::decode_answer(reader, &bytes, &limits);
            assert_eq!(decoded.document, Ok(reply.document), "seed {seed}/{}", reply.name);
            assert_eq!(decoded.rows, reply.rows, "seed {seed}/{}", reply.name);
        }
        for (name, delivery) in generated::deliveries(seed) {
            let sent = server::delivery::encode(&delivery, &limits).expect("generated delivery");
            assert_eq!(forgejo::decode_delivery(sent.event, &sent.body, &limits), Ok(delivery), "seed {seed}/{name}");
        }
    }
}

#[test]
fn each_encoder_has_golden_bytes_and_a_measured_bound() {
    use common::goldens;
    let good = generated::limits();
    for (name, request) in generated::requests() {
        let (method, target, body) = goldens::request(name);
        let mut limits = good;
        limits.target_bytes = u32::try_from(target.len()).expect("target size");
        // A request without a body does not consume writer room.
        limits.writer.length = u32::try_from(body.len()).expect("body size");
        let encoded = client::encode(&request, &limits).expect("exact request room");
        assert_eq!(encoded.method, method, "{name}");
        assert_eq!(encoded.target.as_ref(), target, "{name}");
        assert_eq!(encoded.body.as_deref().unwrap_or_default(), body, "{name}");
        limits.target_bytes -= 1;
        assert_eq!(client::encode(&request, &limits), Err(client::Refusal::TooLong), "{name}/target");
        if !body.is_empty() {
            limits.target_bytes += 1;
            limits.writer.length -= 1;
            assert_eq!(client::encode(&request, &limits), Err(client::Refusal::TooLong), "{name}/writer");
        }
    }
    for reply in generated::replies(0) {
        let expected = goldens::answer(reply.name);
        let mut limits = good;
        limits.writer.length = u32::try_from(expected.len()).expect("answer size");
        assert_eq!(
            server::encode(&reply.answer, &limits).expect("exact answer room").as_ref(),
            expected,
            "{}",
            reply.name
        );
        limits.writer.length -= 1;
        assert_eq!(server::encode(&reply.answer, &limits), Err(writer::Refusal::TooLong), "{}", reply.name);
    }
    for (name, delivery) in forgejo::delivery_cases() {
        let expected = goldens::delivery(name);
        let mut limits = good;
        limits.writer.length = u32::try_from(expected.len()).expect("delivery size");
        let sent = server::delivery::encode(&delivery, &limits).expect("exact delivery room");
        assert_eq!(sent.body.as_ref(), expected, "{name}");
        limits.writer.length -= 1;
        assert_eq!(server::delivery::encode(&delivery, &limits), Err(writer::Refusal::TooLong), "{name}");
    }
}
