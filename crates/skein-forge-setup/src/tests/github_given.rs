use super::github_fixtures as fixtures;
use crate::github::{
    given::{self, Given, Place, Refused},
    limits::{self, Limits},
};
use skein_lib::{Duration, Writer};

// A relation's adjacent values differ by one nanosecond (forge-setup.md, 4.4).
const ONE_NS: Duration = Duration::from_nanos(1);
const LIMIT_PLACES: [Place; 10] = [
    Place::KeyBytes,
    Place::NameBytes,
    Place::PageBytes,
    Place::Installations,
    Place::Reachable,
    Place::Pages,
    Place::StateLife,
    Place::Deadline,
    Place::Backoff,
    Place::BackoffMax,
];

fn refused(value: &Given, place: Place, other: Option<Place>) {
    assert_eq!(given::check(value), Err(Refused { place, other }), "{place:?}");
}
fn limit_refused(value: &Given, place: Place, other: Option<Place>) {
    assert_eq!(limits::check(&value.limits), Err(Refused { place, other }), "{place:?}");
    refused(value, place, other);
}
fn with_address(place: Place, value: &[u8]) -> Given {
    let mut given = fixtures::given();
    match place {
        Place::Url => given.url = value.into(),
        Place::RedirectUrl => given.redirect_url = value.into(),
        Place::SetupUrl => given.setup_url = value.into(),
        Place::Word
        | Place::Web
        | Place::KeyBytes
        | Place::NameBytes
        | Place::PageBytes
        | Place::Installations
        | Place::Reachable
        | Place::Page
        | Place::Pages
        | Place::StateLife
        | Place::Deadline
        | Place::Backoff
        | Place::BackoffMax => unreachable!("address rows"),
    }
    given
}
fn url_of_length(length: usize) -> Box<[u8]> {
    let mut out = Writer::new(length);
    out.put(b"https://").expect("URL scheme fits");
    for _ in b"https://".len()..length {
        out.put(b"a").expect("URL capacity");
    }
    out.finish()
}
fn zero_limit(value: &mut Limits, place: Place) {
    match place {
        Place::KeyBytes => value.key_bytes = 0,
        Place::NameBytes => value.name_bytes = 0,
        Place::PageBytes => value.page_bytes = 0,
        Place::Installations => value.installations = 0,
        Place::Reachable => value.reachable = 0,
        Place::Pages => value.pages = 0,
        Place::StateLife => value.state_life = Duration::ZERO,
        Place::Deadline => value.deadline = Duration::ZERO,
        Place::Backoff => value.backoff = Duration::ZERO,
        Place::BackoffMax => value.backoff_max = Duration::ZERO,
        Place::Word | Place::Web | Place::Url | Place::RedirectUrl | Place::SetupUrl | Place::Page => {
            unreachable!("limit rows")
        }
    }
}

#[test]
fn a_well_formed_github_given_is_admitted() {
    assert_eq!(given::check(&fixtures::given()), Ok(()));
    assert_eq!(limits::check(&fixtures::limits()), Ok(()));
}
#[test]
fn a_word_that_is_not_a_marker_word_is_refused_naming_word() {
    for word in [b"".as_slice(), b"Kit", b"a_b", b"kit:bot", &[b'a'; 17], b"kit\x7f", b"kit\x80"] {
        let mut given = fixtures::given();
        given.word = word.into();
        refused(&given, Place::Word, None);
    }
}
#[test]
fn the_web_origin_is_a_scheme_and_a_host_alone() {
    let bound = url_of_length(given::URL_BYTES);
    let over = url_of_length(given::URL_BYTES + 1);
    for (web, admitted) in [
        (b"https://github.com".as_slice(), true),
        (b"http://127.0.0.1:8080", true),
        (bound.as_ref(), true),
        (b"https://github.com/", false),
        (b"https://github.com/path", false),
        (b"https://github.com?query", false),
        (b"https://github.com#fragment", false),
        (b"ftp://x", false),
        (b"https://", false),
        (b"http://", false),
        (b"", false),
        (b"https://a b", false),
        (b"https://a\x00", false),
        (b"https://a\x7f", false),
        (b"https://a\x80", false),
        (over.as_ref(), false),
    ] {
        let mut given = fixtures::given();
        given.web = web.into();
        if admitted {
            assert_eq!(given::check(&given), Ok(()), "{web:?}");
        } else {
            refused(&given, Place::Web, None);
        }
    }
}
#[test]
fn each_address_is_a_visible_http_url_within_its_bound() {
    let bound = url_of_length(given::URL_BYTES);
    let over = url_of_length(given::URL_BYTES + 1);
    for place in [Place::Url, Place::RedirectUrl, Place::SetupUrl] {
        for value in [
            b"".as_slice(),
            b"ftp://x",
            b"https://",
            b"http://",
            b"https://a b",
            b"https://a\n",
            b"https://a\x00",
            b"https://a\x7f",
            b"https://a\x80",
            over.as_ref(),
        ] {
            refused(&with_address(place, value), place, None);
        }
        for value in [bound.as_ref(), b"http://a", b"https://a/path?query#fragment"] {
            assert_eq!(given::check(&with_address(place, value)), Ok(()), "{place:?}");
        }
    }
}
#[test]
fn a_zero_limit_is_refused_naming_it() {
    for place in LIMIT_PLACES {
        let mut given = fixtures::given();
        zero_limit(&mut given.limits, place);
        limit_refused(&given, place, None);
    }
}
#[test]
fn the_first_zero_limit_wins() {
    for (first, second) in LIMIT_PLACES.into_iter().zip(LIMIT_PLACES.into_iter().skip(1)) {
        let mut given = fixtures::given();
        zero_limit(&mut given.limits, first);
        zero_limit(&mut given.limits, second);
        limit_refused(&given, first, None);
    }
}
#[test]
fn a_state_life_of_an_hour_is_admitted_and_one_nanosecond_more_refused() {
    let mut given = fixtures::given();
    given.limits.state_life = limits::STATE_LIFE_MAX;
    assert_eq!(limits::check(&given.limits), Ok(()));
    assert_eq!(given::check(&given), Ok(()));
    given.limits.state_life = limits::STATE_LIFE_MAX.checked_add(ONE_NS).unwrap();
    limit_refused(&given, Place::StateLife, None);
    given.limits.backoff = given.limits.deadline;
    limit_refused(&given, Place::StateLife, None);
    given.limits.deadline = Duration::ZERO;
    limit_refused(&given, Place::Deadline, None);
}
#[test]
fn backoff_above_its_maximum_names_the_pair() {
    let mut given = fixtures::given();
    given.limits.backoff = given.limits.backoff_max;
    assert_eq!(limits::check(&given.limits), Ok(()));
    assert_eq!(given::check(&given), Ok(()));
    given.limits.backoff = given.limits.backoff_max.checked_add(ONE_NS).unwrap();
    limit_refused(&given, Place::Backoff, Some(Place::BackoffMax));
}
#[test]
fn a_maximum_at_the_deadline_names_the_pair() {
    let mut given = fixtures::given();
    given.limits.backoff_max = Duration::from_nanos(given.limits.deadline.as_nanos() - ONE_NS.as_nanos());
    assert_eq!(limits::check(&given.limits), Ok(()));
    assert_eq!(given::check(&given), Ok(()));
    given.limits.backoff_max = given.limits.deadline;
    limit_refused(&given, Place::BackoffMax, Some(Place::Deadline));
}
#[test]
fn the_given_is_checked_before_its_limits() {
    let mut given = fixtures::given();
    given.word = b"Bad Word".as_slice().into();
    given.limits.key_bytes = 0;
    refused(&given, Place::Word, None);
}
#[test]
#[expect(clippy::disallowed_macros, reason = "the redaction test inspects the refusal's Debug rendering")]
fn a_refusal_holds_no_value() {
    let mut given = fixtures::given();
    given.web = b"https://SECRET-MARK/".as_slice().into();
    given.url = b"https://SECRET-MARK/".as_slice().into();
    given.redirect_url = b"https://SECRET-MARK/redirect".as_slice().into();
    given.setup_url = b"https://SECRET-MARK/setup".as_slice().into();
    let refusal = given::check(&given).unwrap_err();
    assert_eq!(refusal, Refused { place: Place::Web, other: None });
    assert!(!format!("{refusal:?}").contains("SECRET-MARK"));
}
