//! A pidfd-backed child, retained exit and ordered pipe tokens (io.md, section 6).
//! It keeps an observing wait and at most one signal, never a numeric PID.
//! `signal` targets the child or its group. `close` requests the group kill;
//! `release` reaps only after that kill, the exit and every pipe close settle.
//!
//! | Phase | Completion | Next |
//! |---|---|---|
//! | Spawning | Spawned | Running, observing Wait |
//! | Running | exit observed | Exited, Exited told once |
//! | Running, Exited | close | group Kill after an owner signal settles |
//! | Exited | kill and pipes settled | Reaping, reaping Wait |
//! | Reaping | reap | Releasing, pidfd Close |
//! | Releasing | Close | Closed, terminal told |

use alloc::boxed::Box;
use skein_lib::{Id, List, Queue, Token};

use crate::kernel::{self, Done, Fd, Op, Signal, Spawn, Submit, Target, Way};
use crate::layer::{Entity, Io, Landed, Purpose};
use crate::records::{Error, Event};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Spawning,
    Running,
    Exited,
    Reaping,
    Releasing,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Signalling {
    Idle,
    Owner,
    Closing,
}

#[derive(Debug)]
pub(crate) struct Child {
    owner: Token,
    pipes: Box<[Id<Entity>]>,
    pidfd: Option<Fd>,
    phase: Phase,
    signalling: Signalling,
    closing: bool,
    killed: bool,
}

impl Child {
    fn new(owner: Token, pipes: Box<[Id<Entity>]>) -> Child {
        Child {
            owner,
            pipes,
            pidfd: None,
            phase: Phase::Spawning,
            signalling: Signalling::Idle,
            closing: false,
            killed: false,
        }
    }
    pub(crate) const fn is_closed(&self) -> bool {
        match self.phase {
            Phase::Closed => true,
            Phase::Spawning | Phase::Running | Phase::Exited | Phase::Reaping | Phase::Releasing => false,
        }
    }
}

pub(crate) fn spawn(io: &mut Io, owner: Token, mut spawn: Spawn, subs: &mut Queue<Submit>) {
    for (at, spec) in spawn.pipes.iter().enumerate() {
        let before = spawn.pipes.get(..at).expect("entry prefix is within the table");
        for prior in before {
            if prior.child == spec.child {
                io.tables.refused.push(owner);
                return;
            }
        }
    }
    for spec in &mut spawn.pipes {
        match spec.way {
            Way::Given(token) => {
                let fd = match io.entities.get(Id::from_token(token)) {
                    Some(Entity::Listener(listener)) => listener.held_fd(),
                    Some(
                        Entity::Named(_)
                        | Entity::Joined(_)
                        | Entity::Stream(_)
                        | Entity::Pipe(_)
                        | Entity::Child(_)
                        | Entity::Signals(_),
                    )
                    | None => None,
                };
                let Some(fd) = fd else {
                    io.tables.refused.push(owner);
                    return;
                };
                spec.parent = Some(fd);
            }
            Way::In | Way::Out => {}
        }
    }
    let Some(count) = pipe_count(&spawn) else {
        io.tables.refused.push(owner);
        return;
    };
    let Some(needed) = count.checked_add(1) else {
        io.tables.refused.push(owner);
        return;
    };
    if needed > io.entities.capacity().saturating_sub(io.entities.len()) {
        io.tables.refused.push(owner);
        return;
    }
    let mut pipes = List::with_capacity(count);
    for spec in &spawn.pipes {
        if !is_pipe(spec.way) {
            continue;
        }
        let id = io
            .entities
            .insert(Entity::Pipe(crate::pipe::Pipe::pending(spec.way)))
            .expect("reserved a slot for every pipe");
        pipes.push(id).expect("reserved one token per pipe");
    }
    let id = io
        .entities
        .insert(Entity::Child(Child::new(owner, pipes.into_boxed())))
        .expect("reserved a slot for the child");
    io.tables.submit(subs, id, Purpose::Spawn, Op::Spawn { spawn: Box::new(spawn) });
}

#[expect(clippy::too_many_lines, reason = "one exhaustive child completion table includes every kernel result")]
pub(crate) fn landed(io: &mut Io, landed: Landed, up: &mut Queue<Event>, subs: &mut Queue<Submit>) {
    let Landed { entity: id, purpose, result, kind, .. } = landed;
    match purpose {
        Purpose::Spawn => {
            let Op::Spawn { spawn } = kind else { unreachable!("a spawn completion returns its command") };
            match result {
                Ok(Done::Spawned { pidfd, sender }) => {
                    let previous =
                        io.tables.senders.insert(id.token(), sender).expect("at most one sender per child entity");
                    assert!(previous.is_none(), "a child records its sender once");
                    let Some(Entity::Child(child)) = io.entities.get_mut(id) else {
                        unreachable!("a child owns spawn")
                    };
                    assert_eq!(
                        pipe_count(&spawn).expect("spawn pipe count fits u32"),
                        u32::try_from(child.pipes.len()).expect("pipe entity count fits u32"),
                        "an entity per real pipe"
                    );
                    child.pidfd = Some(pidfd);
                    child.phase = Phase::Running;
                    let owner = child.owner;
                    let pipe_ids = child.pipes.clone();
                    let count = u32::try_from(pipe_ids.len()).expect("pipe count fits u32");
                    let mut tokens = List::with_capacity(count);
                    let mut ids = pipe_ids.iter();
                    for spec in &spawn.pipes {
                        if !is_pipe(spec.way) {
                            continue;
                        }
                        let fd = spec.parent.expect("a successful spawn fills every parent end");
                        let pipe_id = *ids.next().expect("one id per fd");
                        let Some(Entity::Pipe(pipe)) = io.entities.get_mut(pipe_id) else {
                            unreachable!("the child owns its pipes")
                        };
                        pipe.activate(id, fd, spec.way);
                        io.tables.ready.mark(pipe_id);
                        tokens.push(pipe_id.token()).expect("one token per pipe");
                    }
                    io.tables.submit(subs, id, Purpose::Wait, Op::Wait { pidfd, reap: false });
                    up.push(Event::Spawned { owner, child: id.token(), pipes: tokens.into_boxed() });
                }
                Err(error) => {
                    let Some(Entity::Child(child)) = io.entities.get_mut(id) else {
                        unreachable!("a child owns spawn")
                    };
                    let owner = child.owner;
                    let pipe_ids = child.pipes.clone();
                    child.phase = Phase::Closed;
                    for pipe in pipe_ids {
                        io.entities.retire(pipe);
                    }
                    up.push(Event::Failed { owner, error: spawn_error(error) });
                    up.push(Event::Closed { owner });
                }
                Ok(
                    Done::Nothing
                    | Done::Count(_)
                    | Done::Fd(_)
                    | Done::Accepted { .. }
                    | Done::Bound(_)
                    | Done::Stat(_)
                    | Done::Exit(_)
                    | Done::Usage(_)
                    | Done::Signalled(_)
                    | Done::Window(_),
                ) => {
                    unreachable!("a spawn answers with a pidfd and pipes")
                }
            }
        }
        Purpose::Wait => {
            let reaping = match io.entities.get(id) {
                Some(Entity::Child(child)) => child.phase == Phase::Reaping,
                Some(Entity::Named(_) | Entity::Joined(_)) => unreachable!("a child owns wait"),
                Some(Entity::Pipe(_) | Entity::Listener(_) | Entity::Stream(_) | Entity::Signals(_)) | None => {
                    unreachable!("a child owns wait")
                }
            };
            if reaping {
                assert!(result.is_ok(), "a reaping wait follows an observed exit");
                let fd = match io.entities.get_mut(id) {
                    Some(Entity::Child(child)) => {
                        child.phase = Phase::Releasing;
                        child.pidfd.expect("a child has its pidfd until close")
                    }
                    Some(Entity::Named(_) | Entity::Joined(_)) => unreachable!("a child owns wait"),
                    Some(Entity::Pipe(_) | Entity::Listener(_) | Entity::Stream(_) | Entity::Signals(_)) | None => {
                        unreachable!("a child owns wait")
                    }
                };
                io.tables.submit(subs, id, Purpose::Close, Op::Close { fd });
                return;
            }
            let Some(Entity::Child(child)) = io.entities.get_mut(id) else { unreachable!("a child owns wait") };
            let _sender = io.tables.senders.remove(&id.token());
            match result {
                Ok(Done::Exit(exit)) => up.push(Event::Exited { owner: child.owner, exit }),
                Err(_) => up.push(Event::Failed { owner: child.owner, error: Error::Other }),
                Ok(
                    Done::Nothing
                    | Done::Count(_)
                    | Done::Fd(_)
                    | Done::Accepted { .. }
                    | Done::Bound(_)
                    | Done::Stat(_)
                    | Done::Spawned { .. }
                    | Done::Usage(_)
                    | Done::Signalled(_)
                    | Done::Window(_),
                ) => {
                    unreachable!("a wait answers with an exit")
                }
            }
            child.phase = Phase::Exited;
        }
        Purpose::Signal => {
            drop(result);
            let Some(Entity::Child(child)) = io.entities.get_mut(id) else { unreachable!("a child owns signal") };
            match child.signalling {
                Signalling::Closing => child.killed = true,
                Signalling::Owner => {}
                Signalling::Idle => unreachable!("a submitted signal has its purpose"),
            }
            child.signalling = Signalling::Idle;
        }
        Purpose::Close => {
            let Some(Entity::Child(child)) = io.entities.get_mut(id) else { unreachable!("a child owns close") };
            child.phase = Phase::Closed;
            up.push(Event::Closed { owner: child.owner });
        }
        Purpose::Socket
        | Purpose::Bind
        | Purpose::Listen
        | Purpose::Accept
        | Purpose::Connect
        | Purpose::Recv
        | Purpose::Send
        | Purpose::Shutdown
        | Purpose::Discard
        | Purpose::ReadSignal
        | Purpose::Window { .. }
        | Purpose::Usage { .. }
        | Purpose::PipeRead
        | Purpose::PipeWrite
        | Purpose::Cancel(_) => unreachable!("a child only spawns, waits, signals and closes"),
    }
    begin_close(io, id, subs);
    release(io, id, subs);
}

pub(crate) fn signal(io: &mut Io, token: Token, signal: Signal, to: Target, subs: &mut Queue<Submit>) {
    let id = Id::<Entity>::from_token(token);
    let Some(Entity::Child(child)) = io.entities.get(id) else { return };
    match child.phase {
        Phase::Running | Phase::Exited => {}
        Phase::Spawning | Phase::Reaping | Phase::Releasing | Phase::Closed => return,
    }
    if child.signalling != Signalling::Idle || child.closing {
        return;
    }
    let Some(pidfd) = child.pidfd else { return };
    let Some(Entity::Child(child)) = io.entities.get_mut(id) else { unreachable!("a live child remains") };
    child.signalling = Signalling::Owner;
    io.tables.submit(subs, id, Purpose::Signal, Op::Signal { pidfd, signal, to });
}

pub(crate) fn close(io: &mut Io, id: Id<Entity>, subs: &mut Queue<Submit>) {
    let Some(Entity::Child(child)) = io.entities.get_mut(id) else { return };
    match child.phase {
        Phase::Running | Phase::Exited => child.closing = true,
        Phase::Spawning | Phase::Reaping | Phase::Releasing | Phase::Closed => return,
    }
    begin_close(io, id, subs);
    release(io, id, subs);
}

fn begin_close(io: &mut Io, id: Id<Entity>, subs: &mut Queue<Submit>) {
    let Some(Entity::Child(child)) = io.entities.get_mut(id) else { return };
    if !child.closing || child.killed || child.signalling != Signalling::Idle {
        return;
    }
    let pidfd = child.pidfd.expect("a closing child has its pidfd");
    child.signalling = Signalling::Closing;
    io.tables.submit(subs, id, Purpose::Signal, Op::Signal { pidfd, signal: Signal::Kill, to: Target::Group });
}

pub(crate) fn release(io: &mut Io, id: Id<Entity>, subs: &mut Queue<Submit>) {
    let Some(Entity::Child(child)) = io.entities.get(id) else { return };
    if child.phase != Phase::Exited || child.signalling != Signalling::Idle {
        return;
    }
    for pipe in &child.pipes {
        match io.entities.get(*pipe) {
            Some(Entity::Pipe(pipe)) if !pipe.is_closed() => return,
            Some(Entity::Named(_) | Entity::Joined(_)) => unreachable!("a child owns pipes"),
            Some(Entity::Pipe(_) | Entity::Listener(_) | Entity::Stream(_) | Entity::Child(_) | Entity::Signals(_))
            | None => {}
        }
    }
    if !child.killed {
        let child = match io.entities.get_mut(id) {
            Some(Entity::Child(child)) => child,
            Some(Entity::Named(_) | Entity::Joined(_)) => unreachable!("a releasing child remains"),
            Some(Entity::Pipe(_) | Entity::Listener(_) | Entity::Stream(_) | Entity::Signals(_)) | None => {
                unreachable!("a releasing child remains")
            }
        };
        child.closing = true;
        begin_close(io, id, subs);
        return;
    }
    let Some(fd) = child.pidfd else {
        return;
    };
    let Some(Entity::Child(child)) = io.entities.get_mut(id) else { unreachable!("a live child remains") };
    child.phase = Phase::Reaping;
    io.tables.submit(subs, id, Purpose::Wait, Op::Wait { pidfd: fd, reap: true });
}

fn spawn_error(error: kernel::Error) -> Error {
    match error {
        kernel::Error::TooManyOpenFiles | kernel::Error::NoBufferSpace => Error::Busy,
        kernel::Error::Refused
        | kernel::Error::Reset
        | kernel::Error::BrokenPipe
        | kernel::Error::NotConnected
        | kernel::Error::AddressInUse
        | kernel::Error::AddressNotAvailable
        | kernel::Error::Unreachable
        | kernel::Error::TimedOut
        | kernel::Error::Cancelled
        | kernel::Error::TooLate
        | kernel::Error::NotFound
        | kernel::Error::Exists
        | kernel::Error::NotADirectory
        | kernel::Error::IsADirectory
        | kernel::Error::NotEmpty
        | kernel::Error::Permission
        | kernel::Error::NoSpace
        | kernel::Error::ReadOnly
        | kernel::Error::TooManyLinks
        | kernel::Error::NameTooLong
        | kernel::Error::Escape
        | kernel::Error::Busy
        | kernel::Error::NotAFile
        | kernel::Error::InvalidArgument
        | kernel::Error::Other(_) => Error::Other,
    }
}

fn is_pipe(way: Way) -> bool {
    match way {
        Way::In | Way::Out => true,
        Way::Given(_) => false,
    }
}

fn pipe_count(spawn: &Spawn) -> Option<u32> {
    let mut count = 0_u32;
    for spec in &spawn.pipes {
        if is_pipe(spec.way) {
            count = count.checked_add(1)?;
        }
    }
    Some(count)
}
