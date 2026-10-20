use super::fixtures::{self, boxed};
use crate::delivery;
use core::str::FromStr;
use skein_fake_forge_domain::{self as domain, api::Change};
use skein_forge::forgejo::{
    delivery::Delivery,
    document::Body,
    request::{HookSpec, Request},
};
use skein_lib::{List, Secret, sha256::Hmac};

const FIRST: &str = "http://127.1.2.3:7201/first?delivery=1";
const SECOND: &str = "http://[::1]:7202/second";
const FIRST_SECRET: &[u8] = b"first-secret";
const SECOND_SECRET: &[u8] = b"second-secret";
#[derive(Clone, Copy)]
enum Place {
    Repository,
    OtherRepository,
    Organization,
}
struct Case {
    name: &'static str,
    url: &'static str,
    events: &'static [&'static [u8]],
    active: bool,
    secret: &'static [u8],
    place: Place,
    two: bool,
    endpoint: bool,
    count: u32,
}
fn one(name: &'static str) -> Case {
    Case {
        name,
        url: FIRST,
        events: &[b"issues"],
        active: true,
        secret: FIRST_SECRET,
        place: Place::Repository,
        two: false,
        endpoint: false,
        count: 0,
    }
}
fn cases() -> [Case; 17] {
    [
        Case { two: true, count: 2, ..one("two hooks in id order") },
        Case { events: &[b"push"], ..one("another row") },
        Case { events: &[b"issue_assign"], count: 1, ..one("another event of the same row") },
        Case { active: false, ..one("inactive") },
        Case { secret: b"", ..one("unsigned") },
        Case { url: "https://127.0.0.1:7201/first", ..one("https") },
        Case { url: "http://localhost:7201/first", ..one("name") },
        Case { url: "http://127.0.0.1/first", ..one("no port") },
        Case { url: "http://192.0.2.1:7201/first", ..one("off loopback") },
        Case { url: "http://127.0.0.1:7201", ..one("no path") },
        Case { url: "http://127.0.0.1:7201/first\r\nInjected: field", ..one("injected URL") },
        Case { url: "http://[broken]:7201/first", ..one("malformed literal") },
        Case { place: Place::Organization, ..one("organization") },
        Case { place: Place::OtherRepository, ..one("other repository") },
        Case { two: true, endpoint: true, count: 1, ..one("endpoint wins") },
        Case { count: 1, ..one("loopback IPv4 range") },
        Case { url: SECOND, secret: SECOND_SECRET, count: 1, ..one("loopback IPv6") },
    ]
}
fn spec(url: &str, secret: &[u8], row: &Case) -> HookSpec {
    let mut events = List::with_capacity(u32::try_from(row.events.len()).unwrap());
    for event in row.events {
        events.push(boxed(event)).unwrap();
    }
    HookSpec {
        url: boxed(url.as_bytes()),
        secret: Secret::new(boxed(secret)),
        events: events.into_boxed(),
        active: row.active,
    }
}

fn seed(h: &mut fixtures::Harness, row: &Case) {
    let hook = spec(row.url, row.secret, row);
    let request = match row.place {
        Place::Repository => Request::CreateRepoHook { at: fixtures::at(), hook },
        Place::OtherRepository => {
            let config = &mut h.service.config;
            let mut repositories = List::with_capacity(2);
            for repository in &config.repositories {
                repositories.push(repository.clone()).unwrap();
            }
            repositories.push(crate::Repository { id: 3, full: boxed(b"org/other"), protected: None }).unwrap();
            config.repositories = repositories;
            Request::CreateRepoHook {
                at: skein_forge::forgejo::request::Repo { owner: boxed(b"org"), name: boxed(b"other") },
                hook,
            }
        }
        Place::Organization => {
            h.send(&Request::CreateOrg { person: boxed(b"kit"), name: boxed(b"work") });
            Request::CreateOrgHook { org: boxed(b"work"), hook }
        }
    };
    h.send(&request);
    assert_eq!(h.status(), 201, "{}", row.name);
    if row.two {
        h.send(&Request::CreateRepoHook { at: fixtures::at(), hook: spec(SECOND, SECOND_SECRET, row) });
        assert_eq!(h.status(), 201);
    }
    if row.endpoint {
        h.service
            .set_hooks(Some(delivery::Endpoint {
                forge: 1,
                host: boxed(b"127.0.0.1:7200"),
                secret: Secret::new(boxed(fixtures::FIXTURE_SECRET)),
            }))
            .unwrap();
    }
}
fn field<'a>(head: &'a [u8], prefix: &[u8]) -> &'a [u8] {
    let start = skein_lib::bytes::find(head, prefix).unwrap().checked_add(prefix.len()).unwrap();
    let tail = head.get(start..).unwrap();
    let end = skein_lib::bytes::find(tail, b"\r\n").unwrap();
    tail.get(..end).unwrap()
}
fn nibble(byte: u8) -> u8 {
    if byte.is_ascii_digit() {
        byte.checked_sub(b'0').unwrap()
    } else if (b'a'..=b'f').contains(&byte) {
        byte.checked_sub(b'a').unwrap().checked_add(10).unwrap()
    } else {
        panic!("hexadecimal signature")
    }
}

fn verify(post: &delivery::Post, secret: &[u8]) {
    let signature = field(&post.head, b"X-Forgejo-Signature: ");
    let mut signed = [0; 32];
    assert_eq!(signature.len(), 64);
    for (byte, digits) in signed.iter_mut().zip(signature.chunks_exact(2)) {
        let [high, low] = digits else { panic!("two hexadecimal digits") };
        *byte = nibble(*high).checked_mul(16).unwrap().checked_add(nibble(*low)).unwrap();
    }
    let mut hmac = Hmac::new(secret);
    hmac.update(&post.body);
    assert_eq!(signed, hmac.finish());
}
#[test]
fn deliveries_follow_only_the_repository_hooks_that_take_the_row() {
    for row in cases() {
        let mut h = fixtures::setup_harness();
        seed(&mut h, &row);
        let hook = domain::Request::Hook {
            repository: boxed(b"org/repo"),
            change: Change::Issue,
            number: Some(3),
            branch: None,
            commit: None,
            by: Some(2),
            body: Some(boxed(b"fixture body")),
            ended: None,
        };
        let posts = h.service.deliveries(&hook, &h.env.limits);
        assert_eq!(posts.len(), row.count, "{}", row.name);
        for (index, routed) in posts.iter().enumerate() {
            let (address, target, secret) = if row.endpoint {
                ("127.0.0.1:7200", "/hooks/1", fixtures::FIXTURE_SECRET)
            } else if index == 1 || row.url == SECOND {
                ("[::1]:7202", "/second", SECOND_SECRET)
            } else {
                ("127.1.2.3:7201", "/first?delivery=1", FIRST_SECRET)
            };
            assert_eq!(routed.address, core::net::SocketAddr::from_str(address).unwrap(), "{}", row.name);
            assert_eq!(field(&routed.post.head, b"Host: "), address.as_bytes());
            let prefix = b"POST ";
            let suffix = b" HTTP/1.1\r\n";
            let length = prefix.len().checked_add(target.len()).unwrap().checked_add(suffix.len()).unwrap();
            let mut start = skein_lib::Writer::new(length);
            for part in [prefix.as_slice(), target.as_bytes(), suffix] {
                start.put(part).unwrap();
            }
            assert!(routed.post.head.starts_with(&start.finish()));
            assert_eq!(
                fixtures::deliver(fixtures::delivery_event(&routed.post), &routed.post.body, &h.env.limits.documents),
                Ok(Delivery::Issue { repository: 2, number: 3, body: Body::Kept(boxed(b"fixture body")) })
            );
            verify(&routed.post, secret);
        }
    }
}
