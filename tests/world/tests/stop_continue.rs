//! A supervised tree records stop and continue without starting cleanup.

use skein_io::kernel::{Exit, Signal};
use skein_lib::{Duration, Time};
use skein_shell::Clock;
use skein_world::end_to_end::{Binary, Cleanup, Command, Expectation, Mode};
use skein_world::{Referee, real};

struct Judge {
    stage: u8,
    next: Option<Time>,
}

impl Referee<Binary> for Judge {
    fn act(&mut self, _now: Time, procs: &mut [Binary]) {
        let child = &mut procs[0];
        let signal = match self.stage {
            0 if child.stderr() == b"ready\n" => Some(Signal::Stop),
            1 if !child.tree_status().stopped_pids.is_empty() => Some(Signal::Continue),
            2 if !child.tree_status().continued_pids.is_empty() => {
                assert_eq!(child.tree_status().stopped_pids, child.tree_status().continued_pids);
                assert_eq!(child.tree_status().cleanup(), Cleanup::None);
                assert_eq!(child.exit_status(), None);
                Some(Signal::Terminate)
            }
            _ => None,
        };
        if let Some(signal) = signal {
            child.signal_tree(signal);
            self.stage += 1;
        }
    }

    fn observe(&mut self, now: Time, _procs: &[Binary]) {
        self.next = (self.stage < 3).then_some(now);
    }

    fn next_deadline(&self) -> Option<Time> {
        self.next
    }

    fn overdue(&self, _now: Time) -> Option<String> {
        None
    }

    fn passed(&self) -> bool {
        self.stage == 3
    }
}

#[test]
fn a_tree_records_stop_and_continue_without_cleanup() {
    // One shell holds the tree alive until the referee terminates it.
    const STDERR_LIMIT: usize = 1024;
    for force_walk in [true, false] {
        let command = Command {
            program: "/bin/sh".into(),
            arguments: vec!["-c".into(), "printf 'ready\\n' >&2; while :; do :; done".into()],
            environment: vec![],
            directory: "/tmp".into(),
        };
        let binary = Binary::start_with_tree(command, Mode::Pipes, STDERR_LIMIT, Expectation::Supervise, force_walk)
            .expect("the supervised program starts");
        let mut world = real::World::new(Judge { stage: 0, next: None });
        world.spawn_with_fds(binary.descriptors(), || binary);
        let outcome = world.run(&Clock::new(), Duration::from_secs(1));
        let child = &outcome.procs[0];
        assert_eq!(child.exit_status(), Some(Exit::Signal(15)));
        assert_eq!(child.tree_status().stopped_pids.len(), 1);
        assert_eq!(child.tree_status().continued_pids, child.tree_status().terminated_pids);
    }
}
