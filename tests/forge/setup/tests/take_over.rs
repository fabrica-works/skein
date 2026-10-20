use skein_fake_forge_protocol::Route;
use skein_forge::forgejo::request::{Page, Request};
use skein_forge_setup::{
    item::Item,
    reconcile::{Failure, Outcome, Stage, Why},
};
use skein_forge_setup_world::{Ended, World, fixtures};

fn failure(why: Why) -> Ended {
    Ended::Stop(Failure { forge: 1, step: Stage::Admin, subject: b"kit-admin".as_slice().into(), why })
}

#[test]
fn the_taken_token_is_written_and_every_older_admin_token_is_deleted() {
    let mut world = World::managed(7);
    let admin = fixtures::admin_handover(&mut world);
    let older: Vec<_> = world.store().issued().iter().take(5).map(|token| token.secret.clone()).collect();
    assert_eq!(world.take_over(fixtures::TAKEN), Ended::Done);
    assert_eq!(world.outcome(), Some(&Outcome::Nothing));
    for secret in older {
        assert_eq!(world.store().token_user(secret.expose()), None);
    }
    assert_eq!(world.store().token_user(fixtures::TAKEN), Some(admin));
    assert!(world.file(Item::AdminToken).expect("taken token kept").expose() == fixtures::TAKEN);
    assert_eq!(world.tally().routes.get(&Route::DeleteToken), Some(&5));
    let pages: Vec<_> =
        world
            .requests()
            .iter()
            .filter_map(|request| {
                if let Request::Tokens { login, page } = request { Some((login.as_ref(), *page)) } else { None }
            })
            .collect();
    assert_eq!(pages, vec![(b"kit-admin".as_slice(), Page { page: Some(1), limit: Some(2) }); 6]);
    assert!(world.secrets_absent());
}

#[test]
fn a_second_take_over_of_the_same_token_deletes_nothing() {
    let mut world = World::managed(7);
    fixtures::admin_handover(&mut world);
    assert_eq!(world.take_over(fixtures::TAKEN), Ended::Done);
    let deletes = world.tally().routes.get(&Route::DeleteToken).copied();
    let since = world.requests().len();
    let kept = world.file(Item::AdminToken).cloned();
    assert_eq!(world.take_over(fixtures::TAKEN), Ended::Done);
    assert_eq!(world.tally().routes.get(&Route::DeleteToken).copied(), deletes);
    assert!(fixtures::write_routes(&world, since).is_empty());
    assert_eq!(world.file(Item::AdminToken), kept.as_ref());
}

#[test]
fn the_reconcile_after_a_take_over_runs_with_the_taken_token() {
    let mut world = World::managed(7);
    let admin = fixtures::admin_handover(&mut world);
    assert_eq!(world.take_over(fixtures::TAKEN), Ended::Done);
    assert_eq!(world.store().token_user(fixtures::ADMIN), None);
    let requests = world.tally().total;
    let as_admin = world.tally().users.get(&admin).copied().expect("admin requests");
    let unauthorized = world.tally().unauthorized;
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(world.tally().unauthorized, unauthorized);
    assert_eq!(
        world.tally().users.get(&admin).copied().expect("admin requests") - as_admin,
        world.tally().total - requests
    );
    assert!(world.file(Item::AdminToken).expect("taken token kept").expose() == fixtures::TAKEN);
    assert!(world.secrets_absent());
}

#[test]
fn a_plain_users_token_is_refused_as_not_admin() {
    let mut world = World::managed(7);
    let person = world.store_mut().account(b"person", false, b"A person");
    world.store_mut().token(person, b"handover", &[b"read:user"], fixtures::secret(fixtures::TAKEN));
    let kept = world.file(Item::AdminToken).cloned();
    assert_eq!(world.take_over(fixtures::TAKEN), failure(Why::NotAdmin));
    assert_eq!(world.tally().routes.get(&Route::DeleteToken), None);
    assert_eq!(fixtures::file_writes(&world), 0);
    assert_eq!(world.file(Item::AdminToken), kept.as_ref());
    assert_eq!(world.requests(), &[Request::User]);
}

#[test]
fn no_secret_is_in_a_take_overs_failure_debug_or_trace() {
    let mut world = World::managed(7);
    let ended = world.take_over(fixtures::TAKEN);
    assert_eq!(ended, failure(Why::Refused(401)));
    let debug = format!("{ended:?}");
    for secret in [fixtures::TAKEN, fixtures::ADMIN] {
        assert!(!debug.as_bytes().windows(secret.len()).any(|bytes| bytes == secret));
    }
    assert!(world.secrets_absent());
}
