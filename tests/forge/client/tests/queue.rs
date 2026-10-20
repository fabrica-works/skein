pub mod fixtures;
use fixtures::merge_entry;
use skein_forge::api::Error;
use skein_forge_client::{Made, Outcome};
use skein_forge_client_world::{Settings, World};

fn queued() -> (World, u64) {
    let mut world = World::new(Settings { queue: true, ..Settings::calm(60) });
    let pull = world.historical_pull();
    let head = world.branch(b"topic").expect("fixture pull head");
    world.make(merge_entry(pull, head));
    world.run_for(1);
    assert_eq!(world.outcomes(), &[(1, Outcome::Queued)]);
    assert!(world.entry(1).expect("queued entry durable").queued);
    (world, pull)
}
#[test]
fn a_queued_merge_that_lands_settles_once() {
    let (mut world, pull) = queued();
    world.land_queued(pull);
    world.run_for(10);
    let merged = skein_forge_client_world::translate::commit(world.branch(b"main").expect("merged base"));
    assert_eq!(
        world.outcomes(),
        &[(1, Outcome::Queued), (1, Outcome::Made { made: Made::Merged(merged), found: false })]
    );
    assert_eq!(world.stats().writes, 1);
    world.finish();
}
#[test]
fn a_queued_merge_that_is_ejected_settles_once() {
    let (mut world, pull) = queued();
    world.eject_queued(pull);
    world.run_for(10);
    assert_eq!(world.outcomes(), &[(1, Outcome::Queued), (1, Outcome::Failed(Error::Refused))]);
    assert_eq!(world.stats().writes, 1);
    world.finish();
}
#[test]
fn a_queued_merge_restored_is_read_not_sent() {
    let (mut world, pull) = queued();
    world.restart();
    world.run_for(1);
    assert!(world.entry(1).expect("restored queue entry").queued);
    assert_eq!(world.recovery_completions(), (1, 1));
    world.land_queued(pull);
    world.run_for(10);
    let merged = skein_forge_client_world::translate::commit(world.branch(b"main").expect("merged base"));
    assert_eq!(
        world.outcomes(),
        &[(1, Outcome::Queued), (1, Outcome::Made { made: Made::Merged(merged), found: false })]
    );
    assert_eq!(world.stats().writes, 1);
    world.finish();
}
