//! The conformance suite against both backends on the real kernel, on loopback
//! and in scratch directories (kernel.md, 8): [`Real`], the chosen kernel as the
//! suite's backend, each process of a scenario a `Kernel` of its own, each
//! root a scratch directory of its own. The tests are
//! `tests/conformance.rs`.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::Path;
use std::process::Command;

use skein_conformance::{Backend as ConformanceBackend, Item, Made};
use skein_io::kernel::{Complete, Error, Fd, Submit};
use skein_lib::{Duration, Queue, Time};
use skein_scratch::Scratch;
use skein_shell::{Backend, Clock, Config, Kernel, Wait, open_root};

/// Operations in flight per process: a scenario has a few at most.
const OPERATIONS: u32 = 16;

/// The longest one wait blocks on one process's ring before the others are
/// entered: their deferred completions run only when they are.
const SLICE: Duration = Duration::from_millis(5);

/// One forced backend per process, and a scratch directory per root, removed
/// with the backend.
pub struct Real {
    backend: Backend,
    kernels: Vec<Kernel>,
    clock: Clock,
    scratches: Vec<Scratch>,
}

impl Real {
    #[must_use]
    pub fn new(backend: Backend) -> Real {
        Real { backend, kernels: Vec::new(), clock: Clock::new(), scratches: Vec::new() }
    }

    /// Whether the kernel checks this process's permissions on its files:
    /// not for root, whom no mode stops.
    #[must_use]
    pub fn permissions_checked() -> bool {
        let scratch = Scratch::new("owner");
        fs::metadata(scratch.path()).expect("a scratch directory has metadata").uid() != 0
    }

    fn kernel(&mut self, process: usize) -> &mut Kernel {
        self.kernels.get_mut(process).expect("a process of this backend")
    }
}

impl ConformanceBackend for Real {
    type Process = usize;

    fn open(&mut self) -> usize {
        let kernel = match Kernel::open_on(Config { operations: OPERATIONS }, self.backend) {
            Ok(kernel) => kernel,
            Err(error) => panic!("{:?} is not usable here, so it cannot be tested: {error}", self.backend),
        };
        self.kernels.push(kernel);
        self.kernels.len().checked_sub(1).expect("the kernel just pushed")
    }

    fn submit(&mut self, process: usize, records: &mut Queue<Submit>) {
        self.kernel(process).submit(records, Wait::No);
    }

    fn reap(&mut self, process: usize, completions: &mut Queue<Complete>) {
        self.kernel(process).reap(completions);
    }

    fn now(&self) -> Time {
        self.clock.now().now
    }

    fn enter(&mut self, process: usize) {
        self.kernel(process).submit(&mut Queue::with_capacity(0), Wait::No);
    }

    /// Enters every other process's kernel to process completions and readiness,
    /// then waits on this one for a slice of `bound`.
    fn pass(&mut self, process: usize, bound: Duration) {
        let mut nothing = Queue::with_capacity(0);
        for (other, kernel) in self.kernels.iter_mut().enumerate() {
            if other != process {
                kernel.submit(&mut nothing, Wait::No);
            }
        }
        let until = self.now().saturating_add(bound.min(SLICE));
        self.kernel(process).submit(&mut nothing, Wait::Until(until));
    }

    /// Blocks the thread: no backend is entered, while the kernel's network
    /// runs on.
    fn sleep(&mut self, span: Duration) {
        std::thread::sleep(std::time::Duration::from_nanos(span.as_nanos()));
    }

    fn assert_settled(&self, process: usize) {
        let kernel = self.kernels.get(process).expect("a process of this backend");
        assert_eq!(kernel.in_flight(), 0, "nothing in flight on process {process}");
    }

    fn append(&mut self, _process: usize, root: Fd, path: &[u8], mode: u32) -> Result<Fd, Error> {
        skein_shell::open_append(root, path, mode)
    }

    /// A scratch directory holding `root`, laid out as `tree` says, and
    /// `outside` beside it, for a link to lead out to; opened as the shell
    /// opens a root.
    fn root(&mut self, _process: usize, tree: &[Item]) -> Fd {
        let scratch = Scratch::new("conformance");
        let outside = scratch.path().join("outside");
        fs::create_dir(&outside).expect("a directory beside the root");
        fs::write(outside.join("secret"), b"outside").expect("a file beside the root");
        let root = scratch.path().join("root");
        fs::create_dir(&root).expect("the root");
        for item in tree {
            let path = root.join(OsStr::from_bytes(&item.path));
            match &item.made {
                Made::File(bytes) => fs::write(&path, bytes).expect("a file laid"),
                Made::Directory => fs::create_dir(&path).expect("a directory laid"),
                Made::Link(target) => symlink(OsStr::from_bytes(target), &path).expect("a symbolic link laid"),
                Made::HardLink(target) => {
                    fs::hard_link(root.join(OsStr::from_bytes(target)), &path).expect("a hard link laid");
                }
                // No FIFO without libc's mkfifo, which is unsafe: the
                // program does it.
                Made::Fifo => {
                    let made = Command::new("mkfifo").arg(&path).status().expect("mkfifo runs");
                    assert!(made.success(), "a FIFO laid");
                }
            }
        }
        // Modes last, from the deepest, so that none stops the laying.
        for item in tree.iter().rev() {
            match item.made {
                Made::File(_) | Made::Directory | Made::Fifo => {
                    mode(&root.join(OsStr::from_bytes(&item.path)), item.mode);
                }
                Made::Link(_) | Made::HardLink(_) => {}
            }
        }
        let fd = open_root(&root).expect("a scratch directory opens as a root");
        self.scratches.push(scratch);
        fd
    }
}

fn mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("a mode given");
}
