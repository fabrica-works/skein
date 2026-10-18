//! Shared startup scenarios and their named admission sizes.

use crate::hosted::{Act, Judge, Script};
use crate::roots::{RootFiles, parent};
use skein_fake_machine::{How, Opened};
use skein_io::kernel::{Error, Spawn};
use skein_lib::Queue;
use skein_sim::{Answer, Call, Config, Handle};
use skein_world::{
    Cut, HostedProgram, Inherited, Machine, Memory, Outcome, StartupAppend, StartupAppends, StartupRoot, StartupRoots,
    World,
};
use std::cell::Cell;
use std::rc::Rc;

/// The child announces readiness after its startup operations.
pub const READY: &[u8] = b"ready";

/// One hosted child; its script provisions eight operations.
pub const INSTANCES: u32 = 1;
/// Failed admission leaves the scenario's sole parent process.
pub const PARENT_PROCESSES: usize = 1;
/// A kill story terminates its single hosted child.
pub const KILLED_CHILDREN: usize = 1;
/// Without pipes, the sole inherited append occupies the first script slot.
pub const APPEND_SLOT: usize = 0;
pub const OPERATIONS: u32 = 8;
/// Two pipes, two roots and a signal need five descriptors.
pub const ROOT_DESCRIPTOR_LIMIT: u32 = 2 + 2;
/// Adding two append files needs seven descriptors.
pub const APPEND_DESCRIPTOR_LIMIT: u32 = ROOT_DESCRIPTOR_LIMIT + 2;
/// The machine's namespace itself holds one handle after startup rollback.
pub const BASE_HANDLES: usize = 1;
/// A first append completes before the next submission is cut.
pub const AFTER_FIRST_APPEND: u32 = 1;
/// The trace read covers the complete seeded prefix and both appended pieces.
pub const TRACE_READ_ROOM: u32 = 13;

#[must_use]
pub fn hosted(make: fn(&Spawn, &Inherited) -> Script) -> HostedProgram<Script> {
    HostedProgram { program: Box::from(&b"hosted"[..]), make, instances: INSTANCES, operations: OPERATIONS }
}

/// A spawn-only parent with the launch's directory argument.
#[must_use]
pub fn spawn_only(fd: skein_io::kernel::Fd) -> Script {
    let mut script = Script::parent(fd, &[Act::Spawn]);
    script.args = Box::new([Box::from(&b"."[..])]);
    script
}

/// An independently rooted simulated world, before admission.
#[must_use]
pub fn rooted(seed: u64, config: Config) -> (World<Script, Judge, RootFiles>, Handle) {
    let mut machine = RootFiles::new();
    let root = machine.parent_root();
    (World::new(seed, config, Judge, Memory::Checked).with_machine(machine), root)
}

/// Runs a hosted startup with a parent that exchanges, kills, or only spawns.
#[must_use]
pub fn launched(
    seed: u64,
    config: Config,
    make: fn(&Spawn, &Inherited) -> Script,
    roots: StartupRoots,
    appends: StartupAppends,
    story: Story,
) -> Outcome<Script, RootFiles> {
    let (mut world, root) = rooted(seed, config);
    world.host_startup(hosted(make), roots, appends);
    world.spawn_root(root, |fd| match story {
        Story::Exchange => parent(fd, b".", false),
        Story::Kill => parent(fd, b".", true),
        Story::SpawnOnly => spawn_only(fd),
    });
    world.run()
}

#[derive(Clone, Copy)]
pub enum Story {
    Exchange,
    Kill,
    SpawnOnly,
}

#[must_use]
pub fn no_appends(_spawn: &Spawn) -> Vec<StartupAppend> {
    Vec::new()
}

pub struct Processes<'a> {
    pub parent: &'a Script,
    pub child: Option<&'a Script>,
}

#[must_use]
pub fn processes(outcome: &Outcome<Script, RootFiles>) -> Processes<'_> {
    let mut processes = outcome.procs.iter();
    Processes { parent: processes.next().expect("parent admitted"), child: processes.next() }
}

#[must_use]
pub fn trace_bytes(machine: &mut RootFiles) -> Vec<u8> {
    let root = machine.open_root(b".").expect("inspect the trace");
    let file = machine.files.open(Opened::new(root.raw()), b"trace", How::Read).expect("trace");
    let bytes = machine.files.read(file, 0, TRACE_READ_ROOM).expect("trace bytes");
    machine.files.close(file);
    machine.close_root(root);
    bytes
}

pub fn prefix(machine: &mut RootFiles, root: Handle) {
    let file =
        machine.files.open(Opened::new(root.raw()), b"trace", How::Append { mode: 0o600 }).expect("existing trace");
    machine.files.append(file, b"prefix:").expect("trace prefix");
    machine.files.close(file);
}

/// The restart scenario's single root and named append file.
#[must_use]
pub fn restart_roots() -> Vec<StartupRoot> {
    vec![StartupRoot { name: Box::from(&b"workspace"[..]), path: Box::from(&b"."[..]) }]
}

#[must_use]
pub fn restart_appends(path: &[u8]) -> Vec<StartupAppend> {
    vec![StartupAppend {
        name: Box::from(&b"trace"[..]),
        root: Box::from(&b"."[..]),
        path: Box::from(path),
        mode: 0o600,
    }]
}

#[must_use]
pub fn restart_child(inherited: &Inherited) -> Script {
    Script::child(inherited, &[Act::AppendFile(APPEND_SLOT, b"one"), Act::AppendFile(APPEND_SLOT, b"two")])
}

/// The root machine with kill settlement and an externally visible rollback count.
pub struct RestartFiles {
    pub roots: RootFiles,
    pub handles: Rc<Cell<usize>>,
}
impl Default for RestartFiles {
    fn default() -> Self {
        Self { roots: RootFiles::new(), handles: Rc::new(Cell::new(BASE_HANDLES)) }
    }
}
impl Machine for RestartFiles {
    fn open_root(&mut self, path: &[u8]) -> Result<Handle, Error> {
        self.roots.open_root(path)
    }
    fn close_root(&mut self, root: Handle) {
        self.roots.close_root(root);
        self.handles.set(self.roots.files.open_handles());
    }
    fn open_append(&mut self, root: Handle, path: &[u8], mode: u32) -> Result<Handle, Error> {
        self.roots.open_append(root, path, mode)
    }
    fn cut(&mut self, cut: Cut, held: &[Handle], _seed: u64) {
        assert_eq!(cut, Cut::Kill);
        for handle in held {
            self.close_root(*handle);
        }
    }
    fn step(&mut self, call: Call, answers: &mut Queue<Answer>) {
        self.roots.step(call, answers);
    }
}

/// A hosted append story whose trace already contains its prefix.
#[must_use]
pub fn prefixed(seed: u64) -> World<Script, Judge, RootFiles> {
    let mut machine = RootFiles::new();
    let root = machine.parent_root();
    prefix(&mut machine, root);
    let mut world = World::new(seed, Config::calm(), Judge, Memory::Checked).with_machine(machine);
    world.host_startup(hosted(crate::roots::append_child), crate::roots::roots, crate::roots::appends);
    world.spawn_root(root, |fd| parent(fd, b".", false));
    world
}

/// A cutting world with its rollback count available to the scenario.
#[must_use]
pub fn restartable(seed: u64) -> (World<Script, Judge, RestartFiles>, Rc<Cell<usize>>) {
    let machine = RestartFiles::default();
    let handles = machine.handles.clone();
    (World::new(seed, Config::calm(), Judge, Memory::Checked).with_machine(machine), handles)
}

/// The parent retained when startup refuses child admission.
#[must_use]
pub fn only_parent(outcome: &Outcome<Script, RootFiles>) -> &Script {
    assert_eq!(outcome.procs.len(), PARENT_PROCESSES);
    processes(outcome).parent
}

/// A kill story's sole terminated child.
#[must_use]
pub fn killed_child(outcome: &Outcome<Script, RootFiles>) -> &skein_world::Killed {
    let mut children = outcome.killed.iter();
    children.next().expect("child killed")
}
