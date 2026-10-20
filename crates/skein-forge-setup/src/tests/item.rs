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
