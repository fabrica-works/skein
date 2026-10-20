use skein_forge_protocol_world::fixtures;
use skein_lib::Duration;

#[test]
fn a_decision_is_committed_before_its_call() {
    let world = fixtures::lost_creation(23);
    assert_eq!(world.creations(), 1);
    assert_eq!(world.released_writes(), 1);
    assert!(world.attempt(1).is_some());
    assert!(!world.made(1));
    assert_eq!(world.owner_counts(), (0, 0));
}

#[test]
fn old_generation_answers_are_discarded() {
    let mut world = fixtures::lost_creation(23);
    let attempt = world.attempt(1).expect("committed first attempt");
    world.restart();
    world.run_for(Duration::from_secs(1));
    assert_eq!(world.discarded(), 1);
    assert_eq!(world.generation(), 1);
    assert_eq!(world.owner_counts(), (0, 0));
    assert_eq!(world.attempt(1), Some(attempt));
}
