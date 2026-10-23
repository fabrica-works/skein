use super::BACKEND;
use skein_conformance::Check;
use skein_conformance::processes;
use skein_conformance_real::Real;

#[test]
fn child_processes_and_pipes() {
    processes(&mut Real::new(BACKEND), env!("CARGO_BIN_EXE_process_fixture").as_bytes()).check();
}

#[test]
fn the_group_is_signalled_after_its_leader_exits() {
    skein_conformance::groups(&mut Real::new(BACKEND), env!("CARGO_BIN_EXE_process_fixture").as_bytes()).check();
}

#[test]
fn usage_counts_the_child_only_after_reaping() {
    skein_conformance::usage(&mut Real::new(BACKEND), env!("CARGO_BIN_EXE_process_fixture").as_bytes()).check();
}

#[test]
fn window_on_a_descriptor_that_is_not_a_terminal_answers_other() {
    skein_conformance::window(&mut Real::new(BACKEND)).check();
}
