use skein_fake_forge_protocol::Route;
use skein_forge::forgejo::request::Request;
use skein_forge_setup::item::Item;
use skein_forge_setup_world::{Ended, World, fixtures};
#[test]
fn an_empty_forge_is_given_its_bot_and_both_tokens() {
    let mut world = World::managed(7);
    assert_eq!(world.reconcile(), Ended::Done);
    let bot = world.store().find(b"kit").expect("the bot is made");
    assert_eq!((bot.admin, &*bot.full_name), (false, b"kit:bot".as_slice()));
    for file in [Item::ApiToken, Item::GitToken] {
        let token = world.file(file).expect("the token is written");
        assert_eq!(world.store().token_user(token.expose()), Some(bot.id), "{file:?} answers as the bot");
    }
    assert_eq!(
        world.file(Item::User).expect("the id is written").expose(),
        &2u64.to_le_bytes(),
        "the id in eight bytes"
    );
    assert!(world.secrets_absent(), "no secret in a failure, a debug or the trace");
}
#[test]
fn the_tokens_hold_the_scopes_of_setup_section_4() {
    let mut world = World::managed(7);
    assert_eq!(world.reconcile(), Ended::Done);
    let id = world.store().find(b"kit").expect("bot").id;
    assert_eq!(
        world.store().scopes(id, b"kit-api"),
        Some(
            [b"read:user".as_slice().into(), b"write:issue".as_slice().into(), b"write:repository".as_slice().into()]
                .as_slice()
        )
    );
    assert_eq!(
        world.store().scopes(id, b"kit-git"),
        Some([b"write:repository".as_slice().into(), b"read:user".as_slice().into()].as_slice())
    );
}
#[test]
fn a_token_file_that_answers_as_the_bot_is_kept_untouched() {
    let mut world = World::managed(3);
    fixtures::bot_tokens(&mut world);
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(world.file(Item::ApiToken), Some(&fixtures::secret(fixtures::API)));
    assert_eq!(world.file(Item::GitToken), Some(&fixtures::secret(fixtures::GIT)));
    assert_eq!(world.tally().routes.get(&Route::CreateToken), None);
    assert_eq!(world.tally().routes.get(&Route::DeleteToken), None);
}
#[test]
fn a_token_file_holding_another_users_token_is_made_again() {
    let mut world = World::managed(3);
    let bot = fixtures::bot_tokens(&mut world);
    let person = world.store_mut().account(b"person", false, b"A person");
    world.store_mut().token(person, b"persons-token", &[b"read:user"], fixtures::secret(b"a-persons-token"));
    world.put(Item::ApiToken, fixtures::secret(b"a-persons-token"));
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(world.store().token_user(world.file(Item::ApiToken).expect("new token").expose()), Some(bot));
    let deletes: Vec<_> =
        world.requests().iter().filter(|request| matches!(request, Request::DeleteToken { .. })).cloned().collect();
    assert_eq!(deletes, vec![Request::DeleteToken { login: b"kit".as_slice().into(), id: 2 }]);
    assert_eq!(world.tally().routes.get(&Route::DeleteToken), Some(&1));
}
#[test]
fn a_token_file_holding_a_secret_the_forge_never_made_is_made_again() {
    let mut world = World::managed(3);
    let bot = fixtures::bot_tokens(&mut world);
    world.put(Item::ApiToken, fixtures::secret(b"a-secret-the-forge-never-made"));
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(world.store().token_user(world.file(Item::ApiToken).expect("new token").expose()), Some(bot));
    assert_eq!(world.tally().routes.get(&Route::CreateToken), Some(&1));
    assert!(world.secrets_absent());
}
#[test]
fn an_owned_forge_makes_a_missing_org_for_its_person() {
    let mut world = World::owned(7);
    assert_eq!(world.reconcile(), Ended::Done);
    for name in [b"acme", b"work"] {
        assert_eq!(world.store().organization(name).expect("organization made").owner.as_ref(), b"ada");
    }
    assert_eq!(world.tally().routes.get(&Route::CreateOrg), Some(&2));
}
#[test]
fn a_managed_forge_reads_no_org() {
    let mut world = World::managed(7);
    assert_eq!(world.reconcile(), Ended::Done);
    assert!(!world.requests().iter().any(|request| matches!(request, Request::Org { .. })));
    assert!(world.secrets_absent());
}
#[test]
fn a_second_run_writes_nothing() {
    let mut world = fixtures::converged(7);
    let since = world.requests().len();
    let files = fixtures::file_writes(&world);
    assert_eq!(world.reconcile(), Ended::Done);
    assert!(fixtures::write_routes(&world, since).is_empty());
    assert_eq!(fixtures::file_writes(&world), files);
}
#[test]
fn a_deleted_bot_is_made_again() {
    let mut world = fixtures::converged(7);
    let since = world.requests().len();
    world.store_mut().delete_account(b"kit");
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(world.store().find(b"kit").expect("replacement bot").id, 4);
    assert_eq!(world.file(Item::User).expect("replacement id file").expose(), &4u64.to_le_bytes());
    assert_eq!(world.store().token_user(world.file(Item::ApiToken).expect("replacement API token").expose()), Some(4));
    assert_eq!(fixtures::write_routes(&world, since).iter().filter(|route| **route == Route::CreateUser).count(), 1);
}
#[test]
fn a_revoked_token_is_made_again() {
    let mut world = fixtures::converged(7);
    let old = world.file(Item::ApiToken).expect("first API token").clone();
    world.store_mut().revoke(old.expose());
    assert_eq!(world.reconcile(), Ended::Done);
    let replacement = world.file(Item::ApiToken).expect("replacement API token");
    assert_ne!(replacement, &old);
    assert_eq!(world.store().token_user(replacement.expose()), Some(3));
}
#[test]
fn a_person_under_the_bots_login_is_refused() {
    use skein_forge_setup::reconcile::{Failure, Stage, Why};
    let mut world = World::owned(7);
    world.store_mut().account(b"kit", false, b"A person");
    assert_eq!(
        world.reconcile(),
        Ended::Stop(Failure { forge: 1, step: Stage::Bot, subject: b"kit".as_slice().into(), why: Why::Unmarked })
    );
    assert_eq!(world.requests().last(), Some(&Request::UserOf { login: b"kit".as_slice().into() }));
    assert!(world.secrets_absent());
}
#[test]
fn no_secret_is_in_any_failure_debug_or_trace() {
    use skein_forge_setup::reconcile::{Failure, Stage, Why};
    let mut world = World::owned(7);
    fixtures::bot_tokens(&mut world);
    world.put(Item::AdminToken, fixtures::secret(b"unrecognized-admin-token"));
    assert_eq!(
        world.reconcile(),
        Ended::Stop(Failure {
            forge: 1,
            step: Stage::Admin,
            subject: b"kit-admin".as_slice().into(),
            why: Why::Refused(401)
        })
    );
    assert_eq!(world.requests(), &[Request::User]);
    assert!(world.secrets_absent());
}

#[test]
fn a_token_on_the_second_page_is_replaced() {
    use skein_forge::forgejo::request::Page;
    let mut world = World::managed(7);
    let bot = world.store_mut().account(b"kit", false, b"kit:bot");
    for (name, value) in [
        (b"other-one".as_slice(), b"first-unrelated-token".as_slice()),
        (b"other-two".as_slice(), b"second-unrelated-token".as_slice()),
        (b"kit-api".as_slice(), b"stale-api-token".as_slice()),
    ] {
        world.store_mut().token(bot, name, &[b"read:user"], fixtures::secret(value));
    }
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(world.store().token_user(b"stale-api-token"), None);
    assert_eq!(world.store().token_user(world.file(Item::ApiToken).expect("API token written").expose()), Some(bot));
    assert_eq!(
        world.store().scopes(bot, b"kit-api"),
        Some(
            [b"read:user".as_slice().into(), b"write:issue".as_slice().into(), b"write:repository".as_slice().into()]
                .as_slice()
        )
    );
    assert_eq!(world.tally().routes.get(&Route::DeleteToken), Some(&1));
    assert!(
        world.requests().contains(&Request::Tokens {
            login: b"kit".as_slice().into(),
            page: Page { page: Some(2), limit: Some(2) },
        })
    );
    assert!(world.secrets_absent());
}
#[test]
fn another_services_bot_is_unmarked() {
    use skein_forge_setup::reconcile::{Failure, Stage, Why};
    let mut world = World::managed(7);
    world.store_mut().account(b"kit", false, b"other:bot");
    assert_eq!(
        world.reconcile(),
        Ended::Stop(Failure { forge: 1, step: Stage::Bot, subject: b"kit".as_slice().into(), why: Why::Unmarked })
    );
    assert_eq!(world.requests().last(), Some(&Request::UserOf { login: b"kit".as_slice().into() }));
    for route in [Route::CreateUser, Route::CreateToken, Route::DeleteToken] {
        assert_eq!(world.tally().routes.get(&route), None);
    }
    assert!(world.secrets_absent());
}
