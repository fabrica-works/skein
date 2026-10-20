use super::fixtures::ADMIN;
use crate::item;
#[test]
fn a_user_item_is_eight_bytes_little_endian_never_zero() {
    for id in [42, u64::MAX] {
        assert_eq!(item::user(&id.to_le_bytes()).expect("nonzero").get(), id);
    }
    for bytes in [b"".as_slice(), &[0; 8], &[1; 7], &[1; 9], b"42"] {
        assert_eq!(item::user(bytes), None);
    }
    assert_eq!(item::user(b"12345678").expect("nonzero").get(), u64::from_le_bytes(*b"12345678"));
    assert_ne!(item::user(b"12345678").expect("nonzero").get(), 12_345_678);
}
#[test]
fn a_token_item_is_visible_text_within_token_bytes() {
    for bytes in [ADMIN, &[b'a'; 64], b"!~"] {
        assert_eq!(item::token(bytes, 64), Some(bytes));
    }
    for bytes in [b"".as_slice(), &[b'a'; 65], b"a b", b"a\tb", b"a\nb", b"a\x7fb", b"a\x80b"] {
        assert_eq!(item::token(bytes, 64), None);
    }
}

#[test]
fn an_app_id_is_eight_bytes_never_zero() {
    for id in [1, 42, u64::MAX] {
        assert_eq!(item::app_id(&id.to_le_bytes()).expect("nonzero").get(), id);
    }
    for bytes in [b"".as_slice(), &[0; 8], &[1; 7], &[1; 9]] {
        assert_eq!(item::app_id(bytes), None);
    }
}

#[test]
fn an_app_slug_is_a_lowercase_name_within_its_bound() {
    for bytes in [b"a".as_slice(), b"0", b"a-9z", b"abc-"] {
        assert_eq!(item::app_slug(bytes, 4), Some(bytes));
    }
    for bytes in [b"".as_slice(), b"abcde", b"Abcd", b"-abc"] {
        assert_eq!(item::app_slug(bytes, 4), None);
    }
    assert_eq!(item::app_slug(b"a", 0), None);
}

#[test]
fn an_app_slug_that_could_leave_a_url_is_refused() {
    for byte in [b'/', b'?', b'#', b'%', b'.', b' ', 0, 0x80] {
        assert_eq!(item::app_slug(&[b'a', byte, b'b'], 3), None);
    }
}

#[test]
fn an_app_key_is_one_whole_der_sequence_within_its_bound() {
    use skein_lib::Writer;
    for (header, length) in [(b"\x30\x03".as_slice(), 3_usize), (b"\x30\x81\x80", 128), (b"\x30\x82\x01\x00", 256)] {
        let mut out = Writer::new(header.len().checked_add(length).expect("small fixture"));
        out.put(header).expect("header fits");
        for _ in 0..length {
            out.put(&[1]).expect("body fits");
        }
        let bytes = out.finish();
        let bound = u32::try_from(bytes.len()).expect("small fixture");
        assert_eq!(item::app_key(&bytes, bound), Some(bytes.as_ref()));
        assert_eq!(item::app_key(&bytes, bound.checked_sub(1).expect("header present")), None);
        assert_eq!(item::app_key(&bytes[..bytes.len().checked_sub(1).expect("header present")], bound), None);
    }
    assert_eq!(item::app_key(b"\x30\x00", 2), Some(b"\x30\x00".as_slice()));
    for bytes in [
        b"".as_slice(),
        b"\x30",
        b"\x30\x81",
        b"\x30\x82\x01",
        b"\x30\x03abcx",
        b"\x30\x81\x05abcde",
        b"\x30\x81\x7f",
        b"\x30\x82\x00\x80",
        b"\x30\x82\x00\x03abc",
        b"\x30\x82\x01\x01abc",
        b"\x30\x80",
        b"\x30\x83\x01\x00\x00",
        b"\x31\x03abc",
        b"\x30\x04abc",
    ] {
        assert_eq!(item::app_key(bytes, 1024), None);
    }
}

#[test]
fn accepted_ids_are_eight_bytes_each_ascending_and_distinct() {
    for ids in [[2, 1], [1, 1]] {
        assert_eq!(item::accepted(item::accepted_item(&ids).expose(), 2), None);
    }
    for bytes in [[1; 7].as_slice(), [1; 9].as_slice()] {
        assert_eq!(item::accepted(bytes, 2), None);
    }
    let encoded = item::accepted_item(&[1, 256, u64::MAX]);
    assert_eq!(item::accepted(encoded.expose(), 3).expect("ordered ids").as_ref(), &[1, 256, u64::MAX]);
}

#[test]
fn accepted_may_be_empty_and_is_bounded_by_installations() {
    for bound in [0, 2] {
        assert!(item::accepted(b"", bound).expect("empty set").is_empty());
    }
    let encoded = item::accepted_item(&[1, 2]);
    assert_eq!(item::accepted(encoded.expose(), 2).expect("at bound").as_ref(), &[1, 2]);
    assert_eq!(item::accepted(encoded.expose(), 1), None);
    assert_eq!(item::accepted(&1_u64.to_le_bytes(), 0), None);
}

#[test]
fn an_accepted_id_of_zero_is_refused() {
    assert_eq!(item::accepted(&0_u64.to_le_bytes(), 1), None);
    assert_eq!(item::accepted(item::accepted_item(&[0, 1]).expose(), 2), None);
    assert_eq!(item::accepted(item::accepted_item(&[1, 0]).expose(), 2), None);
}

#[test]
fn a_pasted_key_is_pem_text_within_twice_the_key_bound() {
    for bytes in [
        b"-----BEGIN RSA PRIVATE KEY-----\nYWJj\n-----END RSA PRIVATE KEY-----\n".as_slice(),
        b"-----BEGIN RSA PRIVATE KEY-----\r\nYWJj\r\n-----END RSA PRIVATE KEY-----\r\n",
        b" !~\r\n",
    ] {
        assert_eq!(item::pasted(bytes, 64), Some(bytes));
    }
    for bytes in [b"".as_slice(), b"a\x00b", b"a\x7fb", b"a\x80b", b"a\tb"] {
        assert_eq!(item::pasted(bytes, 64), None);
    }
    assert_eq!(item::pasted(b"12345678", 4), Some(b"12345678".as_slice()));
    assert_eq!(item::pasted(b"123456789", 4), None);
    assert_eq!(item::pasted(b"a", 0), None);
    assert_eq!(item::pasted(b"a", u32::MAX), Some(b"a".as_slice()));
}

#[test]
fn fits_asks_each_item_its_own_decoder() {
    use item::{Bounds, Item};
    let bounds = Bounds { token_bytes: 4, name_bytes: 4, key_bytes: 4, installations: 1 };
    let id = 1_u64.to_le_bytes();
    for (kind, valid, invalid) in [
        (Item::AdminToken, b"!~".as_slice(), b"a b".as_slice()),
        (Item::ApiToken, b"!~", b"a b"),
        (Item::GitToken, b"!~", b"a b"),
        (Item::WebhookSecret, b"!~", b"a b"),
        (Item::User, id.as_slice(), &[0; 8]),
        (Item::AppId, id.as_slice(), &[0; 8]),
        (Item::AppSlug, b"a-1", b"-a"),
        (Item::AppKey, b"\x30\x02ab", b"\x30\x01ab"),
        (Item::Accepted, id.as_slice(), &[0; 8]),
        (Item::Pasted, b"a b\r\n", b"a\0b"),
    ] {
        assert!(item::fits(kind, valid, &bounds), "{kind:?}");
        assert!(!item::fits(kind, invalid, &bounds), "{kind:?}");
    }
    let zero = Bounds { token_bytes: 0, name_bytes: 0, key_bytes: 0, installations: 0 };
    for (kind, bytes) in [
        (Item::AdminToken, b"a".as_slice()),
        (Item::ApiToken, b"a"),
        (Item::GitToken, b"a"),
        (Item::WebhookSecret, b"a"),
        (Item::AppSlug, b"a"),
        (Item::AppKey, b"\x30\x00"),
        (Item::Accepted, id.as_slice()),
        (Item::Pasted, b"a"),
    ] {
        assert!(!item::fits(kind, bytes, &zero), "{kind:?}");
    }
    for (kind, bytes) in [
        (Item::AdminToken, b"abcde".as_slice()),
        (Item::ApiToken, b"abcde"),
        (Item::GitToken, b"abcde"),
        (Item::WebhookSecret, b"abcde"),
        (Item::AppSlug, b"abcde"),
        (Item::AppKey, b"\x30\x03abc"),
        (Item::Pasted, b"123456789"),
    ] {
        assert!(!item::fits(kind, bytes, &bounds), "{kind:?}");
    }
    assert!(item::fits(Item::Accepted, b"", &zero));
    assert!(item::fits(Item::User, &id, &zero));
    assert!(item::fits(Item::AppId, &id, &zero));
}

#[test]
fn accepted_round_trips_through_its_encoder() {
    for ids in [b"".as_slice(), &1_u64.to_le_bytes(), &u64::MAX.to_le_bytes()] {
        let decoded = item::accepted(ids, 1).expect("valid item");
        assert_eq!(item::accepted_item(&decoded).expose(), ids);
    }
    let ids = [1, 256, u64::MAX];
    let encoded = item::accepted_item(&ids);
    assert_eq!(item::accepted(encoded.expose(), 3).expect("valid item").as_ref(), &ids);
}

#[test]
fn no_decoder_panics_on_any_short_input() {
    let alphabet = [0, 1, 0x30, 0x81, 0x82, 0xff];
    for length in 0_u32..=3 {
        for mut value in 0_usize..6_usize.pow(length) {
            let mut bytes = [0; 3];
            for byte in bytes.iter_mut().take(usize::try_from(length).expect("short length")) {
                *byte = alphabet[value.checked_rem(6).expect("nonzero divisor")];
                value = value.checked_div(6).expect("nonzero divisor");
            }
            let bytes = &bytes[..usize::try_from(length).expect("short length")];
            let _id = item::app_id(bytes);
            let _slug = item::app_slug(bytes, 3);
            let _key = item::app_key(bytes, 3);
            let _ids = item::accepted(bytes, 3);
            let _pem = item::pasted(bytes, 3);
        }
    }
}
