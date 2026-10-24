//! Parent-death exec against a real process tree, with an ordinary exec control.
#![cfg(target_os = "linux")]

use std::fs;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

use skein_io::kernel::{Exit, Fd, Signal};
use skein_lib::Duration;
use skein_scratch::Scratch;
use skein_shell::Clock;

// The task's exit bound; startup uses the same bound so a broken helper fails promptly.
const PATIENCE: Duration = Duration::from_secs(5);
// Long enough to observe that ordinary exec survives, well below sleep's 30 seconds.
const SURVIVAL: Duration = Duration::from_millis(100);
const POLL: std::time::Duration = std::time::Duration::from_millis(1);

fn until(mut ready: impl FnMut() -> bool) {
    let clock = Clock::new();
    let deadline = clock.now().now.saturating_add(PATIENCE);
    while !ready() {
        assert!(clock.now().now < deadline, "process tree did not reach its expected state within 5 s");
        std::thread::sleep(POLL);
    }
}

struct Tree {
    parent: Child,
    child: Option<Fd>,
    scratch: Scratch,
}

impl Tree {
    fn start(mode: &str) -> Self {
        skein_shell::make_subreaper().expect("adopt and reap orphaned sleep");
        let scratch = Scratch::new("parent-death");
        let parent = Command::new(env!("CARGO_BIN_EXE_process_fixture"))
            .args(["parent-death-parent", mode])
            .current_dir(scratch.path())
            .process_group(0)
            .stdin(Stdio::piped())
            .spawn()
            .expect("spawn parent fixture");
        let mut tree = Self { parent, child: None, scratch };
        let mut pid = None;
        until(|| {
            pid = fs::read_to_string(tree.scratch.path().join("child-pid")).ok().and_then(|s| s.parse().ok());
            pid.is_some()
        });
        let pid = pid.expect("parent reported child PID");
        tree.child = Some(skein_shell::open_pidfd(pid).expect("keep child identity across exec"));
        tree.parent.stdin.take().expect("parent input").write_all(b"x").expect("release exec child");
        let sleep = fs::canonicalize("/bin/sleep").expect("sleep executable");
        until(|| fs::read_link(format!("/proc/{pid}/exe")).ok().as_ref() == Some(&sleep));
        tree
    }

    fn kill_parent(&mut self) {
        self.parent.kill().expect("SIGKILL parent");
        self.parent.wait().expect("reap parent");
    }

    fn child_exited(&self) -> bool {
        skein_shell::pidfd_exited(self.child.expect("registered child")).expect("poll sleep identity")
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        // The private group also covers a child not yet reported during failed startup.
        let _killed = skein_shell::signal_group(self.parent.id(), Signal::Kill);
        let _killed = self.parent.kill();
        let _reaped = self.parent.wait();
        if let Some(child) = self.child {
            skein_shell::abandon_binary(Some(child), &[child]);
        }
    }
}

#[test]
fn a_parent_killed_after_exec_takes_its_child_with_it() {
    let mut tree = Tree::start("protected");
    let clock = Clock::new();
    let start = clock.now().now;
    tree.kill_parent();
    until(|| tree.child_exited());
    let elapsed = clock.now().now.saturating_since(start);
    eprintln!("parent SIGKILL to sleep exit: {} ns", elapsed.as_nanos());
    assert!(elapsed < PATIENCE, "parent-death signal must end sleep within 5 s: {elapsed:?}");
    assert_eq!(
        skein_shell::poll_child(tree.child.expect("child identity"), false),
        Ok(Some(Exit::Signal(9))),
        "sleep died from the installed signal"
    );
}

#[test]
fn ordinary_exec_survives_its_parents_death() {
    let mut tree = Tree::start("plain");
    tree.kill_parent();
    let clock = Clock::new();
    let deadline = clock.now().now.saturating_add(SURVIVAL);
    while clock.now().now < deadline {
        assert!(!tree.child_exited(), "ordinary exec must leave sleep alive");
        std::thread::sleep(POLL);
    }
}

struct DirectChild(Child);

impl Drop for DirectChild {
    fn drop(&mut self) {
        let _killed = self.0.kill();
        let _reaped = self.0.wait();
    }
}

#[test]
fn a_wrong_parent_is_refused_before_exec() {
    let scratch = Scratch::new("wrong-parent");
    let stderr = fs::File::create(scratch.path().join("stderr")).expect("capture refusal");
    let mut child = DirectChild(
        Command::new(env!("CARGO_BIN_EXE_process_fixture"))
            .args(["parent-death-child", "--parent", "4294967295", "protected"])
            .current_dir(scratch.path())
            .stdin(Stdio::piped())
            .stderr(stderr)
            .spawn()
            .expect("spawn child with impossible parent PID"),
    );
    child.0.stdin.take().expect("child input").write_all(b"x").expect("release exec child");
    let mut status = None;
    until(|| {
        status = child.0.try_wait().expect("poll refused child");
        status.is_some()
    });
    assert!(!status.expect("child exited").success(), "wrong parent must fail");
    assert!(
        fs::read_to_string(scratch.path().join("stderr"))
            .expect("refusal message")
            .contains("parent-death exec refused: parent changed before exec"),
        "the child must report the parent mismatch instead of executing sleep"
    );
}
