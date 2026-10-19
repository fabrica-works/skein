//! Contracts at the owner's boundary (forge-client.md, sections 2 and 7).
use super::{LIMITS, configured};
use crate::{Domain, Limits};

const KEY_BYTES: u32 = 7; // version + two length bytes + kit + one identity byte
const DEPLOYMENT: [u8; 16] = [7; 16];

#[test]
fn keys_frame_the_namespace_and_opaque_identity() {
    let key = crate::effect_key(b"kit", b"a", KEY_BYTES);
    assert_eq!(key.as_deref(), Some(b"\x01\x00\x03kita".as_slice()));
}

#[test]
fn invalid_key_frames_are_refused() {
    const OVERLONG: usize = 65536; // one past the u16 namespace length
    let long = skein_lib::bytes::zeroed(OVERLONG);
    for (name, namespace, identity, max) in [
        ("empty namespace", b"".as_slice(), b"a".as_slice(), KEY_BYTES),
        ("long namespace", long.as_ref(), b"a", u32::MAX),
        ("empty identity", b"kit", b"", KEY_BYTES),
        ("short cap", b"kit", b"a", KEY_BYTES - 1),
    ] {
        assert_eq!(crate::effect_key(namespace, identity, max), None, "{name}");
    }
}

#[test]
fn the_largest_namespace_keeps_its_identity() {
    const REPEATS: usize = 21845; // 65535 bytes / kit's three bytes
    const MAX_KEY: u32 = 65539; // version + length + largest namespace + identity
    let mut namespace = skein_lib::List::with_capacity(65535);
    for _ in 0..REPEATS {
        for byte in b"kit" {
            namespace.push(*byte).expect("largest namespace room");
        }
    }
    let key = crate::effect_key(namespace.as_slice(), b"a", MAX_KEY).expect("largest namespace fits");
    assert_eq!(key.len(), 65539);
    assert_eq!(key.get(..3), Some([1, 255, 255].as_slice()));
    assert_eq!(key.last(), Some(&b'a'));
}

#[test]
fn opaque_identities_are_stable_and_distinct() {
    let first = crate::effect_key(b"kit", b"a", KEY_BYTES);
    assert_eq!(first.as_deref(), Some(b"\x01\x00\x03kita".as_slice()));
    assert_eq!(first, crate::effect_key(b"kit", b"a", KEY_BYTES));
    assert_ne!(first, crate::effect_key(b"kit", b"b", KEY_BYTES));
    assert_ne!(first, crate::effect_key(b"b", b"a", KEY_BYTES));
}

#[test]
fn binding_replaces_the_namespace_before_restore() {
    let mut d = configured(&LIMITS);
    assert!(d.bind_deployment(DEPLOYMENT, &LIMITS));
    let key = crate::effect_key(&DEPLOYMENT, b"a", LIMITS.op_bytes).expect("deployment frame fits");
    assert!(crate::identity::historical(&d, &key));
}

#[test]
fn binding_without_configuration_changes_nothing() {
    let mut d = Domain::new(&LIMITS);
    assert!(!d.bind_deployment(DEPLOYMENT, &LIMITS));
    assert_eq!(d.config, None);
}

#[test]
fn binding_past_op_bytes_preserves_the_namespace() {
    const KIT_BYTES: u32 = 3; // namespace kit, smaller than the deployment id
    let mut d = configured(&LIMITS);
    assert!(!d.bind_deployment(DEPLOYMENT, &Limits { op_bytes: KIT_BYTES, ..LIMITS }));
    assert_eq!(d.config.expect("configured").namespace.as_ref(), b"kit");
}
