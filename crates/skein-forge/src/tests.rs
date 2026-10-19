use crate::api::Answer;

#[test]
fn locate_bytes_counts_owner_and_name() {
    assert_eq!(crate::locate::bytes(b"ab", b"cde"), Some(5));
}

#[test]
fn locate_accepts_only_nonzero_repository() {
    for (answer, expected) in
        [(Answer::Located { repository: 7 }, true), (Answer::Located { repository: 0 }, false), (Answer::Done, false)]
    {
        assert_eq!(crate::locate::accepts(&answer), expected, "{answer:?}");
    }
}

use crate::api;
use crate::{
    forgejo::{plans::tables::Tables, tests::fixtures},
    marker,
};
use alloc::boxed::Box;
const WORD: &[u8] = b"kit";

#[test]
fn a_marker_round_trips_its_key_and_keeps_the_text() {
    let sixty_four = [0xab; 64];
    let rows: [(&str, &[u8], &[u8]); 4] = [
        ("empty key", b"", b"text"),
        ("one byte", &[0x01], b"text"),
        ("64 bytes", &sixty_four, b"text"),
        ("text holds a marker", &[0xff], b"<!-- kit:key 01 -->\ntext"),
    ];
    for (name, key, text) in rows {
        let encoded = marker::encode(WORD, key, text, 1024).expect("fixture fits");
        assert_eq!(marker::decode(WORD, &encoded), Some(Box::from(key)), "{name}");
        assert!(encoded.ends_with(text), "{name}: text kept whole");
        let size = u32::try_from(encoded.len()).expect("fixture length");
        assert_eq!(marker::encode(WORD, key, text, size), Some(encoded), "{name}: exact limit");
        assert_eq!(marker::encode(WORD, key, text, size.saturating_sub(1)), None, "{name}: one short");
    }
    let marked_body_bytes = marker::length(WORD, 2).expect("fixture length").checked_add(4).expect("fixture text");
    assert_eq!(
        marker::encode(WORD, &[0xab, 0x01], b"body", marked_body_bytes),
        Some(Box::from(&b"<!-- kit:key ab01 -->\nbody"[..]))
    );
    assert_eq!(marker::length(WORD, u32::MAX), None);
}

#[test]
fn a_body_without_a_well_formed_marker_has_no_key() {
    let rows: [(&str, &[u8]); 5] = [
        ("uppercase", b"<!-- kit:key AB -->\n"),
        ("odd", b"<!-- kit:key a -->\n"),
        ("no line feed", b"<!-- kit:key ab -->"),
        ("leading space", b" <!-- kit:key ab -->\n"),
        ("after text", b"text<!-- kit:key ab -->\n"),
    ];
    for (name, body) in rows {
        assert_eq!(marker::decode(WORD, body), None, "{name}");
    }
}

#[test]
fn a_full_table_drops_the_entry_used_longest_ago() {
    let mut tables = Tables::new(2, 2);
    let rows = [
        ("repositories stay separate by forge", api::Repository { forge: 2, repository: 1 }),
        ("repositories stay separate by id", api::Repository { forge: 1, repository: 2 }),
    ];
    for (name, second) in rows {
        tables.keep_repository(fixtures::REPO, fixtures::specimen());
        tables.keep_repository(second, fixtures::specimen());
        assert_eq!(tables.repository(fixtures::REPO), Some(&fixtures::specimen()), "{name}: use first");
        tables.keep_repository(api::Repository { forge: 3, repository: 3 }, fixtures::specimen());
        assert_eq!(tables.repository(second), None, "{name}: oldest evicted");
        assert_eq!(tables.repository(fixtures::REPO), Some(&fixtures::specimen()), "{name}: recent kept");
    }
}

#[test]
fn a_full_user_table_keeps_the_most_recent_login_and_replaces_in_place() {
    let mut tables = Tables::new(2, 2);
    tables.keep_user(1, 1, Box::from(&b"first"[..]));
    tables.keep_user(2, 1, Box::from(&b"other-forge"[..]));
    assert_eq!(tables.user(1, 1), Some(&b"first"[..]), "first touched");
    tables.keep_user(1, 2, Box::from(&b"third"[..]));
    assert_eq!(tables.user(2, 1), None, "oldest evicted");
    tables.keep_user(1, 1, Box::from(&b"renamed"[..]));
    assert_eq!(tables.user(1, 1), Some(&b"renamed"[..]), "existing login refreshed");
    assert_eq!(tables.user(1, 2), Some(&b"third"[..]), "replacement evicts nothing");
}
