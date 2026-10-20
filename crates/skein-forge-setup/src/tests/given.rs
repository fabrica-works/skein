use super::fixtures::{self, Run, secret};
use crate::call::Call;
use crate::{
    given::{self, Given, Place, Refused},
    reconcile::{Event, Failure, Need, Reconcile, Reply, Stage, Why},
};
use core::net::{Ipv4Addr, SocketAddr};
use skein_forge::forgejo::{
    document::{ApiSettings, Document},
    request::Request,
};

#[test]
fn the_word_derives_only_the_bot_marker() {
    assert_eq!(given::marker(b"kit").as_ref(), b"kit:bot");
}

fn other_word() -> Run {
    let mut given = fixtures::given();
    given.word = b"other".as_slice().into();
    Run::with_given(given)
}
fn other_bot() -> Event {
    Event::Reply(Reply::Document(Document::User(fixtures::person(b"kit", b"other:bot"))))
}
#[test]
fn the_given_names_are_used_verbatim() {
    let mut run = other_word();
    run.step(Event::Begin);
    run.step(Event::Read(Some(secret(fixtures::ADMIN))));
    assert_eq!(
        run.step(Event::Reply(Reply::Status(401))),
        Need::Stop(Failure {
            forge: 1,
            step: Stage::Admin,
            subject: b"kit-admin".as_slice().into(),
            why: Why::Refused(401),
        })
    );
    let prefixes: [(Box<[Event]>, &[u8]); 2] =
        [(Box::new([]), b"kit-api"), (Box::new([Event::Read(Some(secret(b"api-token"))), other_bot()]), b"kit-git")];
    for (prefix, name) in prefixes {
        let mut run = other_word();
        fixtures::to_bot(&mut run);
        run.step(other_bot());
        for event in prefix {
            run.step(event);
        }
        run.step(Event::Read(None));
        run.step(Event::Reply(Reply::Document(Document::ApiSettings(ApiSettings {
            max_response_items: 2,
            default_paging_num: 30,
        }))));
        let Need::Send { request: Call::Forgejo(Request::CreateToken { name: found, .. }), .. } =
            run.step(Event::Reply(Reply::Rows(Box::new([]))))
        else {
            panic!("token creation");
        };
        assert_eq!(found.as_ref(), name, "{name:?}");
    }
}

fn accepted(given: &Given, label: &str) {
    assert_eq!(given::check(given), Ok(()), "{label}");
    assert!(Reconcile::new(given).is_ok(), "{label}");
}
// Grammar maxima from forge-setup.md, 4.1, and their one-byte overruns.
const WORD_MAX: usize = 16;
const LOGIN_MAX: usize = 40;
const EMAIL_MAX: usize = 254;
#[test]
fn valid_given_values_are_accepted() {
    accepted(&fixtures::given(), "empty orgs");
    let mut longest_email = [b'a'; EMAIL_MAX];
    longest_email[1] = b'@';
    for (word, login, email) in [
        (b"kit".as_slice(), b"kit".as_slice(), b"kit@example.invalid".as_slice()),
        (b"a".as_slice(), b"a".as_slice(), b"a@b".as_slice()),
        (&[b'a'; WORD_MAX], &[b'a'; LOGIN_MAX], longest_email.as_slice()),
    ] {
        let mut given = fixtures::given();
        given.word = word.into();
        given.bot_login = login.into();
        given.bot_email = email.into();
        given.orgs = Box::new([login.into()]);
        given.org_owner = Some(b"ada".as_slice().into());
        accepted(&given, "grammar boundary");
    }
    for forge in [0, 65535] {
        for listener in [
            None,
            Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 9898))),
            Some(SocketAddr::from((Ipv4Addr::new(192, 0, 2, 1), 9898))),
        ] {
            let mut given = fixtures::given();
            given.forge = forge;
            given.listener = listener;
            accepted(&given, "owner address and number");
        }
    }
}
fn invalid(place: Place, value: &[u8]) -> Given {
    let mut given = fixtures::given();
    match place {
        Place::Word => given.word = value.into(),
        Place::BotLogin => given.bot_login = value.into(),
        Place::BotEmail => given.bot_email = value.into(),
        Place::Org(_) => {
            given.orgs = Box::new([value.into()]);
            given.org_owner = Some(b"ada".as_slice().into());
        }
        Place::OrgOwner => given.org_owner = Some(value.into()),
        Place::AdminName => given.admin_name = value.into(),
        Place::ApiName => given.api_name = value.into(),
        Place::GitName => given.git_name = value.into(),
        Place::Orgs
        | Place::Repositories
        | Place::Repository(_)
        | Place::OrgsLimit
        | Place::ReachableLimit
        | Place::Page
        | Place::RepositoriesLimit
        | Place::Deadline
        | Place::Backoff
        | Place::BackoffMax => {
            unreachable!("grammar rows")
        }
    }
    given
}
#[test]
fn invalid_given_values_name_their_place() {
    for (place, value) in [
        (Place::Word, b"".as_slice()),
        (Place::Word, &[b'a'; WORD_MAX + 1]),
        (Place::Word, b"Kit"),
        (Place::BotLogin, b""),
        (Place::BotLogin, &[b'a'; LOGIN_MAX + 1]),
        (Place::BotLogin, b".kit"),
        (Place::BotEmail, b"a@"),
        (Place::BotEmail, &[b'a'; EMAIL_MAX + 1]),
        (Place::BotEmail, b"a@b\n"),
        (Place::BotEmail, b"a@@b"),
        (Place::Org(0), b".bad"),
        (Place::OrgOwner, b".bad"),
        (Place::AdminName, b""),
        (Place::ApiName, b""),
        (Place::GitName, b""),
    ] {
        let given = invalid(place, value);
        let expected = Err(Refused { place, other: None });
        assert_eq!(given::check(&given), expected, "{place:?} {value:?}");
        assert_eq!(Reconcile::new(&given).err(), expected.err(), "constructor {place:?} {value:?}");
    }
}

fn org_count(count: u32) -> Given {
    let mut given = fixtures::given();
    let mut names = skein_lib::List::with_capacity(count);
    for _ in 0..count {
        names.push(b"acme".as_slice().into()).expect("fixture org count");
    }
    given.orgs = names.into_boxed();
    given.org_owner = if count > 0 { Some(b"ada".as_slice().into()) } else { None };
    given
}
fn refused(given: &Given, place: Place, other: Option<Place>) {
    let expected = Refused { place, other };
    assert_eq!(given::check(given), Err(expected), "{place:?}");
    assert_eq!(Reconcile::new(given).err(), Some(expected), "constructor {place:?}");
}
#[test]
fn no_org_is_admitted() {
    accepted(&org_count(0), "no orgs and no owner");
}
#[test]
fn orgs_without_an_owner_are_refused_naming_the_pair() {
    let mut given = org_count(1);
    given.org_owner = None;
    refused(&given, Place::Orgs, Some(Place::OrgOwner));
}
#[test]
fn orgs_with_an_owner_are_accepted() {
    accepted(&org_count(1), "orgs and their owner");
}
#[test]
fn an_org_count_at_the_bound_is_accepted() {
    accepted(&org_count(fixtures::PROFILE_ORGS), "org bound");
}
#[test]
fn an_org_count_past_the_bound_is_refused() {
    refused(&org_count(fixtures::PROFILE_ORGS + 1), Place::Orgs, None);
}
#[test]
fn a_zero_limit_names_its_place() {
    use crate::limits;
    use skein_lib::Duration;
    for place in [
        Place::OrgsLimit,
        Place::RepositoriesLimit,
        Place::ReachableLimit,
        Place::Deadline,
        Place::Backoff,
        Place::BackoffMax,
    ] {
        let mut given = fixtures::given();
        match place {
            Place::OrgsLimit => given.limits.orgs = 0,
            Place::RepositoriesLimit => given.limits.repositories = 0,
            Place::ReachableLimit => given.limits.reachable = 0,
            Place::Deadline => given.limits.deadline = Duration::ZERO,
            Place::Backoff => given.limits.backoff = Duration::ZERO,
            Place::BackoffMax => given.limits.backoff_max = Duration::ZERO,
            Place::Word
            | Place::BotLogin
            | Place::BotEmail
            | Place::AdminName
            | Place::ApiName
            | Place::GitName
            | Place::Orgs
            | Place::Org(_)
            | Place::OrgOwner
            | Place::Repositories
            | Place::Page
            | Place::Repository(_) => unreachable!("limit rows"),
        }
        assert_eq!(limits::check(&given.limits), Err(Refused { place, other: None }), "{place:?}");
        refused(&given, place, None);
    }
}

fn adopted(ids: &[u64]) -> Given {
    let mut given = fixtures::given();
    given.adopted = ids.into();
    given
}
fn adopted_count(count: u32) -> Given {
    let mut ids = skein_lib::List::with_capacity(count);
    for id in 1..=u64::from(count) {
        ids.push(id).expect("fixture repository count");
    }
    adopted(ids.as_slice())
}
#[test]
fn adopted_repositories_are_accepted_in_the_order_given() {
    let ids = [9, 3, 7];
    let given = adopted(&ids);
    accepted(&given, "adopted order");
    assert_eq!(given.adopted.as_ref(), &ids);
}
#[test]
fn no_repository_is_admitted() {
    accepted(&adopted(&[]), "no repositories");
}
#[test]
fn an_adopted_count_at_the_bound_is_accepted() {
    accepted(&adopted_count(fixtures::PROFILE_REPOSITORIES), "repository bound");
}
#[test]
fn an_adopted_count_past_the_bound_is_refused() {
    refused(&adopted_count(fixtures::PROFILE_REPOSITORIES + 1), Place::Repositories, None);
}
#[test]
fn an_adopted_id_of_zero_names_its_index() {
    for (ids, first_zero) in [([7, 9, 0], 2), ([7, 0, 0], 1), ([7, 7, 0], 2)] {
        refused(&adopted(&ids), Place::Repository(first_zero), None);
    }
}
#[test]
fn a_repeated_adopted_id_names_the_repeat_and_the_first() {
    for (ids, later, earlier) in
        [(&[7, 9, 7][..], 2, 0), (&[9, 7, 9, 7], 2, 0), (&[7, 7, 7], 1, 0), (&[u64::MAX, 1, u64::MAX], 2, 0)]
    {
        refused(&adopted(ids), Place::Repository(later), Some(Place::Repository(earlier)));
    }
}
#[test]
fn ids_that_differ_are_never_a_repeat() {
    for ids in [[1, 2, 3], [3, 2, 1], [u64::MAX, 1, 7]] {
        accepted(&adopted(&ids), "distinct ids");
    }
}
#[test]
fn the_count_is_checked_before_the_ids() {
    let mut given = adopted_count(fixtures::PROFILE_REPOSITORIES + 1);
    given.adopted[0] = 0;
    refused(&given, Place::Repositories, None);
}

#[test]
fn the_owners_check_of_adopted_refuses_what_check_refuses() {
    for (given, expected) in [
        (adopted(&[7, 9, 7]), Refused { place: Place::Repository(2), other: Some(Place::Repository(0)) }),
        (adopted(&[7, 9, 0]), Refused { place: Place::Repository(2), other: None }),
        (adopted_count(fixtures::PROFILE_REPOSITORIES + 1), Refused { place: Place::Repositories, other: None }),
    ] {
        assert_eq!(given::check_adopted(&given.adopted, &given.limits), Err(expected));
        assert_eq!(given::check(&given), Err(expected));
    }
}
#[test]
fn an_empty_adopted_passes_the_owners_check() {
    assert_eq!(given::check_adopted(&[], &fixtures::limits()), Ok(()));
}
