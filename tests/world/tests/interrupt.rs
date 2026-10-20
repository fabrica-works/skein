//! A supervised interrupt is handled once while the tree remains alive.

use skein_io::kernel::{Exit, Signal};
use skein_lib::{Duration, Time};
use skein_shell::Clock;
use skein_world::end_to_end::{Binary, Cleanup, Command, Expectation, Mode};
use skein_world::{Referee, real};

struct Judge {
    sent: bool,
    handled_at: Option<Time>,
    checked: bool,
    next: Option<Time>,
}

impl Referee<Binary> for Judge {
    fn act(&mut self, now: Time, procs: &mut [Binary]) {
        let child = &mut procs[0];
        if !self.sent && child.stderr() == b"ready\n" {
            child.signal_tree(Signal::Interrupt);
            self.sent = true;
        }
        if self.sent && self.handled_at.is_none() && child.stderr().ends_with(b"interrupted\n") {
            self.handled_at = Some(now);
        }
        if !self.checked && self.handled_at.is_some_and(|at| now >= at.saturating_add(Duration::from_millis(25))) {
            assert_eq!(child.stderr(), b"ready\ninterrupted\n", "an interrupt never starts a repeated sweep");
            assert_eq!(child.exit_status(), None, "the handler lets the program continue");
            assert!(child.counts().is_none(), "a live tree retains its keeper");
            assert_eq!(child.tree_status().interrupted_pids.len(), 1);
            assert_eq!(child.tree_status().cleanup(), Cleanup::None);
            assert!(child.tree_status().terminated_pids.is_empty());
            assert!(child.tree_status().killed_pids.is_empty());
            child.signal_tree(Signal::Kill);
            self.checked = true;
        }
    }

    fn observe(&mut self, now: Time, procs: &[Binary]) {
        let child = &procs[0];
        self.next = if self.checked {
            None
        } else if let Some(at) = self.handled_at {
            Some(at.saturating_add(Duration::from_millis(25)))
        } else {
            (child.stderr() == b"ready\n" || child.stderr().ends_with(b"interrupted\n")).then_some(now)
        };
    }

    fn next_deadline(&self) -> Option<Time> {
        self.next
    }

    fn overdue(&self, _now: Time) -> Option<String> {
        None
    }

    fn passed(&self) -> bool {
        self.checked
    }
}

#[test]
fn a_tree_interrupt_is_handled_without_starting_cleanup() {
    for force_walk in [true, false] {
        let command = Command {
            program: "/bin/sh".into(),
            arguments: vec![
                "-c".into(),
                "trap 'printf \"interrupted\\n\" >&2' INT; printf 'ready\\n' >&2; while :; do :; done".into(),
            ],
            environment: vec![],
            directory: "/tmp".into(),
        };
        let binary = Binary::start_with_tree(command, Mode::Pipes, 1024, Expectation::Supervise, force_walk)
            .expect("the supervised program starts");
        let mut world = real::World::new(Judge { sent: false, handled_at: None, checked: false, next: None });
        world.spawn_with_fds(binary.descriptors(), || binary);
        let outcome = world.run(&Clock::new(), Duration::from_secs(1));
        let child = &outcome.procs[0];
        assert_eq!(child.exit_status(), Some(Exit::Signal(9)));
        assert_eq!(child.tree_status().cleanup(), Cleanup::Kill);
        assert_eq!(child.tree_status().interrupted_pids, child.tree_status().killed_pids);
    }
}
