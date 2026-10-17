use skein_sim::{Event, Pid};
use skein_world::{Digests, Host, Machine, NoMachine, StateDigests};
use skein_world_tests::fixtures::{exchange, scripts};
use skein_world_tests::{Act, Script, ms};

#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;

#[test]
fn an_outcome_gives_each_process_s_digest_and_the_machine_s() {
    let outcome = exchange(7, Digests::Untraced, 3).run();
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
        let outcome = exchange(seed, Digests::Untraced, 3).run();
        let digests: StateDigests = outcome.state_digests();
        (outcome.trace.iter().map(|entry| format!("{entry:?}")).collect(), digests)
    };
    assert!(!skein_world::domain::assert_replays(11, 12, run).is_empty());
}

#[test]
fn untraced_a_world_s_trace_holds_no_digest() {
    let outcome = exchange(7, Digests::Untraced, 3).run();
    assert!(!outcome.trace.iter().any(|entry| matches!(entry.event, Event::Digest(_))));
}

#[test]
fn traced_each_process_s_digest_comes_between_its_iterate_and_its_submit() {
    let mut world = scripts(7, vec![]);
    for _ in 0..2 {
        world.spawn(|| Script::new(ms(0), &[Act::Socket]));
    }
    world.digests(Digests::Traced);
    let outcome = world.run();
    for pid in 0..2 {
        let mut entries = outcome.trace.iter().filter(|entry| entry.pid.raw() == pid);
        assert!(matches!(entries.next().expect("first entry").event, Event::Digest(_)), "pid {pid}");
        assert!(matches!(entries.next().expect("second entry").event, Event::Submit { .. }), "pid {pid}");
    }
}

#[test]
fn traced_the_machine_s_digest_follows_each_iteration() {
    let outcome = exchange(7, Digests::Traced, 3).run();
    let entries: Vec<_> = outcome.trace.iter().filter(|entry| entry.pid == Pid::MACHINE).collect();
    assert_eq!(entries.len(), usize::try_from(outcome.iterations).expect("iterations fit"));
    for entry in entries {
        assert_eq!(entry.event, Event::Digest([7; 32]));
    }
    assert_eq!(outcome.trace.last().expect("an iteration").pid, Pid::MACHINE);
}

#[test]
fn traced_two_runs_that_part_differ_first_at_a_digest() {
    let first = exchange(7, Digests::Traced, 1).run();
    let second = exchange(7, Digests::Traced, 0).run();
    let (left, right) = first
        .trace
        .iter()
        .zip(&second.trace)
        .find(|(left, right)| left != right)
        .expect("the scripts hold different noted state");
    assert!(matches!(left.event, Event::Digest(_)));
    assert!(matches!(right.event, Event::Digest(_)));
    assert_eq!(left.pid.raw(), 1);
    assert_eq!(right.pid.raw(), 1);
}
