//! Hand-fed checks of each issuer entrance and encoder.
#![expect(clippy::disallowed_types, reason = "tests construct bounded vectors and text")]
#![expect(clippy::disallowed_macros, reason = "test tables use vec, format and matches")]
#![expect(clippy::disallowed_methods, reason = "test fixtures use ordinary Rust collection helpers")]

mod authorization;
mod bounds;
mod documents;
mod settings;
mod token;

use super::*;
use skein_heap::Counting;
use skein_lib::{Duration, Wall, bytes};

#[global_allocator]
static HEAP: Counting = Counting;

const VERIFIER: &[u8] = b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const CHALLENGE: &[u8; 43] = b"E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const REDIRECT: &[u8] = b"https://app.example.com/callback";
const QUERY: &[u8] = b"client_id=client&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcallback&response_type=code&scope=openid&code_challenge_method=S256&code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const BODY: &[u8] = b"grant_type=authorization_code&client_id=client&client_secret=secret&code=code&redirect_uri=https%3A%2F%2Fapp.example.com%2Fcallback&code_verifier=dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
fn boxed(input: &[u8]) -> Box<[u8]> {
    bytes::copy_of(input)
}
fn secret(input: &[u8]) -> Secret {
    Secret::new(boxed(input))
}

// RFC 7515 Appendix A.2 and RFC 9500 section 2.1 testRSA2048:
// public test keys, never secrets. This is this crate's only key seam.
fn test_key(second: bool) -> Secret {
    let file =
        if second { "rfc9500-2.1-testRSA2048-public-test-key.pk8.der" } else { "rfc7515-a2-public-test-key.pk8.der" };
    let path = std::path::Path::new("src/tests/fixtures").join(file);
    Secret::new(std::fs::read(path).expect("published PKCS#8 test key").into_boxed_slice())
}
fn limits() -> Limits {
    Limits {
        clients: 2,
        uri_bytes: 256,
        client_bytes: 64,
        request_bytes: 2048,
        state_bytes: 64,
        token_bytes: 64,
        claim_bytes: 64,
    }
}
fn settings() -> Settings {
    let mut clients = List::with_capacity(2);
    clients
        .push(Client { id: boxed(b"client"), secret: secret(b"secret"), redirect_uri: boxed(REDIRECT) })
        .expect("client slot");
    Settings {
        issuer: boxed(b"https://issuer.example"),
        authorization_endpoint: boxed(b"https://issuer.example/authorize"),
        token_endpoint: boxed(b"https://issuer.example/token"),
        userinfo_endpoint: boxed(b"https://issuer.example/userinfo"),
        jwks_uri: boxed(b"https://issuer.example/keys"),
        clients,
        key: test_key(false),
    }
}
fn issuer() -> Issuer {
    Issuer::new(settings(), limits()).expect("issuer")
}
fn authorization() -> Authorization {
    issuer().authorize(QUERY).expect("authorization")
}
fn person() -> Person {
    Person {
        subject: boxed(b"person"),
        username: Some(boxed(b"username")),
        email: Some(Email { address: boxed(b"a@example.com"), verified: true }),
        name: Some(boxed(b"A Person")),
    }
}
fn wall() -> Wall {
    Wall::from_nanos(100_000_000_000)
}
fn replace(input: &[u8], from: &str, to: &str) -> Box<[u8]> {
    String::from_utf8(input.to_vec()).expect("ASCII fixture").replace(from, to).into_bytes().into_boxed_slice()
}
fn back(error: &[u8], state: Option<&[u8]>) -> Refusal {
    Refusal::Back { location: form::location(REDIRECT, b"error", error, state).expect("location") }
}
