//! Sign-out admission, revocation and forgetting (oauth.md, section 6.3).
use super::*;
use private::Owner;
use skein_io::{file, kernel};
use skein_lib::Token;
const ADDRESS: core::net::SocketAddr =
    core::net::SocketAddr::new(core::net::IpAddr::V4(core::net::Ipv4Addr::LOCALHOST), 31000);
const SOCKET: Token = Token::new(8);
const CONFIRMED: &[u8] = b"HTTP/1.1 200 Revoked\r\nContent-Length: 0\r\n\r\n";
const UNCONFIRMED: &[u8] = b"HTTP/1.1 503 Unavailable\r\nContent-Length: 0\r\n\r\n";

fn requests<const N: usize>(requests: [Request; N]) -> Box<[Request]> {
    Box::new(requests)
}
fn revocation_terms(endpoint: Option<Box<[u8]>>) -> skein_oauth::Terms {
    skein_oauth::Terms { revocation_endpoint: endpoint, ..terms() }
}
fn configuration(endpoint: bool, access_only: bool) -> Account {
    let mut configuration = refresh_account(0, ADDRESS);
    if let Account::SignIn { registration, keeper: Keeper::Owner { kept: Some(record) }, .. } = &mut configuration {
        registration.terms =
            Some(revocation_terms(endpoint.then_some(bytes::copy_of(b"http://127.0.0.1:31000/revoke"))));
        record.expires_at = Wall::from_nanos(Duration::from_secs(30).as_nanos());
        if access_only {
            record.refresh_token = None;
        }
    }
    configuration
}
fn owner(endpoint: bool, access_only: bool) -> Owner {
    Owner::configured([configuration(endpoint, access_only)], &limits())
}
fn started(endpoint: bool, access_only: bool) -> Owner {
    let mut owner = owner(endpoint, access_only);
    owner.ask(Request::SignOut { account: 0 });
    owner
}
fn private_owner() -> Owner {
    let mut owner = Owner::new();
    let load = owner.opened();
    owner.terminal(file::Event::Loaded {
        owner: load,
        bytes: skein_oauth::encode_record(&tests_record(0), &owner.env.limits.client.document).expect("record"),
    });
    owner
}
fn answer(owner: &mut Owner, response: &[u8]) {
    owner.fire();
    let identity = match owner.io.pop() {
        Some(Lower::Connect { owner, .. }) => owner,
        None => return,
        Some(_) => panic!("revocation opens a connection"),
    };
    owner.socket(LowerEvent::Connecting { owner: identity, socket: SOCKET });
    owner.socket(LowerEvent::Connected { owner: identity });
    let mut remaining = response;
    for _ in 0_u32..64 {
        owner.fire();
        owner.finish_socket(identity, &mut remaining);
    }
}
#[test]
fn a_sign_out_is_refused_where_it_does_not_belong() {
    let mut empty = configuration(false, false);
    if let Account::SignIn { keeper, .. } = &mut empty {
        *keeper = Keeper::Owner { kept: None };
    }
    let mut full = limits();
    full.exchanges = 1;
    let rows = [
        ("handed in", Owner::configured([Account::HandedIn], &limits()), requests([]), 0, Refusal::Source),
        ("no record", Owner::configured([empty], &limits()), requests([]), 0, Refusal::NoRecord),
        ("busy", owner(false, false), requests([Request::SignOut { account: 0 }]), 0, Refusal::Busy),
        (
            "full",
            Owner::configured([configuration(false, false), configuration(false, false)], &full),
            requests([Request::SignOut { account: 0 }]),
            1,
            Refusal::Full { bound: 1 },
        ),
        ("closed", owner(false, false), requests([Request::Close]), 0, Refusal::Closed),
        ("absent", owner(false, false), requests([]), 1, Refusal::Account),
    ];
    for (name, mut owner, requests, account, expected) in rows {
        for request in requests {
            owner.ask(request);
        }
        while owner.above.pop().is_some() {}
        owner.ask(Request::SignOut { account });
        assert_eq!(refusal_of(owner.above.pop()), Some((account, Asked::SignOut, expected)), "{name}");
        assert!(owner.io.is_empty() && owner.files.is_empty(), "nothing to revoke or remove");
    }
}
#[test]
fn a_sign_out_says_whether_revocation_was_confirmed() {
    for (endpoint, access, response, revoked) in [
        (true, false, CONFIRMED, true),
        (true, false, UNCONFIRMED, false),
        (false, false, CONFIRMED, false),
        (true, true, CONFIRMED, false),
    ] {
        let mut owner = started(endpoint, access);
        answer(&mut owner, response);
        assert_event(owner.above.pop(), Event::SignedOut { account: 0, revoked });
        owner.ask(Request::Grant { account: 0 });
        assert_expired(owner.above.pop());
    }
}
#[test]
fn a_sign_out_forgets_the_record() {
    for (private, mut owner) in [(false, owner(false, false)), (true, private_owner())] {
        owner.ask(Request::SignOut { account: 0 });
        if private {
            let removal = owner.removal();
            owner.terminal(file::Event::Removed { owner: removal });
            assert_eq!(owner.component.private_expected(0), file::Expect::Absent);
        } else {
            owner.fire();
        }
        assert_event(owner.above.pop(), Event::SignedOut { account: 0, revoked: false });
        owner.ask(Request::SignOut { account: 0 });
        if !private {
            assert_eq!(refusal_of(owner.above.pop()), Some((0, Asked::SignOut, Refusal::NoRecord)));
        }
    }
}
#[test]
fn a_held_grant_ends_as_expired_at_a_sign_out() {
    let mut owner = owner(false, false);
    owner.ask(Request::Grant { account: 0 });
    drop(owner.above.pop());
    owner.ask(Request::SignOut { account: 0 });
    assert_expired(owner.above.pop());
    owner.ask(Request::Grant { account: 0 });
    owner.fire();
    assert_event(owner.above.pop(), Event::SignedOut { account: 0, revoked: false });
    assert_expired(owner.above.pop());
}
#[test]
fn a_cancelled_sign_out_ends_as_cancelled_and_keeps_the_record() {
    for (private, mut owner) in [(false, owner(true, false)), (true, private_owner())] {
        owner.ask(Request::SignOut { account: 0 });
        let removal = if private { Some(owner.removal()) } else { None };
        owner.ask(Request::Cancel { account: 0 });
        if let Some(removal) = removal {
            owner.fire();
            let cancelled = match owner.files.pop() {
                Some(FileLower::Cancel { owner }) => Some(owner),
                Some(FileLower::Request { .. } | FileLower::Remove { .. }) | None => None,
            };
            assert_eq!(cancelled, Some(removal));
            owner.terminal(file::Event::Cancelled { owner: removal });
        }
        assert_event(
            owner.above.pop(),
            Event::Failed {
                account: 0,
                ends: Ends::SignOut,
                failure: Failure::Exchange(skein_oauth::Failure::Cancelled),
            },
        );
        owner.ask(Request::Grant { account: 0 });
        assert_granted(owner.above.pop(), 0, 30);
    }
}
#[test]
fn a_file_not_removed_fails_the_sign_out_as_not_kept() {
    let mut owner = private_owner();
    owner.ask(Request::SignOut { account: 0 });
    let removal = owner.removal();
    owner.terminal(file::Event::Failed {
        owner: removal,
        error: kernel::Error::Permission,
        committed: false,
        residue: None,
    });
    assert_event(owner.above.pop(), Event::Failed { account: 0, ends: Ends::SignOut, failure: Failure::NotKept });
    owner.ask(Request::Grant { account: 0 });
    assert!(owner.above.is_empty(), "the next grant loads again");
}
#[test]
fn a_sign_in_after_a_sign_out_stores_expecting_no_file() {
    let mut owner = private_owner();
    owner.ask(Request::SignOut { account: 0 });
    let removal = owner.removal();
    owner.terminal(file::Event::Removed { owner: removal });
    owner.ask(Request::SignIn { account: 0 });
    assert_eq!(owner.component.private_expected(0), file::Expect::Absent);
}
#[test]
fn a_terms_endpoint_on_another_origin_is_unusable() {
    let mut configuration = configuration(true, false);
    if let Account::SignIn { registration, .. } = &mut configuration {
        registration.terms = Some(revocation_terms(Some(bytes::copy_of(b"https://another.example/revoke"))));
    }
    let mut configured = List::with_capacity(1);
    assert!(configured.push(configuration).is_ok());
    assert_eq!(Component::new(configured, &limits(), crypto(1)).err(), Some(Unusable::Origin { account: 0 }));
}
