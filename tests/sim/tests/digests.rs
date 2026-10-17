use skein_lib::Time;
use skein_sim::{Event, Pid};
use skein_sim_tests::World;

#[test]
fn a_traced_digest_is_an_entry_at_the_present_time_under_its_pid() {
    let mut world = World::calm();
    let pid = world.spawn();
    world.sim.advance_to(Time::from_nanos(37));
    world.sim.trace_digest(pid, [7; 32]);
    let last = world.sim.trace().last().expect("an entry");
    assert_eq!(last.at, Time::from_nanos(37));
    assert_eq!(last.pid.raw(), 0);
    assert_eq!(last.event, Event::Digest([7; 32]));
    assert_ne!(pid, Pid::MACHINE, "no process has the machine's pid");
}

#[test]
fn a_machine_digest_renders_under_the_machine_not_a_pid() {
    let mut world = World::calm();
    world.sim.trace_digest(Pid::MACHINE, [3; 32]);
    assert_eq!(Pid::MACHINE.to_string(), "machine");
    assert!(world.sim.render_trace().contains("ns  machine  Digest([3, 3,"));
    assert_eq!(world.sim.trace().len(), 1);
    assert_eq!(world.sim.trace()[0].event, Event::Digest([3; 32]));
}
