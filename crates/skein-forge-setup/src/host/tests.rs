use super::*;
use crate::tests::fixtures;
use skein_lib::Wall;

#[test]
fn abort_upgrades_a_graceful_close_already_sent_to_io() {
    use crate::host::connection::Connection;
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: crate::tests::host::limits() };
    let forge = Forge {
        kind: Kind::Forgejo(fixtures::given()),
        address: skein_io::Addr::Ip(([127, 0, 0, 1], 3000).into()),
        transport: Transport::Plaintext { name: None },
    };
    let mut c = Connection::new(
        Token::new(1),
        &forge,
        b"kit",
        &skein_forge::forgejo::request::Request::User.into(),
        &fixtures::secret(fixtures::ADMIN),
        &env.limits,
    )
    .expect("connection");
    c.socket = Some(Token::new(9));
    c.fail();
    let mut io = Queue::with_capacity(4);
    assert!(c.pump(&env, &mut io));
    assert_eq!(io.pop(), Some(skein_io::Request::Close { entity: Token::new(9) }));
    c.abort();
    assert!(c.pump(&env, &mut io));
    assert_eq!(io.pop(), Some(skein_io::Request::Abort { entity: Token::new(9) }));
    assert!(!c.pump(&env, &mut io));
}
#[test]
fn a_listing_past_the_priced_page_is_refused_at_open() {
    use skein_forge::forgejo::request::{Page, Request};
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: crate::tests::host::limits() };
    let forge = Forge {
        kind: Kind::Forgejo(fixtures::given()),
        address: skein_io::Addr::Ip(([127, 0, 0, 1], 3000).into()),
        transport: Transport::Plaintext { name: None },
    };
    let send = |limit| Send {
        call: Request::Tokens { login: b"kit".as_slice().into(), page: Page { page: Some(1), limit: Some(limit) } }
            .into(),
        auth: crate::call::Auth::Token(fixtures::secret(fixtures::ADMIN)),
    };
    let inner = env.limits.inner;
    open(Token::new(1), &forge, b"kit", &send(inner), &env).expect("a page at inner is priced");
    assert_eq!(open(Token::new(1), &forge, b"kit", &send(inner + 1), &env).err(), Some(Refused::Unexpected));
}
