use super::*;
use skein_json::writer::Encoder;
use skein_lib::{Writer, base64};

const MIN_BITS: u32 = 2048; // oauth.md, section 2.2.
const MAX_BITS: u32 = 4096; // limits().modulus_bits.
const KEY_CAP: u32 = 4; // limits().keys.
const METADATA: &[u8] = br#"{"issuer":"https://issuer.example","jwks_uri":"https://issuer.example/keys"}"#;

fn registration_for_keys(direct: bool) -> Registration {
    let mut registration = registration(true, WireFormat::Form);
    registration.scope = boxed(b"openid read");
    let mut additions = terms::terms(true, false);
    if let Some(openid) = &mut additions.openid {
        openid.key_set = if direct { Some(boxed(b"https://issuer.example/keys")) } else { None };
    }
    registration.terms = Some(additions);
    registration
}
fn sign_in(registration: Registration) -> Event {
    Event::SignIn {
        registration,
        key: 7,
        generation: 0,
        state: state(),
        verifier: Some(verifier()),
        nonce: Some(boxed(b"0123456789abcdef")),
        port: None,
        issued_client_id: None,
        id_token: None,
        subject: None,
        now: time(1),
    }
}
fn fetching(direct: bool) -> (Client, HttpRequest) {
    let mut client = Client::new(limits()).expect("client");
    let get = request(step(&mut client, sign_in(registration_for_keys(direct))));
    (client, get)
}
fn encoded(raw: &[u8]) -> Box<[u8]> {
    let length =
        base64::encoded_len(base64::Alphabet::Url, u32::try_from(raw.len()).expect("fixture")).expect("fixture");
    let mut out = Writer::new(usize::try_from(length).expect("fixture"));
    base64::encode(base64::Alphabet::Url, raw, &mut out).expect("fixture");
    out.finish()
}
fn modulus(bits: u32, leading_zero: bool) -> Box<[u8]> {
    const MAX_BYTES: usize = 514; // One byte beyond 4096 bits, plus a leading zero.
    let count = usize::try_from(bits.div_ceil(8)).expect("fixture");
    let mut raw = [0xff; MAX_BYTES];
    let offset = usize::from(leading_zero);
    if leading_zero {
        *raw.first_mut().expect("fixture") = 0;
    }
    *raw.get_mut(offset).expect("fixture") = 0xff_u8.checked_shr((8_u32.wrapping_sub(bits % 8)) % 8).expect("fixture");
    encoded(raw.get(..count.checked_add(offset).expect("fixture")).expect("fixture"))
}
fn write_keys(out: &mut Encoder, n: &[u8], kty: &[u8], use_: Option<&[u8]>, count: u32) {
    out.object_start();
    out.key(b"keys");
    out.array_start();
    for _ in 0..count {
        out.object_start();
        for (name, value) in [(b"kty".as_slice(), kty), (b"n", n), (b"e", b"AQAB"), (b"kid", b"test")] {
            out.key(name);
            out.string(value);
        }
        if let Some(value) = use_ {
            out.key(b"use");
            out.string(value);
        }
        out.object_end();
    }
    out.array_end();
    out.object_end();
}
fn key_set(bits: u32, leading_zero: bool, kty: &[u8], use_: Option<&[u8]>, count: u32) -> Box<[u8]> {
    let n = modulus(bits, leading_zero);
    let bounds = limits().document.writer_limits();
    let mut measure = Encoder::measure(&bounds);
    write_keys(&mut measure, &n, kty, use_, count);
    let mut out = Encoder::write(measure.measured().expect("fixture"), &bounds);
    write_keys(&mut out, &n, kty, use_, count);
    out.finish()
}
pub(super) fn one_key() -> Box<[u8]> {
    key_set(MIN_BITS, false, b"RSA", Some(b"sig"), 1)
}

#[test]
fn an_openid_sign_in_fetches_its_keys_before_the_visit() {
    for (direct, endpoint) in [
        (true, b"https://issuer.example/keys".as_slice()),
        (false, b"https://issuer.example/.well-known/openid-configuration"),
    ] {
        let (mut client, get) = fetching(direct);
        let first_endpoint = get.endpoint.clone();
        let result = if direct {
            step(&mut client, answered(get.id, 200, &one_key(), 2))
        } else {
            let keys = request(step(&mut client, answered(get.id, 200, METADATA, 2)));
            step(&mut client, answered(keys.id, 200, &one_key(), 3))
        };
        let visit = is_visit(result);
        assert_eq!((first_endpoint.as_ref(), visit), (endpoint, true), "direct {direct}");
    }
}
#[test]
fn a_get_carries_no_body_or_content_type() {
    let (_, get) = fetching(true);
    assert_eq!(get_shape(&get), GetShape { method: Method::Get, content_type: 0, body: 0 });
}
#[test]
fn a_non_openid_sign_in_visits_at_once() {
    let mut client = Client::new(limits()).expect("client");
    let mut event = sign_in(registration(true, WireFormat::Form));
    if let Event::SignIn { nonce, .. } = &mut event {
        *nonce = None;
    }
    let visit = is_visit(step(&mut client, event));
    assert!(visit);
}
#[test]
fn the_key_set_keeps_only_keys_that_fit() {
    for (name, bits, zero, kty, use_, count, expected) in [
        ("EC", MIN_BITS, false, b"EC".as_slice(), Some(b"sig".as_slice()), 1, None),
        ("enc", MIN_BITS, false, b"RSA", Some(b"enc"), 1, None),
        ("no use", MIN_BITS, false, b"RSA", None, 1, Some((1, 256))),
        ("short", MIN_BITS - 1, false, b"RSA", None, 1, None),
        ("minimum", MIN_BITS, false, b"RSA", None, 1, Some((1, 256))),
        ("maximum", MAX_BITS, false, b"RSA", None, 1, Some((1, 512))),
        ("long", MAX_BITS + 1, false, b"RSA", None, 1, None),
        ("leading zero", MIN_BITS, true, b"RSA", None, 1, Some((1, 256))),
        ("key cap", MIN_BITS, false, b"RSA", None, KEY_CAP, Some((4, 256))),
        ("past key cap", MIN_BITS, false, b"RSA", None, KEY_CAP + 1, None),
    ] {
        let got = match keys::fitting_keys(&key_set(bits, zero, kty, use_, count), &limits()) {
            Ok(keys) => Some((keys.len(), keys.first().expect("kept key").modulus.len())),
            Err(_) => None,
        };
        assert_eq!(got, expected, "{name}");
    }
}

struct FetchRow {
    name: &'static str,
    direct: bool,
    body: Box<[u8]>,
    status: u16,
    now: u64,
    attempts: u32,
}
fn failed_fetch(row: FetchRow) -> Option<Request> {
    let mut bounds = limits();
    bounds.max_attempts = row.attempts;
    let mut client = Client::new(bounds).expect("client");
    let get = request(step(&mut client, sign_in(registration_for_keys(row.direct))));
    step(&mut client, answered(get.id, row.status, &row.body, row.now))
}
#[test]
fn each_keys_failure_is_unverified_before_any_visit() {
    const METADATA_CAP: usize = 1024; // limits().metadata_bytes.
    const SET_CAP: usize = 4096; // limits().key_set_bytes.
    for row in [
        FetchRow { name: "unreadable metadata", direct: false, body: boxed(b"bad"), status: 200, now: 2, attempts: 3 },
        FetchRow { name: "unreadable keys", direct: true, body: boxed(b"bad"), status: 200, now: 2, attempts: 3 },
        FetchRow {
            name: "metadata cap",
            direct: false,
            body: boxed(&[b' '; METADATA_CAP + 1]),
            status: 200,
            now: 2,
            attempts: 3,
        },
        FetchRow {
            name: "keys cap",
            direct: true,
            body: boxed(&[b' '; SET_CAP + 1]),
            status: 200,
            now: 2,
            attempts: 3,
        },
        FetchRow {
            name: "issuer",
            direct: false,
            body: boxed(br#"{"issuer":"https://other.example","jwks_uri":"https://issuer.example/keys"}"#),
            status: 200,
            now: 2,
            attempts: 3,
        },
        FetchRow {
            name: "http off loopback",
            direct: false,
            body: boxed(br#"{"issuer":"https://issuer.example","jwks_uri":"http://issuer.example/keys"}"#),
            status: 200,
            now: 2,
            attempts: 3,
        },
        FetchRow {
            name: "no fitting key",
            direct: true,
            body: boxed(br#"{"keys":[]}"#),
            status: 200,
            now: 2,
            attempts: 3,
        },
        FetchRow { name: "not found", direct: true, body: boxed(b""), status: 404, now: 2, attempts: 3 },
        FetchRow { name: "budget", direct: true, body: boxed(b""), status: 503, now: 2, attempts: 1 },
        FetchRow { name: "deadline", direct: true, body: one_key(), status: 200, now: 121, attempts: 3 },
    ] {
        let name = row.name;
        assert_eq!(failure(failed_fetch(row)), Failure::Unverified { check: Check::Keys }, "{name}");
    }
}
#[test]
fn a_get_is_retried_after_an_ambiguous_send() {
    for (evidence, status) in [(HttpEvidence::Unknown, 0), (HttpEvidence::Response, 503)] {
        let (mut client, get) = fetching(true);
        let answer = HttpResponse {
            id: get.id,
            status,
            body: Box::default(),
            retry_after: Duration::ZERO,
            evidence,
            now: time(2),
            wall: wall(100),
        };
        let pending = step(&mut client, Event::Http(answer));
        let retry = request(step(&mut client, Event::Tick { now: time(4) }));
        let terminal = step(&mut client, answered(retry.id, 200, &one_key(), 5));
        let visited = is_visit(terminal);
        assert_eq!(
            RetryOutcome {
                pending: pending.is_none(),
                method: retry.method,
                same_endpoint: retry.endpoint == get.endpoint,
                visited
            },
            RetryOutcome { pending: true, method: Method::Get, same_endpoint: true, visited: true },
            "{evidence:?}"
        );
    }
}
#[test]
fn cancel_while_fetching_keys_is_cancelled() {
    let (mut client, _) = fetching(true);
    assert_eq!(failure(step(&mut client, Event::Cancel)), Failure::Cancelled);
}
#[test]
fn the_key_caps_are_admitted_where_the_design_allows() {
    for (bits, accepted) in [(2047, false), (2048, true), (8192, true), (8193, false)] {
        let mut bounds = limits();
        bounds.modulus_bits = bits;
        assert_eq!(Client::new(bounds).is_ok(), accepted, "bits {bits}");
    }
    for (direct, metadata, keys, count, expected) in [
        (false, 0, 4096, 4, Err(Failure::Limit)),
        (true, 0, 4096, 4, Ok(())),
        (false, 1024, 0, 4, Err(Failure::Limit)),
        (true, 1024, 0, 4, Err(Failure::Limit)),
        (false, 1024, 4096, 0, Err(Failure::Limit)),
        (true, 1024, 4096, 0, Err(Failure::Limit)),
    ] {
        let mut bounds = limits();
        bounds.metadata_bytes = metadata;
        bounds.key_set_bytes = keys;
        bounds.keys = count;
        assert_eq!(
            validate_registration(&registration_for_keys(direct), &bounds, true),
            expected,
            "direct {direct}, caps {metadata} {keys} {count}"
        );
    }
}

fn is_visit(result: Option<Request>) -> bool {
    match result {
        Some(Request::Visit { .. }) => true,
        Some(Request::Http(_) | Request::Tokens { .. } | Request::Revoked { .. } | Request::Failed { .. }) | None => {
            false
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
struct GetShape {
    method: Method,
    content_type: usize,
    body: usize,
}
fn get_shape(get: &HttpRequest) -> GetShape {
    GetShape { method: get.method, content_type: get.content_type.len(), body: get.body.len() }
}
#[derive(Debug, PartialEq, Eq)]
struct RetryOutcome {
    pending: bool,
    method: Method,
    same_endpoint: bool,
    visited: bool,
}
