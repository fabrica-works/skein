use skein_world::{Host, Machine, Memory, NoMachine, StateDigests, World};
use skein_world_tests::{Expect, Judge, Script, Stamped, client, ms, server};

#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;

fn exchange(seed: u64) -> skein_world::Outcome<Script, Stamped> {
    let judge = Judge::new(seed, vec![Expect::Receives { at: 0, by: ms(1_000) }]);
    let mut world = World::<Script, Judge>::new(seed, skein_sim::Config::chaos(), judge, Memory::Checked)
        .with_machine(Stamped([7; 32]));
    world.spawn(server);
    world.spawn(|| client(ms(10), ms(20)).noting(3));
    world.run()
}

#[test]
fn an_outcome_gives_each_process_s_digest_and_the_machine_s() {
    let outcome = exchange(7);
    let digests = outcome.state_digests();
    assert_eq!(digests.procs.len(), 2);
    assert!(digests.procs.iter().all(Option::is_some));
    assert_ne!(digests.procs[0], digests.procs[1]);
    assert_eq!(digests.procs, outcome.procs.iter().map(Host::state_digest).collect::<Vec<_>>());
    assert_eq!(digests.machine, Some([7; 32]), "the machine's, as it gives it");
}

#[test]
fn the_default_digests_of_a_host_and_a_machine_are_none() {
    let host = skein_world_tests::hosted::Script::new(&[]);
    assert_eq!(host.state_digest(), None);
    assert_eq!(NoMachine.state_digest(), None);
}

#[test]
fn a_seed_replays_to_the_same_state_digests_through_assert_replays() {
    let run = |seed| {
        let outcome = exchange(seed);
        let digests: StateDigests = outcome.state_digests();
        (outcome.trace.iter().map(|entry| format!("{entry:?}")).collect(), digests)
    };
    assert!(!skein_world::domain::assert_replays(11, 12, run).is_empty());
}
