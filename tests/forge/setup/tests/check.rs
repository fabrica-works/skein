use skein_forge::forgejo::request::Request;
use skein_forge_setup::{
    item::Item,
    reconcile::{Failure, Outcome, Stage, Why},
};
use skein_forge_setup_world::{Ended, World, fixtures};

fn failure(subject: &[u8], why: Why) -> Ended {
    Ended::Stop(Failure { forge: 1, step: Stage::Token, subject: subject.into(), why })
}

#[test]
fn two_tokens_of_one_user_are_accepted_and_the_user_is_kept() {
    let mut world = World::managed(7);
    let id = fixtures::connected_tokens(&mut world);
    assert_eq!(world.check(), Ended::Done);
    assert_eq!(world.outcome(), Some(&Outcome::Identity { id, login: b"alice".as_slice().into() }));
    assert_eq!(world.file(Item::User).expect("user kept").expose(), &id.to_le_bytes());
    assert_eq!(world.requests(), &[Request::User, Request::User]);
    assert!(fixtures::write_routes(&world, 0).is_empty());
}

#[test]
fn a_second_check_of_the_same_tokens_writes_nothing() {
    let mut world = World::managed(7);
    fixtures::connected_tokens(&mut world);
    assert_eq!(world.check(), Ended::Done);
    let writes = fixtures::file_writes(&world);
    assert_eq!(world.check(), Ended::Done);
    assert_eq!(fixtures::file_writes(&world), writes);
}

#[test]
fn two_tokens_of_two_users_are_refused_naming_git() {
    let mut world = World::managed(7);
    fixtures::connected_tokens(&mut world);
    let other = world.store_mut().account(b"another", false, b"Another person");
    let token = fixtures::secret(b"another-persons-git-token");
    world.store_mut().token(other, b"git", &[b"write:repository", b"read:user"], token.clone());
    world.put(Item::GitToken, token);
    assert_eq!(world.check(), failure(b"git", Why::OtherUser));
    assert!(world.file(Item::User).is_none());
}

#[test]
fn a_git_token_without_read_user_is_refused_naming_git() {
    let mut world = World::managed(7);
    let id = fixtures::connected_tokens(&mut world);
    let token = fixtures::secret(b"git-token-without-read-user");
    world.store_mut().token(id, b"git-no-user", &[b"write:repository"], token.clone());
    world.put(Item::GitToken, token);
    assert_eq!(world.check(), failure(b"git", Why::Refused(403)));
    assert!(world.file(Item::User).is_none());
}

#[test]
fn an_api_token_of_a_site_admin_is_refused() {
    let mut world = World::managed(7);
    fixtures::connected_tokens(&mut world);
    let admin = world.store().find(b"kit-admin").expect("admin fixture").id;
    let token = fixtures::secret(b"admin-token-for-a-connected-forge");
    world.store_mut().token(admin, b"connected-admin", &[b"read:user"], token.clone());
    world.put(Item::ApiToken, token);
    assert_eq!(world.check(), failure(b"api", Why::BotIsAdmin));
    assert_eq!(world.requests(), &[Request::User]);
    assert!(world.file(Item::User).is_none());
}

#[test]
fn a_replacement_answering_as_another_user_is_refused() {
    let mut world = World::managed(7);
    fixtures::connected_tokens(&mut world);
    let other = world.store_mut().account(b"another", false, b"Another person");
    let kept = fixtures::secret(&other.to_le_bytes());
    world.put(Item::User, kept.clone());
    assert_eq!(world.check(), failure(b"api", Why::OtherUser));
    assert_eq!(world.file(Item::User), Some(&kept));
    assert_eq!(fixtures::file_writes(&world), 0);
}

#[test]
fn no_secret_is_in_a_checks_failure_debug_or_trace() {
    let mut world = World::managed(7);
    fixtures::connected_tokens(&mut world);
    let unknown = b"unknown-connected-api-token";
    world.put(Item::ApiToken, fixtures::secret(unknown));
    let ended = world.check();
    assert_eq!(ended, failure(b"api", Why::Refused(401)));
    let debug = format!("{ended:?}");
    for secret in [unknown.as_slice(), fixtures::API, fixtures::GIT, fixtures::ADMIN] {
        assert!(!debug.as_bytes().windows(secret.len()).any(|bytes| bytes == secret));
    }
    assert!(world.secrets_absent());
}
