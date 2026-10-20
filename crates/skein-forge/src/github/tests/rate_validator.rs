use super::{Head, Wall, api, header, limits};

const RATE_HEADERS: [(&[u8], &[u8]); 3] =
    [(b"x-ratelimit-limit", b"5000"), (b"x-ratelimit-remaining", b"4999"), (b"x-ratelimit-reset", b"1792483260")];

#[test]
fn github_rate_maps_all_three_headers() {
    for order in [[0, 1, 2], [2, 0, 1], [1, 2, 0]] {
        let mut head = Head::new();
        for index in order {
            let (name, value) = RATE_HEADERS[index];
            header::field(&mut head, name, value, &limits());
        }
        assert_eq!(
            header::rate(&head),
            Some(api::Rate { limit: 5000, remaining: 4999, reset_at: Wall::from_nanos(1_792_483_260_000_000_000) })
        );
    }
}

#[test]
fn github_rate_is_none_when_one_header_is_absent() {
    for omitted in 0..RATE_HEADERS.len() {
        let mut head = Head::new();
        for (index, (name, value)) in RATE_HEADERS.iter().enumerate() {
            if index != omitted {
                header::field(&mut head, name, value, &limits());
            }
        }
        assert_eq!(header::rate(&head), None, "absent field {omitted}");
    }
}

#[test]
fn github_rate_is_none_for_a_jwt_call_head() {
    let mut head = Head::new();
    header::field(&mut head, b"Date", b"Tue, 20 Oct 2026 08:00:00 GMT", &limits());
    assert_eq!(header::rate(&head), None);
    assert_eq!(header::validator(&head), None);
}

#[test]
fn github_validator_maps_a_printable_etag() {
    let boundary = [b'a'; api::VALIDATOR_BYTES];
    for bytes in [b"\"example-app\"".as_slice(), boundary.as_slice()] {
        let mut head = Head::new();
        header::field(&mut head, b"ETag", bytes, &limits());
        let validator = header::validator(&head).expect("printable bounded tag");
        assert_eq!(validator.as_bytes(), bytes);
    }
}

#[test]
fn github_validator_drops_a_129_byte_etag() {
    let mut head = Head::new();
    header::field(&mut head, b"ETag", b"\"example-app\"", &limits());
    header::field(&mut head, b"ETag", &[b'a'; 129], &limits());
    assert_eq!(head.etag, None);
    assert_eq!(header::validator(&head), None);
}

#[test]
fn github_validator_keeps_a_weak_etag_as_sent() {
    let bytes = b"W/\"example-app\"";
    let mut head = Head::new();
    header::field(&mut head, b"ETag", bytes, &limits());
    assert_eq!(header::validator(&head).expect("weak printable tag").as_bytes(), bytes);
}

#[test]
fn github_rate_is_none_when_one_header_is_malformed() {
    for (malformed, (name, _)) in RATE_HEADERS.iter().enumerate() {
        let mut head = Head::new();
        for (name, value) in RATE_HEADERS {
            header::field(&mut head, name, value, &limits());
        }
        header::field(&mut head, name, b"invalid", &limits());
        assert_eq!(header::rate(&head), None, "malformed field {malformed}");
    }
}

#[test]
fn github_validator_drops_empty_and_nonprintable_etags() {
    for bytes in [b"".as_slice(), b"\"example-app\"\r\n", b"\x1f", b"\x7f", b"\x80"] {
        let mut head = Head::new();
        header::field(&mut head, b"ETag", bytes, &limits());
        assert_eq!(header::validator(&head), None, "{bytes:?}");
    }
}
