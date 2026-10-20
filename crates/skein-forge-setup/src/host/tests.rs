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
        &skein_forge::forgejo::request::Request::User,
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
