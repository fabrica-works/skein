//! Independent child roots, startup rollback and per-process metering.

use skein_io::kernel::{Error, Exit};
use skein_world::Cut;
use skein_world_tests::fixtures::startup::{
    AFTER_FIRST_APPEND, APPEND_DESCRIPTOR_LIMIT, BASE_HANDLES, KILLED_CHILDREN, Processes, READY,
    ROOT_DESCRIPTOR_LIMIT, Story, killed_child, launched, no_appends, only_parent, prefixed, processes,
    restart_appends, restart_child, restart_roots, restartable, trace_bytes,
};
use skein_world_tests::roots::{
    append_child, appends, file_child, missing_appends, missing_roots, roots, waiting_child,
};

#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;

const REPLAY: &[(bool, bool)] = &[(false, false), (true, false), (false, true)];

#[test]
fn independently_owned_roots_support_child_files_after_parent_close_and_settle_memory() {
    let mut outcome = launched(17, skein_sim::Config::calm(), file_child, roots, no_appends, Story::Exchange);
    let Processes { parent, child } = processes(&outcome);
    assert_eq!(parent.received, READY);
    assert_eq!(child.expect("child admitted").received, b"gofrom child");
    assert_eq!(parent.child_exit, Some(Exit::Code(0)));
    assert!(outcome.heap.as_ref().expect("metered processes").iter().all(|(peak, bound)| peak <= bound));
    outcome.machine.finish();
}

#[test]
fn killed_child_closes_all_startup_roots_under_its_own_memory_span() {
    let mut outcome = launched(19, skein_sim::Config::calm(), waiting_child, roots, no_appends, Story::Kill);
    assert_eq!(processes(&outcome).parent.child_exit, Some(Exit::Signal(9)));
    assert_eq!(outcome.killed.len(), KILLED_CHILDREN);
    assert!(killed_child(&outcome).heap.is_some());
    outcome.machine.finish();
}

#[test]
fn failed_startup_rolls_back_roots_and_never_admits_a_child() {
    let mut outcome = launched(23, skein_sim::Config::calm(), file_child, missing_roots, no_appends, Story::SpawnOnly);
    assert!(only_parent(&outcome).results.contains(&Err(Error::NotFound)));
    outcome.machine.finish();
}

#[test]
fn startup_root_stories_replay_completion_reordering() {
    for &(kill, missing) in REPLAY {
        let mut first = skein_world_tests::roots::simulated(31, kill, missing);
        let mut second = skein_world_tests::roots::simulated(31, kill, missing);
        assert_eq!(first.trace, second.trace, "kill {kill}, missing {missing}: startup roots preserve seeded replay");
        first.machine.finish();
        second.machine.finish();
    }
}

#[test]
fn startup_roots_refuse_a_child_beyond_its_descriptor_limit() {
    let mut outcome = launched(
        29,
        skein_sim::Config { max_fds: ROOT_DESCRIPTOR_LIMIT, ..skein_sim::Config::calm() },
        file_child,
        roots,
        no_appends,
        Story::SpawnOnly,
    );
    assert!(only_parent(&outcome).results.contains(&Err(Error::TooManyOpenFiles)));
    outcome.machine.finish();
}

#[test]
fn startup_append_files_survive_parent_close_and_all_owned_handles_settle() {
    let mut outcome = prefixed(37).run();
    assert_eq!(processes(&outcome).parent.received, READY);
    assert_eq!(trace_bytes(&mut outcome.machine), b"prefix:onetwo");
    outcome.machine.finish();
}

#[test]
fn a_failed_later_startup_append_rolls_back_files_and_roots_before_admission() {
    let mut outcome = launched(41, skein_sim::Config::calm(), append_child, roots, missing_appends, Story::SpawnOnly);
    assert!(only_parent(&outcome).results.contains(&Err(Error::NotFound)));
    outcome.machine.finish();
}

#[test]
fn a_killed_child_releases_its_startup_append_files_in_its_own_memory_span() {
    let mut outcome = launched(43, skein_sim::Config::calm(), waiting_child, roots, appends, Story::Kill);
    assert_eq!(outcome.killed.len(), KILLED_CHILDREN);
    assert!(killed_child(&outcome).heap.is_some());
    outcome.machine.finish();
}

#[test]
fn startup_append_descriptors_enter_the_childs_descriptor_limit() {
    let mut outcome = launched(
        47,
        skein_sim::Config { max_fds: APPEND_DESCRIPTOR_LIMIT, ..skein_sim::Config::calm() },
        append_child,
        roots,
        appends,
        Story::SpawnOnly,
    );
    assert!(only_parent(&outcome).results.contains(&Err(Error::TooManyOpenFiles)));
    outcome.machine.finish();
}

#[test]
fn a_restarted_process_appends_again_to_its_startup_file() {
    let (mut world, _) = restartable(53);
    let host = world.spawn_restartable_startup(restart_roots(), restart_appends(b"trace"), restart_child);
    world.cut(host, AFTER_FIRST_APPEND, Cut::Kill);
    let mut outcome = world.run();
    assert_eq!(trace_bytes(&mut outcome.machine.roots), b"oneonetwo");
    outcome.machine.roots.finish();
}

#[test]
fn a_failed_restartable_startup_append_closes_its_roots_and_fails() {
    let (mut world, handles) = restartable(59);
    let refusal = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        world.spawn_restartable_startup(restart_roots(), restart_appends(b"missing/trace"), restart_child);
    }))
    .expect_err("missing append fails startup");
    assert_eq!(
        refusal.downcast_ref::<String>().expect("world failure text"),
        "a restartable startup append trace opens: NotFound"
    );
    assert_eq!(handles.get(), BASE_HANDLES);
}
