//! The GitHub layer's full stores, measured independently of the shared domain.
#[path = "github/fixtures.rs"]
mod f;

use skein_fake_forge_github::{
    self as fake,
    installations::{Kind, Reach, Scope},
    pages::Page,
};
use skein_forge::github::request::Method;
use skein_lib::List;
use skein_world::domain::heap::{Counting, Meter};

#[global_allocator]
static HEAP: Counting = Counting;

#[test]
fn github_service_full_stores_stay_within_worst_case() {
    // The queue belongs to the shared domain and has its own memory binary.
    // Keep every queue full while measuring only the GitHub layer's ownership.
    let (_domain, _domain_config) = f::domain();
    let meter = Meter::new();
    meter.start();
    let config = f::config();
    let limits = f::limits();
    let bound = fake::worst_case(&limits, &config).expect("bounded scenario");
    let mut service = fake::Service::new(config, &limits, f::SEED).expect("service");
    meter.check(meter.end(), bound, &"construction");
    for (account, login, ids) in [(22, b"acme".as_slice(), [1, 2]), (23, b"example", [3, 4])] {
        meter.start();
        let mut listed = List::with_capacity(limits.installations.listed);
        for id in ids {
            listed.push(id).unwrap();
        }
        service.installations_mut().install(account, login, Kind::Organization, Reach::Listed(listed), f::NOW).unwrap();
        meter.check(meter.end(), bound, &"full installation reach");
    }
    assert_eq!(service.installations().iter().count(), 2);
    for n in 0..limits.installations.tokens {
        meter.start();
        let token = service.installations_mut().mint(1 + u64::from(n % 2), Scope::Full, f::NOW).unwrap();
        let measured = meter.end();
        drop(token);
        meter.check(measured, bound, &"mint");
    }
    assert!(service.installations_mut().mint(1, Scope::Full, f::NOW).is_err(), "token store is full");
    let form = f::form();
    for (target, status) in [
        (b"/settings/apps/new?state=one".as_slice(), 200),
        (b"/settings/apps/new?state=two", 200),
        (b"/settings/apps/new?state=three", 503),
    ] {
        meter.start();
        let page = service.page(&Page { method: Method::Post, target, body: &form }, f::NOW);
        let measured = meter.end();
        assert_eq!(page.status, status);
        drop(page);
        meter.check(measured, bound, &"full manifest store");
    }
    let blob_path = vec![b'x'; usize::try_from(limits.http.head).expect("head length")];
    for _ in 0..=limits.blob_records {
        meter.start();
        let fake::service::Outcome::Reply(response) = service.blob(
            Method::Get,
            &blob_path,
            false,
            true,
            f::NOW,
            skein_lib::ReplyTo::new(skein_lib::Token::new(1)),
        ) else {
            panic!("unknown blob target")
        };
        let measured = meter.end();
        assert_eq!(response.status, 404);
        drop(response);
        meter.check(measured, bound, &"full blob records");
    }
    assert_eq!(service.blob_records().len(), limits.blob_records);
    let jwt = f::jwt(7, fake::keys::KEYS[0], f::NOW);
    for target in [b"/app".as_slice(), b"/app/installations?per_page=100&page=1"] {
        meter.start();
        let response = f::ask(&mut service, Method::Get, target, Some(jwt.expose()), b"");
        let measured = meter.end();
        assert_eq!(response.status, 200);
        drop(response);
        meter.check(measured, bound, &"full App and installation reads");
    }
    assert!(service.manifest(b"one").is_some());
    assert!(service.manifest(b"two").is_some());
}
