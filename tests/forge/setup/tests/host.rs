use skein_forge_setup::{
    item::{self, Item},
    reconcile::{Failure, Stage, Why},
};
use skein_forge_setup_world::{Connect, Heard, HostWorld};
use skein_lib::{Duration, Token};
fn stopped(stage: Stage, subject: &[u8], at: Duration) -> Heard {
    Heard::Stop {
        run: Token::new(1),
        failure: Failure { forge: 1, step: stage, subject: subject.into(), why: Why::Down },
        at,
    }
}
#[test]
fn the_host_reconciles_an_empty_forge_through_http() {
    let mut world = HostWorld::new(7, &[]);
    world.run(1);
    world.finish();
    assert_eq!(world.heard(), &[Heard::Done { run: Token::new(1), at: Duration::ZERO }]);
    assert_eq!(world.file(Item::User).expect("User written").expose(), &2u64.to_le_bytes());
    for key in [Item::ApiToken, Item::GitToken] {
        assert!(item::token(world.file(key).expect("token written").expose(), 64).is_some());
    }
}
#[test]
fn refused_and_unavailable_first_tries_wait_doubling_then_converge() {
    let mut world = HostWorld::new(7, &[Connect::Refused, Connect::Unavailable, Connect::Refused]);
    world.run(1);
    world.finish();
    assert_eq!(world.heard(), &[Heard::Done { run: Token::new(1), at: Duration::from_millis(700) }]);
    assert_eq!(&world.connects()[..4], &[0, 100, 300, 700].map(Duration::from_millis));
}
#[test]
fn a_down_after_the_first_answer_stops_the_run_at_its_step() {
    let mut world = HostWorld::new(7, &[Connect::Made, Connect::Refused]);
    world.run(1);
    world.finish();
    assert_eq!(world.heard(), &[stopped(Stage::Bot, b"kit", Duration::ZERO)]);
}
#[test]
fn a_forge_that_never_answers_stops_down_at_its_deadline() {
    let mut world = HostWorld::new(7, &[Connect::Silent]);
    world.run(1);
    world.finish();
    assert_eq!(world.heard(), &[stopped(Stage::Admin, b"kit-admin", Duration::from_secs(120))]);
    assert_eq!(world.aborts(), 1);
}
#[test]
fn close_mid_run_ends_it_unheard_and_says_closed() {
    let mut world = HostWorld::new(7, &[Connect::Silent]);
    world.run(1);
    world.settle();
    world.close();
    world.settle();
    assert_eq!(world.heard(), &[Heard::Closed { at: Duration::ZERO }]);
    assert_eq!(world.aborts(), 1);
}
