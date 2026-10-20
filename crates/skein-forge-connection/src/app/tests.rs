use alloc::boxed::Box;
use skein_fake_forge_github::keys::KEYS;
use skein_forge::github;
use skein_lib::{Duration, Secret, Wall};

use super::Jwt;

const APP: u64 = 7;
const DEADLINE: Duration = Duration::from_secs(10);
// forge-github.md, 8.1: the 256-byte signature's unpadded base64url.
const SIGNATURE_BYTES: usize = 342;

fn key() -> Secret {
    Secret::new(Box::from(KEYS[0]))
}

fn at(secs: u64) -> Wall {
    Wall::from_nanos(secs.checked_mul(1_000_000_000).expect("fixture seconds fit nanoseconds"))
}

fn input(issued: u64, expires: u64) -> Box<[u8]> {
    github::jwt::signing_input(APP, at(issued), at(expires)).expect("valid fixture life")
}

fn held(bytes: &[u8], expires: u64) -> Jwt {
    Jwt { token: Some(Secret::new(Box::from(bytes))), expires: at(expires) }
}

fn is_url_safe(bytes: &[u8]) -> bool {
    for byte in bytes {
        if !byte.is_ascii_alphanumeric() && *byte != b'-' && *byte != b'_' {
            return false;
        }
    }
    true
}

#[test]
fn a_bearer_is_signed_a_minute_back_for_ten_minutes() {
    let mut jwt = Jwt::new();
    let token = jwt.bearer(APP, &key(), at(1_200), DEADLINE).expect("fixture key signs");
    let input = input(1_140, 1_740);
    let bytes = token.expose();
    assert!(bytes.starts_with(&input));
    assert_eq!(bytes[input.len()], b'.');
    let signature = &bytes[input.len() + 1..];
    assert_eq!(signature.len(), SIGNATURE_BYTES);
    assert!(is_url_safe(signature));
    assert_eq!(token.len(), input.len() + 1 + SIGNATURE_BYTES);
    let signed = Box::<[u8]>::from(bytes);
    assert_eq!(jwt.token.as_ref().expect("signed token held").expose(), signed.as_ref());
    assert_eq!(jwt.expires, at(1_740));
}

#[test]
fn a_bearer_is_reused_while_its_life_is_not_shorter_than_the_deadline() {
    let mut jwt = Jwt::new();
    let token = jwt.bearer(APP, &key(), at(1_200), DEADLINE).expect("fixture key signs");
    let signed = Box::<[u8]>::from(token.expose());
    let token = jwt.bearer(APP, &key(), at(1_730), DEADLINE).expect("held token fresh");
    assert_eq!(token.expose(), signed.as_ref());
    assert_eq!(jwt.expires, at(1_740));
}

#[test]
fn a_bearer_is_signed_again_when_less_than_the_deadline_is_left() {
    let mut jwt = held(b"old", 1_740);
    let token = jwt.bearer(APP, &key(), at(1_731), DEADLINE).expect("fixture key signs");
    assert!(token.expose().starts_with(&input(1_671, 2_271)));
    assert_ne!(token.expose(), b"old");
    assert_eq!(jwt.expires, at(2_271));
}

#[test]
fn a_bearer_near_the_epoch_is_issued_at_zero() {
    let mut jwt = Jwt::new();
    let token = jwt.bearer(APP, &key(), at(30), DEADLINE).expect("fixture key signs");
    assert!(token.expose().starts_with(&input(0, 600)));
    assert_eq!(jwt.expires, at(600));
}

#[test]
fn a_key_that_will_not_sign_gives_no_bearer_and_keeps_no_token() {
    let mut jwt = held(b"old", 1_740);
    let key = Secret::new(Box::from([0_u8; 64]));
    assert!(jwt.bearer(APP, &key, at(1_800), DEADLINE).is_none());
    assert!(jwt.token.is_none());
    assert_eq!(jwt.expires, Wall::EPOCH);
    assert!(!jwt.fresh(at(1_800), DEADLINE));
}

#[test]
fn fresh_is_decided_by_the_life_left() {
    let jwt = held(b"t", 1_740);
    let cases = [
        (at(1_200), true),
        (at(1_730), true),
        (Wall::from_nanos(at(1_730).as_nanos() + 1), false),
        (at(1_740), false),
        (at(1_800), false),
    ];
    for (wall, fresh) in cases {
        assert_eq!(jwt.fresh(wall, DEADLINE), fresh, "wall {wall:?}");
    }
    assert!(!Jwt::new().fresh(at(0), DEADLINE));
}

#[test]
fn forgetting_drops_the_token() {
    let mut jwt = held(b"t", 1_740);
    jwt.forget();
    assert!(jwt.token.is_none());
    assert_eq!(jwt.expires, Wall::EPOCH);
    assert!(!jwt.fresh(at(1_200), DEADLINE));
}

#[test]
#[expect(clippy::disallowed_macros, reason = "test checks the secret's debug view")]
fn the_debug_view_holds_no_token_bytes() {
    let jwt = held(b"jwt-secret-bytes", 1_740);
    assert!(!alloc::format!("{jwt:?}").contains("jwt-secret"));
}
