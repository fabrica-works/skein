use super::{Installations, Kind, LOGIN_BYTES, Limits, Reach, Scope};
use skein_lib::{List, Wall};

#[test]
fn github_fake_memory() {
    let limits = Limits { installations: 3, tokens: 5, listed: 4 };
    let mut store = Installations::new(&limits, 7);
    for account in 1..=u64::from(limits.installations) {
        // The retained table must not keep a caller's excess capacity.
        let mut repositories = List::with_capacity(16);
        for repository in 0..limits.listed {
            repositories.push(repository).expect("repository capacity");
        }
        assert_eq!(
            store.install(account, &[b'x'; LOGIN_BYTES], Kind::Organization, Reach::Listed(repositories), Wall::EPOCH),
            Ok(account)
        );
    }
    for _token in 0..limits.tokens {
        drop(store.mint(1, Scope::Full, Wall::EPOCH).expect("token capacity"));
    }
    let mut held =
        size_of_val(store.rows.as_slice()).checked_add(size_of_val(store.tokens.as_slice())).expect("table bytes");
    assert_eq!(store.rows.len(), store.rows.capacity());
    assert_eq!(store.tokens.len(), store.tokens.capacity());
    for row in store.iter() {
        held = held.checked_add(row.login.len()).expect("login bytes");
        match &row.reach {
            Reach::All => unreachable!("filled with listed installations"),
            Reach::Listed(repositories) => {
                assert_eq!(repositories.capacity(), limits.listed, "retained capacity is bounded");
                held = held.checked_add(size_of_val(repositories.as_slice())).expect("repository bytes");
            }
        }
    }
    for token in store.tokens.iter().flatten() {
        held = held.checked_add(token.secret.len()).expect("token bytes");
    }
    assert_eq!(Installations::worst_case(&limits), Some(u64::try_from(held).expect("heap bytes")));
    assert_eq!(Installations::worst_case(&Limits { installations: 0, tokens: 0, listed: 0 }), Some(0));
    assert_eq!(
        Installations::worst_case(&Limits { installations: u32::MAX, tokens: u32::MAX, listed: u32::MAX }),
        None
    );
    // Failed admission changes neither a store's rows nor its id sequence.
    assert!(store.remove(2), "remove middle row");
    assert!(store.remove(1), "remove row and all its tokens");
    assert_eq!(store.tokens.len(), 0);
    assert_eq!(store.install(9, b"new", Kind::User, Reach::All, Wall::EPOCH), Ok(4));
    assert_eq!(store.iter().next().expect("surviving row").id, 3);
}
