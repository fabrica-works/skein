//! Locating a repository by owner and name (forge-client.md, 5).
use skein_forge::api;
use skein_forge_client_world::{Settings, World};

#[test]
fn a_locate_answers_the_forges_id_for_a_repository_by_name() {
    let mut world = World::new(Settings::calm(5));
    world.read(1, api::Read::Locate { owner: Box::from(&b"org"[..]), name: Box::from(&b"repo"[..]) });
    world.run_for(1);
    assert_eq!(world.read_result(1), Some(&Ok(api::Answer::Located { repository: 2 })));
    world.finish();
}

#[test]
fn a_locate_of_a_repository_the_forge_lacks_answers_missing() {
    let mut world = World::new(Settings::calm(5));
    world.read(1, api::Read::Locate { owner: Box::from(&b"org"[..]), name: Box::from(&b"absent"[..]) });
    world.run_for(1);
    assert_eq!(world.read_result(1), Some(&Err(api::Error::Missing)));
    world.finish();
}
