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
