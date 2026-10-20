use core::num::NonZeroU64;
use skein_forge_setup::{
    host::{Need, Plan},
    item::{self, Item},
    reconcile::{Failure, Outcome, Stage, Why},
};
use skein_forge_setup_world::{Connect, Heard, HostWorld, fixtures};
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
    assert_eq!(world.heard(), &[Heard::Done { run: Token::new(1), outcome: Outcome::Nothing, at: Duration::ZERO }]);
    assert_eq!(world.file(Item::User).expect("User written").expose(), &2u64.to_le_bytes());
    for key in [Item::ApiToken, Item::GitToken] {
        assert!(item::token(world.file(key).expect("token written").expose(), 64).is_some());
    }
}
#[test]
fn the_host_runs_a_plan_that_names_adopted_repositories() {
    use skein_fake_forge_protocol::Route;
    let mut without = HostWorld::new(7, &[]);
    without.run(1);
    without.finish();
    let mut world = HostWorld::new(7, &[]);
    world.run_adopted(1, &[1, 2]);
    world.finish();
    assert_eq!(world.heard(), &[Heard::Done { run: Token::new(1), outcome: Outcome::Nothing, at: Duration::ZERO }]);
    assert_eq!(world.requests().total, without.requests().total + 2);
    assert_eq!(world.requests().routes.get(&Route::RepositoryById), Some(&2));
    for (route, count) in &without.requests().routes {
        assert_eq!(world.requests().routes.get(route), Some(count));
    }
    assert_eq!(world.requests().routes.len(), without.requests().routes.len() + 1);
    assert_eq!(world.requests().unrouted, 0);
}
#[test]
fn refused_and_unavailable_first_tries_wait_doubling_then_converge() {
    let mut world = HostWorld::new(7, &[Connect::Refused, Connect::Unavailable, Connect::Refused]);
    world.run(1);
    world.finish();
    assert_eq!(
        world.heard(),
        &[Heard::Done { run: Token::new(1), outcome: Outcome::Nothing, at: Duration::from_millis(700) }]
    );
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

fn connected(world: &mut HostWorld) -> u64 {
    let id = fixtures::seed_connected_tokens(world.store_mut());
    world.put(Item::ApiToken, fixtures::secret(fixtures::API));
    world.put(Item::GitToken, fixtures::secret(fixtures::GIT));
    id
}
fn identity(id: u64, at: Duration) -> Heard {
    Heard::Done { run: Token::new(1), outcome: Outcome::Identity { id, login: b"alice".as_slice().into() }, at }
}
fn done(run: u64) -> Heard {
    Heard::Done { run: Token::new(run), outcome: Outcome::Nothing, at: Duration::ZERO }
}
#[test]
fn the_host_checks_two_tokens_of_one_user_and_hands_up_the_identity() {
    let mut world = HostWorld::new(7, &[]);
    let id = connected(&mut world);
    world.run_plan(1, Plan::Check { forge: 1 });
    world.finish();
    assert_eq!(world.heard(), &[identity(id, Duration::ZERO)]);
    assert_eq!(
        world.needs(),
        &[
            Need::Read(Item::ApiToken),
            Need::Read(Item::GitToken),
            Need::Read(Item::User),
            Need::Write { item: Item::User, bytes: fixtures::secret(&id.to_le_bytes()) },
        ]
    );
    assert_eq!(world.file(Item::User).expect("written before Done").expose(), &id.to_le_bytes());
}
#[test]
fn the_host_stops_a_check_of_two_users_naming_git() {
    let mut world = HostWorld::new(7, &[]);
    connected(&mut world);
    let other = world.store_mut().account(b"another", false, b"Another person");
    let token = fixtures::secret(b"another-persons-git-token");
    world.store_mut().token(other, b"git", &[b"write:repository", b"read:user"], token.clone());
    world.put(Item::GitToken, token);
    world.run_plan(1, Plan::Check { forge: 1 });
    world.finish();
    assert_eq!(
        world.heard(),
        &[Heard::Stop {
            run: Token::new(1),
            failure: Failure { forge: 1, step: Stage::Token, subject: b"git".as_slice().into(), why: Why::OtherUser },
            at: Duration::ZERO,
        }]
    );
    assert_eq!(world.needs(), &[Need::Read(Item::ApiToken), Need::Read(Item::GitToken)]);
    assert!(world.file(Item::User).is_none());
}
#[test]
fn a_check_with_no_user_item_kept_writes_the_id_as_eight_bytes() {
    let mut world = HostWorld::new(7, &[]);
    let id = connected(&mut world);
    world.pause_writes();
    world.run_plan(1, Plan::Check { forge: 1 });
    world.settle();
    assert!(world.heard().is_empty());
    assert!(world.file(Item::User).is_none());
    assert_eq!(
        world.needs(),
        &[
            Need::Read(Item::ApiToken),
            Need::Read(Item::GitToken),
            Need::Read(Item::User),
            Need::Write { item: Item::User, bytes: fixtures::secret(&id.to_le_bytes()) },
        ]
    );
    world.write_pending();
    world.finish();
    assert_eq!(world.heard(), &[identity(id, Duration::ZERO)]);
    assert_eq!(world.file(Item::User).expect("written").expose(), &id.to_le_bytes());
}
#[test]
fn a_check_waits_for_the_forge_as_a_reconcile_does() {
    let mut world = HostWorld::new(7, &[Connect::Refused, Connect::Unavailable]);
    let id = connected(&mut world);
    world.run_plan(1, Plan::Check { forge: 1 });
    world.finish();
    assert_eq!(world.heard(), &[identity(id, Duration::from_millis(300))]);
    assert_eq!(world.connects(), &[0, 100, 300, 300].map(Duration::from_millis));
}
#[test]
fn the_host_takes_over_the_handed_token_and_deletes_the_older_ones() {
    use skein_fake_forge_protocol::Route;
    let mut world = HostWorld::new(7, &[]);
    let admin = fixtures::seed_admin_handover(world.store_mut(), 1);
    let older: Vec<_> = world.store().issued().iter().take(2).map(|token| token.secret.clone()).collect();
    world.pause_writes();
    world.run_plan(1, Plan::TakeOver { forge: 1, admin_token: fixtures::secret(fixtures::TAKEN) });
    world.settle();
    assert!(world.heard().is_empty());
    assert_eq!(world.file(Item::AdminToken).expect("old token").expose(), fixtures::ADMIN);
    assert_eq!(world.needs(), &[Need::Write { item: Item::AdminToken, bytes: fixtures::secret(fixtures::TAKEN) }]);
    assert_eq!(world.requests().routes.get(&Route::DeleteToken), None);
    world.write_pending();
    world.finish();
    assert_eq!(world.heard(), &[done(1)]);
    assert_eq!(world.file(Item::AdminToken).expect("taken token written").expose(), fixtures::TAKEN);
    assert_eq!(world.requests().routes.get(&Route::DeleteToken), Some(&2));
    for secret in older {
        assert_eq!(world.store().token_user(secret.expose()), None);
    }
    assert_eq!(world.store().token_user(fixtures::TAKEN), Some(admin));
}
#[test]
fn a_reconcile_after_a_take_over_uses_the_taken_token() {
    let mut world = HostWorld::new(7, &[]);
    fixtures::seed_admin_handover(world.store_mut(), 1);
    world.run_plan(1, Plan::TakeOver { forge: 1, admin_token: fixtures::secret(fixtures::TAKEN) });
    world.finish();
    let since = world.needs().len();
    world.run(2);
    world.finish();
    assert_eq!(world.needs().get(since), Some(&Need::Read(Item::AdminToken)));
    assert_eq!(world.file(Item::AdminToken).expect("taken token").expose(), fixtures::TAKEN);
    assert_eq!(world.store().token_user(fixtures::ADMIN), None);
    assert_eq!(world.heard(), &[done(1), done(2)]);
}
#[test]
fn the_host_adopts_a_repository_and_hands_up_nothing() {
    use skein_forge::forgejo::request::Grant;
    let mut world = HostWorld::with_config(7, &[], fixtures::repository_config(&[b"alice/specimen"]));
    fixtures::seed_bot_tokens(world.store_mut());
    world.run_plan(1, Plan::Adopt { forge: 1, repository: NonZeroU64::new(1).unwrap() });
    world.finish();
    assert_eq!(world.store().collaborator(1, b"kit"), Some(Grant::Write));
    assert_eq!(world.heard(), &[done(1)]);
}
#[test]
fn an_adopt_of_a_repository_the_forge_lacks_stops_missing_naming_its_id() {
    let mut world = HostWorld::new(7, &[]);
    fixtures::seed_bot_tokens(world.store_mut());
    world.run_plan(1, Plan::Adopt { forge: 1, repository: NonZeroU64::new(99).unwrap() });
    world.finish();
    assert_eq!(
        world.heard(),
        &[Heard::Stop {
            run: Token::new(1),
            failure: Failure { forge: 1, step: Stage::Repository, subject: b"99".as_slice().into(), why: Why::Missing },
            at: Duration::ZERO,
        }]
    );
}
