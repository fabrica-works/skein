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
fn a_managed_forge_missing_an_org_is_refused_naming_it() {
    use skein_forge_setup::reconcile::{Failure, Stage, Why};
    let mut world = World::managed(7);
    world.missing_org(b"absent");
    assert_eq!(
        world.reconcile(),
        Ended::Stop(Failure { forge: 1, step: Stage::Org, subject: b"absent".as_slice().into(), why: Why::OrgMissing })
    );
    assert_eq!(world.requests().last(), Some(&Request::Org { name: b"absent".as_slice().into() }));
    assert!(world.secrets_absent());
}
#[test]
fn a_team_named_for_the_bot_is_made_with_write_on_every_repository() {
    use skein_forge::forgejo::document::{Access, TeamSpec, Units};
    let mut world = World::managed(7);
    assert_eq!(world.reconcile(), Ended::Done);
    let bot = world.store().find(b"kit").expect("bot").id;
    let expected = TeamSpec {
        name: b"kit".as_slice().into(),
        permission: Access::Write,
        includes_all_repositories: true,
        units: Units { code: Access::Write, issues: Access::Write, pulls: Access::Write, actions: Access::Write },
    };
    for org in [b"acme", b"work"] {
        let team = world.store().team(org, b"kit").expect("team made");
        assert_eq!(team.spec, expected);
        assert_eq!(team.members.as_slice(), &[bot]);
    }
}
#[test]
fn a_listing_longer_than_a_page_is_read_to_its_end() {
    use skein_forge::forgejo::request::Page;
    let mut world = World::managed(7);
    for name in [b"first".as_slice(), b"second", b"kit"] {
        world.store_mut().add_team(b"acme", fixtures::team(name));
    }
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(world.tally().routes.get(&Route::CreateTeam), Some(&1), "only work needs a team");
    assert!(
        world
            .requests()
            .contains(&Request::Teams { org: b"acme".as_slice().into(), page: Page { page: Some(2), limit: Some(2) } })
    );
}
#[test]
fn an_org_hook_is_made_with_the_secret_the_events_json_and_active() {
    use skein_forge::forgejo::delivery::SUBSCRIBED;
    let mut world = World::owned(7);
    world.listening(fixtures::listener());
    world.put(Item::WebhookSecret, fixtures::secret(fixtures::WEBHOOK));
    assert_eq!(world.reconcile(), Ended::Done);
    for org in [b"acme", b"work"] {
        let hooks: Vec<_> = world.store().subscriptions(org).collect();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].url.as_ref(), fixtures::URL);
        assert_eq!(hooks[0].secret.expose(), fixtures::WEBHOOK);
        assert_eq!(hooks[0].events.iter().map(AsRef::as_ref).collect::<Vec<&[u8]>>(), SUBSCRIBED);
        assert!(hooks[0].active);
    }
    assert!(world.secrets_absent());
}
#[test]
fn the_webhook_secret_is_drawn_when_missing() {
    let mut world = World::managed(7);
    world.listening(fixtures::listener());
    assert_eq!(world.reconcile(), Ended::Done);
    let secret = world.file(Item::WebhookSecret).expect("secret drawn");
    assert_eq!(secret.len(), 64);
    assert!(secret.expose().iter().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte)));
    for org in [b"acme", b"work"] {
        assert_eq!(&world.store().subscriptions(org).next().expect("hook").secret, secret);
    }
}
#[test]
fn the_webhook_secret_is_kept_when_present() {
    let mut world = World::managed(7);
    world.listening(fixtures::listener());
    world.put(Item::WebhookSecret, fixtures::secret(fixtures::WEBHOOK));
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(world.file(Item::WebhookSecret), Some(&fixtures::secret(fixtures::WEBHOOK)));
    assert_eq!(
        world.trace().iter().filter(|line| line.as_str() == "Draw(32)").count(),
        1,
        "only the bot password is drawn"
    );
}
#[test]
fn two_hooks_with_the_listeners_url_keep_the_lowest_patched_and_the_rest_deleted() {
    let mut world = World::managed(7);
    world.listening(fixtures::listener());
    world.put(Item::WebhookSecret, fixtures::secret(fixtures::WEBHOOK));
    let low = world.store_mut().hook(b"acme", fixtures::URL, fixtures::secret(b"old-secret-low"));
    let high = world.store_mut().hook(b"acme", fixtures::URL, fixtures::secret(b"old-secret-high"));
    let other = world.store_mut().hook(b"acme", b"http://elsewhere.invalid/hook", fixtures::secret(b"other-secret"));
    assert_eq!(world.reconcile(), Ended::Done);
    let hooks: Vec<_> = world.store().subscriptions(b"acme").collect();
    assert_eq!(hooks.iter().map(|hook| hook.id).collect::<Vec<_>>(), vec![low, other]);
    assert_eq!(hooks[0].secret.expose(), fixtures::WEBHOOK);
    assert!(hooks[0].active);
    assert_eq!(
        (hooks[1].url.as_ref(), hooks[1].secret.expose(), hooks[1].active),
        (b"http://elsewhere.invalid/hook".as_slice(), b"other-secret".as_slice(), false)
    );
    assert!(world.requests().contains(&Request::DeleteOrgHook { org: b"acme".as_slice().into(), id: high }));
}
#[test]
fn a_second_run_writes_only_the_hooks_patches() {
    let mut world = fixtures::converged(7);
    let since = world.requests().len();
    let files = fixtures::file_writes(&world);
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(fixtures::write_routes(&world, since), vec![Route::EditOrgHook, Route::EditOrgHook]);
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
fn a_removed_team_member_is_added_again() {
    let mut world = fixtures::converged(7);
    let team = world.store().team(b"acme", b"kit").expect("team").id;
    let since = world.requests().len();
    world.store_mut().remove_member(team, 3);
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(world.store().team(b"acme", b"kit").expect("repaired team").members.as_slice(), &[3]);
    assert_eq!(fixtures::write_routes(&world, since), vec![Route::AddMember, Route::EditOrgHook, Route::EditOrgHook]);
}
#[test]
fn a_deleted_hook_is_made_again() {
    let mut world = fixtures::converged(7);
    let id = world.store().subscriptions(b"acme").next().expect("first hook").id;
    world.store_mut().delete_hook(id);
    assert_eq!(world.reconcile(), Ended::Done);
    let hooks: Vec<_> = world.store().subscriptions(b"acme").collect();
    assert_eq!(hooks.len(), 1);
    assert_ne!(hooks[0].id, id);
    assert_eq!(hooks[0].url.as_ref(), fixtures::URL);
    assert_eq!(&hooks[0].secret, world.file(Item::WebhookSecret).expect("webhook secret"));
}
#[test]
fn a_team_edited_by_a_person_is_repaired() {
    let mut world = fixtures::converged(7);
    let id = world.store().team(b"acme", b"kit").expect("team").id;
    let since = world.requests().len();
    world.store_mut().edit_team(id, fixtures::stale_team());
    assert_eq!(world.reconcile(), Ended::Done);
    assert_eq!(world.store().team(b"acme", b"kit").expect("repaired team").spec, fixtures::team(b"kit"));
    assert_eq!(fixtures::write_routes(&world, since), vec![Route::EditTeam, Route::EditOrgHook, Route::EditOrgHook]);
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
