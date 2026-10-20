use skein_fake_forge_protocol::Route;
use skein_forge::forgejo::request::{Grant, Repo, Request};
use skein_forge_setup::{
    item::Item,
    reconcile::{Failure, Stage, Why},
};
use skein_forge_setup_world::{Ended, World, fixtures};
fn world() -> World {
    fixtures::repositories(7, false, &[b"alice/specimen"])
}
fn at() -> Repo {
    Repo { owner: b"alice".as_slice().into(), name: b"specimen".as_slice().into() }
}
fn access_requests() -> [Request; 3] {
    [
        Request::RepositoryById { id: 1 },
        Request::Permission { at: at(), login: b"kit".as_slice().into() },
        Request::AddCollaborator { at: at(), login: b"kit".as_slice().into(), permission: Grant::Write },
    ]
}
fn stopped(stage: Stage, subject: &[u8], why: Why) -> Ended {
    Ended::Stop(Failure { forge: 1, step: stage, subject: subject.into(), why })
}
#[test]
fn an_adopted_repository_gives_the_bot_write() {
    let mut world = world();
    world.adopted(&[1]);
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(world.store().collaborator(1, b"kit"), Some(Grant::Write));
    assert!(world.requests().ends_with(&access_requests()));
}
#[test]
fn a_second_run_writes_nothing_to_a_repository_the_bot_can_write() {
    let mut world = world();
    world.adopted(&[1]);
    assert_eq!(world.reconcile(), Ended::Done);
    let since = world.requests().len();
    let files = fixtures::file_writes(&world);
    assert_eq!(world.reconcile(), Ended::Done);
    assert!(fixtures::write_routes(&world, since).is_empty());
    assert_eq!(fixtures::file_writes(&world), files);
}
#[test]
fn a_collaborator_removed_is_made_again() {
    let mut world = world();
    world.adopted(&[1]);
    assert_eq!(world.reconcile(), Ended::Done);
    world.store_mut().grant(1, b"other", Grant::Read).expect("other grant");
    world.store_mut().revoke_collaborator(1, b"absent");
    world.store_mut().revoke_collaborator(1, b"kit");
    assert_eq!(world.store().collaborator(1, b"other"), Some(Grant::Read));
    assert_eq!(world.store().collaborator(1, b"kit"), None);
    let since = world.requests().len();
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(fixtures::write_routes(&world, since), [Route::AddCollaborator]);
    assert_eq!(world.store().collaborator(1, b"kit"), Some(Grant::Write));
}
#[test]
fn a_collaborator_downgraded_to_read_is_made_again() {
    let mut world = world();
    fixtures::bot_tokens(&mut world);
    world.adopted(&[1]);
    assert_eq!(world.reconcile(), Ended::Done);
    world.store_mut().grant(1, b"kit", Grant::Read).expect("grant");
    let since = world.requests().len();
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(fixtures::write_routes(&world, since), [Route::AddCollaborator]);
    assert_eq!(world.store().collaborator(1, b"kit"), Some(Grant::Write));
}
#[test]
fn a_collaborator_on_the_second_page_of_many_is_not_added_again() {
    let mut world = world();
    fixtures::bot_tokens(&mut world);
    world.adopted(&[1]);
    for login in [b"one".as_slice(), b"two", b"three", b"kit", b"five"] {
        if login != b"kit" {
            world.store_mut().account(login, false, b"");
        }
        world.store_mut().grant(1, login, Grant::Write).expect("grant");
    }
    assert_eq!(world.reconcile(), Ended::Done);
    assert!(fixtures::write_routes(&world, 0).is_empty());
    assert!(world.requests().ends_with(&access_requests()[..2]));
    assert!(!world.requests().iter().any(|r| matches!(r, Request::Collaborators { .. })));
}
#[test]
fn a_repository_the_bot_owns_is_left_as_it_is() {
    let mut world = fixtures::repositories(7, false, &[b"kit/own"]);
    fixtures::bot_tokens(&mut world);
    world.adopted(&[1]);
    assert_eq!(world.reconcile(), Ended::Done);
    assert!(fixtures::write_routes(&world, 0).is_empty());
    assert_eq!(world.store().collaborator(1, b"kit"), None);
}
#[test]
fn a_repository_the_forge_no_longer_has_is_passed_over_by_a_reconcile() {
    let mut world = world();
    world.adopted(&[1, 99]);
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(world.requests().last(), Some(&Request::RepositoryById { id: 99 }));
    assert_eq!(world.store().collaborator(1, b"kit"), Some(Grant::Write));
}
#[test]
fn two_services_keep_separate_access_on_one_forge() {
    let mut world = fixtures::repositories(7, false, &[b"alice/one", b"alice/two"]);
    world.service_word(b"alpha");
    world.adopted(&[1]);
    assert_eq!(world.reconcile(), Ended::Done);
    world.service_word(b"beta");
    world.adopted(&[2]);
    assert_eq!(world.reconcile(), Ended::Done);
    for (id, login, expected) in [
        (1, b"alpha".as_slice(), Some(Grant::Write)),
        (2, b"alpha", None),
        (1, b"beta", None),
        (2, b"beta", Some(Grant::Write)),
    ] {
        assert_eq!(world.store().collaborator(id, login), expected);
    }
}
#[test]
fn adopt_gives_the_bot_write_and_the_reconcile_after_it_writes_nothing() {
    let mut world = world();
    fixtures::bot_tokens(&mut world);
    assert_eq!(world.adopt(1), Ended::Done);
    assert_eq!(world.requests(), &[Request::User].into_iter().chain(access_requests()).collect::<Vec<_>>());
    assert_eq!(world.store().collaborator(1, b"kit"), Some(Grant::Write));
    world.adopted(&[1]);
    let since = world.requests().len();
    assert_eq!(world.reconcile(), Ended::Done);
    assert!(fixtures::write_routes(&world, since).is_empty());
}
#[test]
fn adopt_of_a_repository_the_forge_does_not_have_is_missing() {
    let mut world = world();
    assert_eq!(world.adopt(99), stopped(Stage::Repository, b"99", Why::Missing));
}
#[test]
fn a_plain_users_token_cannot_adopt() {
    let mut world = world();
    let user = world.store_mut().account(b"alice", false, b"");
    world.store_mut().token(user, b"plain", &[b"all"], fixtures::secret(b"plain-user-token"));
    world.put(Item::AdminToken, fixtures::secret(b"plain-user-token"));
    assert_eq!(world.adopt(1), stopped(Stage::Admin, b"kit-admin", Why::NotAdmin));
    assert_eq!(world.requests(), &[Request::User]);
}
#[test]
fn an_admin_token_without_write_repository_stops_adopt_refused() {
    let mut world = world();
    fixtures::bot_tokens(&mut world);
    let admin = world.store().find(b"kit-admin").expect("admin").id;
    world.store_mut().token(
        admin,
        b"without-repository",
        &[b"write:admin", b"read:user"],
        fixtures::secret(b"admin-without-repository-scope"),
    );
    world.put(Item::AdminToken, fixtures::secret(b"admin-without-repository-scope"));
    assert_eq!(world.adopt(1), stopped(Stage::Repository, b"1", Why::Refused(403)));
    assert_eq!(world.requests(), &[Request::User, Request::RepositoryById { id: 1 }]);
    assert!(fixtures::write_routes(&world, 0).is_empty());
}
#[test]
fn no_secret_is_in_an_adopts_failure_debug_or_trace() {
    let mut world = world();
    fixtures::bot_tokens(&mut world);
    assert_eq!(world.adopt(99), stopped(Stage::Repository, b"99", Why::Missing));
    assert!(world.secrets_absent());
}
