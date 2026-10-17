//! The same exclusive lock contract on the real kernel and the simulator.
use crate::processes::{command, spawned};
use crate::run::Run;
use crate::{Backend, Check, Item};
use alloc::boxed::Box;
use skein_io::kernel::{Done, Error, Fd, Op, OpenHow, Signal, Target};

const NOTHING: Result<Done, Error> = Ok(Done::Nothing);

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Locks {
    pub taken: Result<Done, Error>,
    pub through_a_second_open: Result<Done, Error>,
    pub again_through_the_first: Result<Done, Error>,
    pub once_the_first_is_closed: Result<Done, Error>,
    pub beside_a_living_child: Result<Done, Error>,
}

impl Check for Locks {
    fn check(&self) {
        assert_eq!(self.taken, NOTHING, "the contract: the first open takes the lock");
        assert_eq!(self.through_a_second_open, Err(Error::Busy), "the contract: a second open is Busy");
        assert_eq!(self.again_through_the_first, NOTHING, "the contract: its own lock is already taken");
        assert_eq!(self.once_the_first_is_closed, NOTHING, "the contract: Close frees the lock");
        assert_eq!(self.beside_a_living_child, NOTHING, "the contract: a child never holds the lock");
    }
}

#[must_use]
pub fn lock<B: Backend>(backend: &mut B, executable: &[u8]) -> Locks {
    let mut run = Run::new(backend);
    let process = run.process();
    let root = run.root(process, &[Item::directory(b"store")]);
    let first = run.open(process, root, b"store", OpenHow::Directory).expect("a directory");
    let second = run.open(process, root, b"store", OpenHow::Directory).expect("a directory");
    let taken = run.lock(process, first);
    let through_a_second_open = run.lock(process, second);
    let again_through_the_first = run.lock(process, first);
    run.close(process, first);
    let once_the_first_is_closed = run.lock(process, second);
    let never = command(executable, root, &[b"never"], &[]);
    let (child, _) = spawned(run.call(process, Op::Spawn { spawn: Box::new(never) }));
    run.close(process, second);
    let third = run.open(process, root, b"store", OpenHow::Directory).expect("a directory");
    let beside_a_living_child = run.lock(process, third);
    ended(&mut run, process, child);
    run.close(process, third);
    run.close(process, root);
    run.finish();
    Locks { taken, through_a_second_open, again_through_the_first, once_the_first_is_closed, beside_a_living_child }
}

fn ended<B: Backend>(run: &mut Run<'_, B>, process: B::Process, child: Fd) {
    assert_eq!(
        run.call(process, Op::Signal { pidfd: child, signal: Signal::Kill, to: Target::Child }).result,
        NOTHING,
        "the contract: Kill signals the living child"
    );
    assert_eq!(
        run.call(process, Op::Wait { pidfd: child, reap: false }).result,
        Ok(Done::Exit(skein_io::kernel::Exit::Signal(9))),
        "the contract: a killed child reports its signal, observed and reaped"
    );
    assert_eq!(
        run.call(process, Op::Wait { pidfd: child, reap: true }).result,
        Ok(Done::Exit(skein_io::kernel::Exit::Signal(9))),
        "the contract: a killed child reports its signal, observed and reaped"
    );
    run.close(process, child);
}
