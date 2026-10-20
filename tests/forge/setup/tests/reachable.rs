use skein_fake_forge_protocol::Route;
use skein_forge::forgejo::{client, request::Method};
use skein_forge_setup::{
    item::Item,
    reachable::Reached,
    reconcile::{Failure, Outcome, Stage, Why},
};
use skein_forge_setup_world::{Ended, World, fixtures};

fn failure(stage: Stage, subject: &[u8], why: Why) -> Ended {
    Ended::Stop(Failure { forge: 1, step: stage, subject: subject.into(), why })
}
fn page(world: &World) -> (&[Reached], bool) {
    let Some(Outcome::Reachable { rows, more }) = world.outcome() else { panic!("picker outcome") };
    (rows, *more)
}
#[test]
fn the_admin_pages_through_every_repository_to_the_end() {
    let mut world = World::managed_paged(7, 4);
    let mut all = Vec::new();
    for (number, count, more) in [(1, 4, true), (2, 4, true), (3, 2, false)] {
        assert_eq!(world.reachable(None, number), Ended::Done);
        let (rows, found_more) = page(&world);
        assert_eq!(rows.len(), count);
        assert_eq!(found_more, more);
        all.extend_from_slice(rows);
    }
    assert_eq!(all.iter().map(|row| row.id).collect::<Vec<_>>(), (1..=10).collect::<Vec<_>>());
    for row in &all {
        let login = match row.id % 3 {
            1 => b"ada".as_slice(),
            2 => b"kit",
            0 => b"grace",
            _ => unreachable!("remainder"),
        };
        assert_eq!(row.owner.as_ref(), login);
        assert_eq!(row.owner_id, world.store().find(login).expect("owner fixture").id);
        assert_eq!(row.name.as_ref(), format!("repo-{}", row.id).as_bytes());
        assert_eq!(row.private, [2, 3, 6, 10].contains(&row.id));
    }
    assert!(all.iter().any(|row| row.owner.as_ref() == b"grace" && row.private));
    assert_eq!(world.tally().routes.get(&Route::RepoSearch), Some(&3));
}
#[test]
fn a_page_past_the_end_is_empty_and_has_no_more() {
    let mut world = World::managed_paged(7, 4);
    assert_eq!(world.reachable(None, 4), Ended::Done);
    assert_eq!(page(&world), ([].as_slice(), false));
}
#[test]
fn a_reachable_run_writes_nothing() {
    let mut world = World::managed_paged(7, 4);
    let writes = fixtures::file_writes(&world);
    let files: Vec<_> = [Item::AdminToken, Item::ApiToken, Item::GitToken, Item::User, Item::WebhookSecret]
        .into_iter()
        .map(|item| (item, world.file(item).cloned()))
        .collect();
    assert_eq!(world.reachable(None, 1), Ended::Done);
    let limits = skein_forge_machine_world::fixtures::fake_limits();
    for request in world.requests() {
        assert_eq!(client::encode(request, &limits.documents).expect("request encodes").method, Method::Get);
    }
    assert!(fixtures::write_routes(&world, 0).is_empty());
    assert_eq!(world.tally().total, 3);
    assert_eq!(world.tally().routes.len(), 3);
    for route in [Route::User, Route::ApiSettings, Route::RepoSearch] {
        assert_eq!(world.tally().routes.get(&route), Some(&1));
    }
    assert_eq!(fixtures::file_writes(&world), writes);
    for (item, value) in files {
        assert_eq!(world.file(item), value.as_ref());
    }
}
#[test]
fn a_plain_users_token_cannot_list() {
    let mut world = World::managed_paged(7, 4);
    let user = world.store().find(b"ada").expect("person fixture").id;
    let token = fixtures::secret(b"plain-person-token-for-reachable");
    world.store_mut().token(user, b"picker", &[b"read:user", b"write:repository"], token.clone());
    world.put(Item::AdminToken, token);
    assert_eq!(world.reachable(None, 1), failure(Stage::Admin, b"kit-admin", Why::NotAdmin));
    assert_eq!(world.tally().routes.get(&Route::RepoSearch), None);
    assert_eq!(world.requests(), &[skein_forge::forgejo::request::Request::User]);
}
#[test]
fn an_installation_given_to_a_forgejo_sends_no_request() {
    let mut world = World::managed_paged(7, 4);
    assert_eq!(world.reachable(Some(7), 1), failure(Stage::Installation, b"7", Why::Unexpected));
    assert!(world.requests().is_empty());
    assert_eq!(world.tally().total, 0);
}
#[test]
fn neither_token_kept_stops_missing() {
    let mut world = World::managed_paged(7, 4);
    world.forget(Item::AdminToken);
    world.forget(Item::ApiToken);
    assert_eq!(world.reachable(None, 1), failure(Stage::Token, b"api", Why::Missing));
    assert!(world.requests().is_empty());
}
#[test]
fn no_secret_is_in_a_reachable_failure_debug_or_trace() {
    let mut world = World::managed_paged(7, 4);
    let unknown = b"unknown-admin-token-for-reachable";
    world.put(Item::AdminToken, fixtures::secret(unknown));
    let ended = world.reachable(None, 1);
    assert_eq!(ended, failure(Stage::Admin, b"kit-admin", Why::Refused(401)));
    let debug = format!("{ended:?}");
    for secret in [unknown.as_slice(), fixtures::ADMIN] {
        assert!(!debug.as_bytes().windows(secret.len()).any(|bytes| bytes == secret));
    }
    assert!(world.secrets_absent());
}

#[test]
fn a_connected_forge_lists_the_repositories_its_user_can_write() {
    let mut world = fixtures::connected_picker(7);
    let mut all = Vec::new();
    for number in 1..=3 {
        assert_eq!(world.reachable(None, number), Ended::Done);
        all.extend_from_slice(page(&world).0);
    }
    assert_eq!(all.iter().map(|row| row.id).collect::<Vec<_>>(), [1, 2, 4, 5, 8, 10]);
    for (row, owner, private) in all
        .iter()
        .zip([b"kit".as_slice(), b"ada", b"ada", b"ada", b"grace", b"kit"])
        .zip([true, true, false, false, false, true])
        .map(|((row, owner), private)| (row, owner, private))
    {
        let owner_id = if owner == b"grace" {
            world.store().organization(owner).expect("org fixture").id
        } else {
            world.store().find(owner).expect("owner fixture").id
        };
        assert_eq!(row.owner.as_ref(), owner);
        assert_eq!(row.owner_id, owner_id);
        assert_eq!(row.name.as_ref(), format!("repo-{}", row.id).as_bytes());
        assert_eq!(row.private, private);
    }
}
#[test]
fn a_connected_page_may_hold_fewer_rows_and_still_have_more() {
    let mut world = fixtures::connected_picker(7);
    for (number, ids, more) in
        [(1, [1, 2, 4].as_slice(), true), (2, [5, 8].as_slice(), true), (3, [10].as_slice(), false)]
    {
        assert_eq!(world.reachable(None, number), Ended::Done);
        let (rows, found_more) = page(&world);
        assert_eq!(rows.iter().map(|row| row.id).collect::<Vec<_>>(), ids);
        assert_eq!(found_more, more);
    }
}
#[test]
fn a_kept_admin_token_decides_the_listing() {
    let mut world = fixtures::connected_picker(7);
    world.put(Item::AdminToken, fixtures::secret(fixtures::ADMIN));
    let since = world.requests().len();
    assert_eq!(world.reachable(None, 1), Ended::Done);
    assert_eq!(
        world.requests()[since..].iter().map(skein_fake_forge_protocol::route).collect::<Vec<_>>(),
        [Route::User, Route::ApiSettings, Route::RepoSearch]
    );
}
#[test]
fn forgetting_the_admin_token_makes_the_picker_the_connected_one() {
    let mut world = fixtures::connected_picker(7);
    world.put(Item::AdminToken, fixtures::secret(fixtures::ADMIN));
    world.reachable(None, 1);
    world.forget(Item::AdminToken);
    let since = world.requests().len();
    assert_eq!(world.reachable(None, 1), Ended::Done);
    assert_eq!(
        world.requests()[since..].iter().map(skein_fake_forge_protocol::route).collect::<Vec<_>>(),
        [Route::ApiSettings, Route::UserRepos]
    );
}
#[test]
fn a_revoked_api_token_stops_the_connected_picker_naming_api() {
    let mut world = fixtures::connected_picker(7);
    let token = world.file(Item::ApiToken).expect("API item").clone();
    world.store_mut().revoke(token.expose());
    assert_eq!(world.reachable(None, 1), failure(Stage::Token, b"api", Why::Refused(401)));
}
#[test]
fn a_connected_run_writes_nothing() {
    let mut world = fixtures::connected_picker(7);
    let since = world.requests().len();
    let writes = fixtures::file_writes(&world);
    let tally = world.tally().total;
    let routes: Vec<_> = world.tally().routes.iter().map(|(route, count)| (*route, *count)).collect();
    assert_eq!(world.reachable(None, 1), Ended::Done);
    let limits = skein_forge_machine_world::fixtures::fake_limits();
    for request in &world.requests()[since..] {
        assert_eq!(client::encode(request, &limits.documents).expect("request encodes").method, Method::Get);
    }
    assert!(fixtures::write_routes(&world, since).is_empty());
    assert_eq!(world.tally().total, tally + 2);
    for (route, count) in routes {
        if route != Route::ApiSettings && route != Route::UserRepos {
            assert_eq!(world.tally().routes.get(&route), Some(&count));
        }
    }
    assert_eq!(fixtures::file_writes(&world), writes);
}
#[test]
fn no_secret_is_in_a_connected_failure_debug_or_trace() {
    let mut world = fixtures::connected_picker(7);
    world.reachable(None, 1);
    let token = world.file(Item::ApiToken).expect("API item").clone();
    world.store_mut().revoke(token.expose());
    let ended = world.reachable(None, 1);
    assert_eq!(ended, failure(Stage::Token, b"api", Why::Refused(401)));
    assert!(!format!("{ended:?}").as_bytes().windows(token.len()).any(|bytes| bytes == token.expose()));
    assert!(world.secrets_absent());
}
