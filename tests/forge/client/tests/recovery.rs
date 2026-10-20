pub mod fixtures;
use skein_forge_client::{Made, Outcome, Resource, Watch, What};
use skein_forge_client_world::{REPO, Settings, World};

#[test]
fn an_uncertain_branch_is_found_without_retry_after_restart() {
    let mut world = World::new(Settings::calm(41));
    let head = world.branch(b"main").expect("fixture main branch");
    world.land_writes_late(skein_lib::Duration::from_secs(5));
    world.make(fixtures::branch_entry(head));
    world.run_for(1);
    let first = world.entry(1).expect("attempt retained").attempt.expect("write was sent");
    assert_eq!(world.stats().writes, 1);
    assert_eq!(world.branch(b"kit/7"), None);
    world.calm();
    world.restart();
    world.run_for(8);
    assert_eq!(world.entry(1).expect("deadline retained").attempt, Some(first));
    assert_eq!(world.stats().writes, 1, "restart cannot renew the first deadline or send early");
    world.run_for(5);
    assert_eq!(world.stats().writes, 1, "a found branch is never sent again");
    assert_eq!(world.branch(b"kit/7"), Some(head));
    world.run_for(10);
    assert_eq!(world.stats().late_landings, 1);
    assert_eq!(world.branch(b"kit/7"), Some(head));
    assert!(
        world
            .outcomes()
            .iter()
            .any(|(entry, outcome)| { *entry == 1 && matches!(outcome, Outcome::Made { made: Made::Branch(_), .. }) })
    );
    world.finish();
}

#[test]
fn a_deleted_uncertain_branch_is_held_after_its_deadline() {
    let (mut world, _, _) = fixtures::held_branch();
    assert_eq!(world.stats().writes, 1);
    assert!(world.outcomes().contains(&(1, Outcome::Held)));
    world.finish();
}

#[test]
fn restart_reads_afresh_before_settling_the_outbox() {
    let mut world = World::new(Settings::calm(41));
    let resource = Resource { repository: REPO, what: What::Branch(Box::from(&b"main"[..])) };
    world.keep(Box::new([Watch { resource: resource.clone(), participating: false }]));
    world.writer(resource.clone(), true);
    world.run_for(1);
    world.land_writes_late(skein_lib::Duration::from_secs(5));
    world.make(fixtures::issue(1));
    world.run_for(1);
    let attempt = world.entry(1).expect("attempt retained").attempt.expect("write was sent");
    assert_eq!(world.stats().writes, 1);

    world.calm();
    world.restart();
    world.run_for(1);
    assert_eq!(world.recovery_completions(), (1, 1));
    assert_eq!(world.entry(1).expect("attempt retained while waiting").attempt, Some(attempt));
    assert_eq!(world.stats().writes, 1);
    world.run_for(30);
    assert_eq!(world.recovery_completions(), (1, 1));
    assert_eq!(world.stats().writes, 1);
    assert_eq!(world.stats().creations, 1);
    world.writer(resource, false);
    world.finish();
}

#[test]
fn a_held_branch_is_written_again_only_after_remake() {
    let (mut world, entry, head) = fixtures::held_branch();
    world.run_for(30);
    assert_eq!(world.stats().writes, 1);
    assert_eq!(world.outcomes().last(), Some(&(entry.number, Outcome::Held)));
    world.restart();
    world.run_for(30);
    assert_eq!(world.stats().writes, 1);
    let outcomes = world.outcomes().len();
    world.remake(entry.clone());
    world.run_for(30);
    assert_eq!(world.stats().writes, 2);
    assert_eq!(world.branch(b"kit/7"), Some(head));
    assert_eq!(
        &world.outcomes()[outcomes..],
        &[(
            entry.number,
            Outcome::Made { made: Made::Branch(skein_forge_client_world::translate::commit(head)), found: false }
        )]
    );
    world.finish();
}

#[test]
fn a_writer_given_at_runtime_is_kept_across_restart() {
    let mut world = World::new(Settings { writer: false, ..Settings::calm(41) });
    world.make(fixtures::issue(1));
    assert!(world.outcomes().contains(&(1, Outcome::Failed(skein_forge::api::Error::Forbidden))));
    assert!(world.give_writer(1));
    let mut entry = fixtures::issue(2);
    let skein_forge::api::Write::CreateIssue { key, .. } = &mut entry.effect.write else {
        panic!("issue fixture creates an issue");
    };
    *key = Box::from(&b"\x01\x00\x03kitdeployment:gate"[..]);
    world.make(entry);
    assert!(world.entry(2).expect("entry committed before restart").attempt.is_none());
    world.restart();
    world.run_for(30);
    assert!(
        world
            .outcomes()
            .iter()
            .any(|(entry, outcome)| { *entry == 2 && matches!(outcome, Outcome::Made { made: Made::Created(_), .. }) })
    );
    assert_eq!(world.stats().writes, 1);
    assert_eq!(world.stats().creations, 1);
    world.finish();
}
