//! Process ownership, one-way pipe streams, and the terminal order.

use alloc::boxed::Box;
use skein_lib::stream::{Down, Read, Up};

use super::{Kind, Rig, limits, owner};
use crate::kernel::{Done, Exit, Fd, Op, Pipe, Spawn, Way};
use crate::{Event, Limits, Request};

fn command(pipes: &[(u32, Way)]) -> Spawn {
    Spawn {
        program: Box::from(&b"/bin/true"[..]),
        args: Box::default(),
        env: Box::default(),
        root: Fd::new(3),
        dir: Box::from(&b"."[..]),
        pipes: {
            let mut specs = skein_lib::List::with_capacity(u32::try_from(pipes.len()).expect("small test"));
            for (child, way) in pipes {
                specs.push(Pipe { child: *child, way: *way, parent: None }).expect("room for each pipe");
            }
            specs.into_boxed()
        },
    }
}

#[test]
fn inherited_read_pipe_uses_stream_events_and_closes_once() {
    let mut rig = Rig::new(limits());
    let pipe = rig.io.adopt_read_pipe(Fd::new(40)).expect("room for an inherited pipe");
    let read = rig.next().take(Kind::PipeRead);
    let Op::PipeRead { fd, .. } = &read.kind else { panic!("readable pipe") };
    assert_eq!(*fd, Fd::new(40));
    rig.down(Request::Stream { stream: pipe, down: Down::Demand { read: Read::Fill(2), room: 0 } }).nothing();
    rig.next().nothing();
    let Op::PipeRead { fd, mut buf } = read.kind else { panic!("readable pipe") };
    for (to, from) in buf.iter_mut().zip(b"ok") {
        *to = *from;
    }
    let mut out =
        rig.complete(crate::kernel::Submit { op: read.op, kind: Op::PipeRead { fd, buf } }, Ok(Done::Count(2)));
    assert_eq!(out.events, [Event::Stream { owner: pipe, up: Up::Bytes(Box::from(&b"ok"[..])) }]);
    let read = out.take(Kind::PipeRead);
    assert_eq!(rig.complete(read, Ok(Done::Count(0))).events, [Event::Stream { owner: pipe, up: Up::End }]);
    let close = rig.down(Request::Close { entity: pipe }).take(Kind::Close);
    assert_eq!(close.kind, Op::Close { fd: Fd::new(40) });
    assert_eq!(rig.complete(close, Ok(Done::Nothing)).events, [Event::Closed { owner: pipe }]);
    rig.empty();
}

#[test]
fn inherited_write_pipe_uses_room_and_returns_descriptor_on_full_slab() {
    let mut rig = Rig::new(Limits { sockets: 1, ..limits() });
    let pipe = rig.io.adopt_write_pipe(Fd::new(41), None).expect("one slot");
    assert_eq!(rig.io.adopt_read_pipe(Fd::new(42)), Err(Fd::new(42)));
    rig.next().nothing();
    rig.down(Request::Stream { stream: pipe, down: Down::Demand { read: Read::Nothing, room: 2 } }).nothing();
    assert_eq!(rig.next().events, [Event::Stream { owner: pipe, up: Up::Room }]);
    let write =
        rig.down(Request::Stream { stream: pipe, down: Down::Send(Box::from(&b"ok"[..])) }).take(Kind::PipeWrite);
    let Op::PipeWrite { fd, .. } = &write.kind else { panic!("writable pipe") };
    assert_eq!(*fd, Fd::new(41));
    rig.complete(write, Ok(Done::Count(2))).nothing();
    let close = rig.down(Request::Stream { stream: pipe, down: Down::Finish }).take(Kind::Close);
    assert_eq!(close.kind, Op::Close { fd: Fd::new(41) });
    assert_eq!(rig.complete(close, Ok(Done::Nothing)).events, [Event::Closed { owner: pipe }]);
    rig.empty();
}

#[test]
fn inherited_write_pipe_finishes_after_an_in_flight_short_write() {
    let mut rig = Rig::new(limits());
    let pipe = rig.io.adopt_write_pipe(Fd::new(43), None).expect("room for an inherited pipe");
    rig.next().nothing();
    rig.down(Request::Stream { stream: pipe, down: Down::Demand { read: Read::Nothing, room: 3 } }).nothing();
    assert_eq!(rig.next().events, [Event::Stream { owner: pipe, up: Up::Room }]);
    let write =
        rig.down(Request::Stream { stream: pipe, down: Down::Send(Box::from(&b"end"[..])) }).take(Kind::PipeWrite);
    rig.down(Request::Stream { stream: pipe, down: Down::Finish }).nothing();
    let remaining = rig.complete(write, Ok(Done::Count(1))).take(Kind::PipeWrite);
    let Op::PipeWrite { from, .. } = &remaining.kind else { panic!("write continuation") };
    assert_eq!(*from, 1);
    let close = rig.complete(remaining, Ok(Done::Count(2))).take(Kind::Close);
    assert_eq!(close.kind, Op::Close { fd: Fd::new(43) });
    assert_eq!(rig.complete(close, Ok(Done::Nothing)).events, [Event::Closed { owner: pipe }]);
    rig.empty();
}

#[test]
fn a_childs_output_is_a_stream_and_the_child_closes_after_its_pipe() {
    let mut rig = Rig::new(Limits { sockets: 3, ..limits() });
    let mut spawn = rig.down(Request::Spawn { owner: owner(1), spawn: command(&[(9, Way::Out)]) }).take(Kind::Spawn);
    parent_ends(&mut spawn, &[Fd::new(12)]);
    let mut out = rig.complete(spawn, Ok(Done::Spawned { pidfd: Fd::new(11) }));
    let wait = out.take(Kind::Wait);
    assert_eq!(wait.kind, Op::Wait { pidfd: Fd::new(11), reap: false });
    let (child, pipe) = match out.events.as_slice() {
        [Event::Spawned { owner: got, child, pipes }] if *got == owner(1) && pipes.len() == 1 => (*child, pipes[0]),
        other => panic!("spawned child and pipe: {other:?}"),
    };
    let read = rig.next().take(Kind::PipeRead);
    rig.down(Request::Stream { stream: pipe, down: Down::Demand { read: Read::Fill(3), room: 0 } }).nothing();
    rig.next().nothing();
    let Op::PipeRead { fd, mut buf } = read.kind else { panic!("pipe read") };
    let first = buf.get_mut(..3).expect("the read has room");
    for (to, from) in first.iter_mut().zip(b"hey") {
        *to = *from;
    }
    let mut out =
        rig.complete(crate::kernel::Submit { op: read.op, kind: Op::PipeRead { fd, buf } }, Ok(Done::Count(3)));
    assert_eq!(out.events, [Event::Stream { owner: pipe, up: Up::Bytes(Box::from(&b"hey"[..])) }]);
    let read = out.take(Kind::PipeRead);
    assert_eq!(rig.complete(read, Ok(Done::Count(0))).events, [Event::Stream { owner: pipe, up: Up::End }]);
    assert_eq!(
        rig.complete(wait, Ok(Done::Exit(Exit::Code(7)))).events,
        [Event::Exited { owner: owner(1), exit: Exit::Code(7) }]
    );
    let close = rig.down(Request::Close { entity: pipe }).take(Kind::Close);
    let mut out = rig.complete(close, Ok(Done::Nothing));
    assert_eq!(out.events, [Event::Closed { owner: pipe }]);
    let killing = out.take(Kind::Signal);
    assert_eq!(
        killing.kind,
        Op::Signal { pidfd: Fd::new(11), signal: crate::kernel::Signal::Kill, to: crate::kernel::Target::Group }
    );
    let reaping = rig.complete(killing, Ok(Done::Nothing)).take(Kind::Wait);
    assert_eq!(reaping.kind, Op::Wait { pidfd: Fd::new(11), reap: true });
    let mut out = rig.complete(reaping, Ok(Done::Exit(Exit::Code(7))));
    assert!(out.events.is_empty(), "the reaping wait does not repeat Exited");
    let pidfd_close = out.take(Kind::Close);
    assert_eq!(pidfd_close.kind, Op::Close { fd: Fd::new(11) });
    assert_eq!(rig.complete(pidfd_close, Ok(Done::Nothing)).events, [Event::Closed { owner: owner(1) }]);
    rig.next().nothing();
    rig.empty();
    assert_ne!(child, pipe);
}

#[test]
fn a_childs_input_grants_room_continues_short_writes_and_finishes() {
    let mut rig = Rig::new(Limits { sockets: 3, ..limits() });
    let mut spawn = rig.down(Request::Spawn { owner: owner(2), spawn: command(&[(7, Way::In)]) }).take(Kind::Spawn);
    parent_ends(&mut spawn, &[Fd::new(22)]);
    let mut out = rig.complete(spawn, Ok(Done::Spawned { pidfd: Fd::new(21) }));
    let wait = out.take(Kind::Wait);
    assert_eq!(wait.kind, Op::Wait { pidfd: Fd::new(21), reap: false });
    let pipe = match out.events.as_slice() {
        [Event::Spawned { pipes, .. }] => pipes[0],
        other => panic!("spawned pipe: {other:?}"),
    };
    rig.next().nothing();
    rig.down(Request::Stream { stream: pipe, down: Down::Demand { read: Read::Nothing, room: 3 } }).nothing();
    assert_eq!(rig.next().events, [Event::Stream { owner: pipe, up: Up::Room }]);
    let write =
        rig.down(Request::Stream { stream: pipe, down: Down::Send(Box::from(&b"abc"[..])) }).take(Kind::PipeWrite);
    let continued = rig.complete(write, Ok(Done::Count(1))).take(Kind::PipeWrite);
    if let Op::PipeWrite { from: 1, .. } = continued.kind {
    } else {
        panic!("short write resumes at one")
    }
    rig.complete(continued, Ok(Done::Count(2))).nothing();
    let close = rig.down(Request::Stream { stream: pipe, down: Down::Finish }).take(Kind::Close);
    assert_eq!(rig.complete(close, Ok(Done::Nothing)).events, [Event::Closed { owner: pipe }]);
    let mut out = rig.complete(wait, Ok(Done::Exit(Exit::Code(0))));
    assert_eq!(out.events, [Event::Exited { owner: owner(2), exit: Exit::Code(0) }]);
    let killing = out.take(Kind::Signal);
    let reaping = rig.complete(killing, Ok(Done::Nothing)).take(Kind::Wait);
    assert_eq!(reaping.kind, Op::Wait { pidfd: Fd::new(21), reap: true });
    let close = rig.complete(reaping, Ok(Done::Exit(Exit::Code(0)))).take(Kind::Close);
    assert_eq!(rig.complete(close, Ok(Done::Nothing)).events, [Event::Closed { owner: owner(2) }]);
    rig.next().nothing();
    rig.empty();
}

pub(super) fn parent_ends(submit: &mut crate::kernel::Submit, ends: &[Fd]) {
    let Op::Spawn { spawn } = &mut submit.kind else { panic!("spawn record") };
    assert_eq!(spawn.pipes.len(), ends.len());
    for (pipe, fd) in spawn.pipes.iter_mut().zip(ends) {
        pipe.parent = Some(*fd);
    }
}

fn child_with_input(rig: &mut Rig) -> (skein_lib::Token, skein_lib::Token, crate::kernel::Submit) {
    let mut spawning = rig.down(Request::Spawn { owner: owner(1), spawn: command(&[(0, Way::In)]) }).take(Kind::Spawn);
    parent_ends(&mut spawning, &[Fd::new(41)]);
    let mut spawned = rig.complete(spawning, Ok(Done::Spawned { pidfd: Fd::new(40) }));
    let waiting = spawned.take(Kind::Wait);
    let (child, pipe) = match spawned.events.as_slice() {
        [Event::Spawned { child, pipes, .. }] => (*child, pipes[0]),
        other => panic!("a child with input: {other:?}"),
    };
    rig.next().nothing();
    (child, pipe, waiting)
}

#[test]
fn owner_signals_target_the_child_or_group_running_and_exited() {
    use crate::kernel::{Signal, Target};
    for exited in [false, true] {
        for target in [Target::Child, Target::Group] {
            for signal in [Signal::Interrupt, Signal::Terminate, Signal::Kill, Signal::Stop, Signal::Continue] {
                let mut rig = Rig::new(Limits { sockets: 2, ..limits() });
                let (child, pipe, waiting) = child_with_input(&mut rig);
                let waiting = if exited {
                    assert_eq!(
                        rig.complete(waiting, Ok(Done::Exit(Exit::Code(0)))).events,
                        [Event::Exited { owner: owner(1), exit: Exit::Code(0) }]
                    );
                    None
                } else {
                    Some(waiting)
                };
                let signalling = rig.down(Request::Signal { child, signal, to: target }).take(Kind::Signal);
                assert_eq!(signalling.kind, Op::Signal { pidfd: Fd::new(40), signal, to: target });
                rig.down(Request::Signal { child, signal: Signal::Kill, to: target }).nothing();
                rig.complete(signalling, Ok(Done::Nothing)).nothing();
                if let Some(waiting) = waiting {
                    assert_eq!(
                        rig.complete(waiting, Ok(Done::Exit(Exit::Code(0)))).events,
                        [Event::Exited { owner: owner(1), exit: Exit::Code(0) }]
                    );
                }
                let closing = rig.down(Request::Close { entity: pipe }).take(Kind::Close);
                let killing = rig.complete(closing, Ok(Done::Nothing)).take(Kind::Signal);
                assert_eq!(killing.kind, Op::Signal { pidfd: Fd::new(40), signal: Signal::Kill, to: Target::Group });
                let reaping = rig.complete(killing, Ok(Done::Nothing)).take(Kind::Wait);
                let closing = rig.complete(reaping, Ok(Done::Exit(Exit::Code(0)))).take(Kind::Close);
                assert_eq!(rig.complete(closing, Ok(Done::Nothing)).events, [Event::Closed { owner: owner(1) }]);
                rig.empty();
            }
        }
    }
}

#[test]
fn a_child_close_kills_its_group_after_an_in_flight_owner_signal_settles() {
    use crate::kernel::{Signal, Target};
    for exited in [false, true] {
        for owner_signal in [false, true] {
            let mut rig = Rig::new(Limits { sockets: 2, ..limits() });
            let (child, pipe, waiting) = child_with_input(&mut rig);
            let waiting = if exited {
                rig.complete(waiting, Ok(Done::Exit(Exit::Code(0))));
                None
            } else {
                Some(waiting)
            };
            let signalling = if owner_signal {
                Some(
                    rig.down(Request::Signal { child, signal: Signal::Terminate, to: Target::Child })
                        .take(Kind::Signal),
                )
            } else {
                None
            };
            let mut closing = rig.down(Request::Close { entity: child });
            let killing = match signalling {
                Some(signalling) => {
                    closing.nothing();
                    rig.complete(signalling, Ok(Done::Nothing)).take(Kind::Signal)
                }
                None => closing.take(Kind::Signal),
            };
            assert_eq!(killing.kind, Op::Signal { pidfd: Fd::new(40), signal: Signal::Kill, to: Target::Group });
            rig.down(Request::Signal { child, signal: Signal::Terminate, to: Target::Group }).nothing();
            rig.down(Request::Close { entity: child }).nothing();
            rig.complete(killing, Ok(Done::Nothing)).nothing();
            if let Some(waiting) = waiting {
                rig.complete(waiting, Ok(Done::Exit(Exit::Code(0))));
            }
            let closing = rig.down(Request::Close { entity: pipe }).take(Kind::Close);
            let reaping = rig.complete(closing, Ok(Done::Nothing)).take(Kind::Wait);
            assert_eq!(reaping.kind, Op::Wait { pidfd: Fd::new(40), reap: true });
            let closing = rig.complete(reaping, Ok(Done::Exit(Exit::Code(0)))).take(Kind::Close);
            rig.complete(closing, Ok(Done::Nothing));
            rig.down(Request::Signal { child, signal: Signal::Kill, to: Target::Group }).nothing();
            rig.empty();
        }
    }
}

#[test]
fn a_childs_output_is_read_as_it_arrives() {
    let mut rig = Rig::new(Limits { sockets: 3, ..limits() });
    let mut spawn = rig.down(Request::Spawn { owner: owner(1), spawn: command(&[(1, Way::Out)]) }).take(Kind::Spawn);
    parent_ends(&mut spawn, &[Fd::new(12)]);
    let out = rig.complete(spawn, Ok(Done::Spawned { pidfd: Fd::new(11) }));
    let pipe = match out.events.as_slice() {
        [Event::Spawned { pipes, .. }] => pipes[0],
        other => panic!("spawned pipe: {other:?}"),
    };
    let read = rig.next().take(Kind::PipeRead);
    rig.down(Request::Stream { stream: pipe, down: Down::Demand { read: Read::Arrived { max: 64 }, room: 0 } })
        .nothing();
    rig.next().nothing();
    let Op::PipeRead { fd, mut buf } = read.kind else { panic!("pipe read") };
    for (to, from) in buf.iter_mut().zip(b"hey") {
        *to = *from;
    }
    let mut out =
        rig.complete(crate::kernel::Submit { op: read.op, kind: Op::PipeRead { fd, buf } }, Ok(Done::Count(3)));
    assert_eq!(out.events, [Event::Stream { owner: pipe, up: Up::Bytes(Box::from(&b"hey"[..])) }]);
    let read = out.take(Kind::PipeRead);
    rig.down(Request::Stream { stream: pipe, down: Down::Demand { read: Read::Arrived { max: 64 }, room: 0 } })
        .nothing();
    rig.next().nothing();
    assert_eq!(rig.complete(read, Ok(Done::Count(0))).events, [Event::Stream { owner: pipe, up: Up::End }]);
}

#[test]
#[expect(clippy::disallowed_macros, reason = "verify Debug redacts environment values")]
fn spawn_debug_prints_env_names_without_values() {
    let mut spawn = command(&[]);
    spawn.env = Box::new([Box::from(&b"NAME=secret-value"[..]), Box::from(&b"NO_EQUALS"[..])]);
    for debug in [alloc::format!("{spawn:?}"), alloc::format!("{:?}", Request::Spawn { owner: owner(1), spawn })] {
        assert!(debug.contains(&alloc::format!("{:?}", b"NAME".as_slice())));
        assert!(debug.contains(&alloc::format!("{:?}", b"NO_EQUALS".as_slice())));
        assert!(!debug.contains("secret-value"));
        assert!(!debug.contains(&alloc::format!("{:?}", b"NAME=secret-value".as_slice())));
        assert!(!debug.contains("115, 101, 99, 114, 101, 116"));
    }
}

#[test]
fn given_entries_resolve_without_pipe_entities_and_preserve_pipe_order() {
    let mut rig = Rig::new(Limits { sockets: 4, ..limits() });
    let held = rig.io.adopt_listener(Fd::new(50), super::local(8000)).expect("held listener");
    let mut spawning = rig
        .down(Request::Spawn { owner: owner(1), spawn: command(&[(7, Way::In), (3, Way::Given(held)), (9, Way::Out)]) })
        .take(Kind::Spawn);
    assert_eq!(rig.io.entities.len(), 4, "listener, child and two pipes fill the slab");
    let Op::Spawn { spawn } = &mut spawning.kind else { panic!("spawn") };
    assert_eq!(spawn.pipes[1].parent, Some(Fd::new(50)));
    assert!(spawning.kind.is_valid(), "resolved Given is a valid backend submission");
    let Op::Spawn { spawn } = &mut spawning.kind else { panic!("spawn") };
    spawn.pipes[0].parent = Some(Fd::new(51));
    spawn.pipes[2].parent = Some(Fd::new(52));
    let out = rig.complete(spawning, Ok(Done::Spawned { pidfd: Fd::new(53) }));
    let [Event::Spawned { pipes, .. }] = out.events.as_slice() else { panic!("spawned") };
    assert_eq!(pipes.len(), 2);
    let (input, output) = (pipes[0], pipes[1]);
    let read = rig.next().take(Kind::PipeRead);
    let Op::PipeRead { fd, .. } = &read.kind else { panic!("output pipe") };
    assert_eq!(*fd, Fd::new(52));
    rig.down(Request::Stream { stream: input, down: Down::Demand { read: Read::Nothing, room: 1 } }).nothing();
    assert_eq!(rig.next().events, [Event::Stream { owner: input, up: Up::Room }]);
    let write =
        rig.down(Request::Stream { stream: input, down: Down::Send(Box::from(&b"x"[..])) }).take(Kind::PipeWrite);
    let Op::PipeWrite { fd, .. } = &write.kind else { panic!("input pipe") };
    assert_eq!(*fd, Fd::new(51));
    rig.complete(write, Ok(Done::Count(1))).nothing();
    rig.down(Request::Stream { stream: output, down: Down::Demand { read: Read::Fill(1), room: 0 } }).nothing();
    rig.next().nothing();
    assert_eq!(rig.complete(read, Ok(Done::Count(0))).events, [Event::Stream { owner: output, up: Up::End }]);
    assert_eq!(rig.io.tables.held.get(&super::local(8000)).copied(), Some(skein_lib::Id::from_token(held)));
}

#[test]
fn a_given_listener_stays_held_after_exit_and_can_be_given_again() {
    let mut rig = Rig::new(Limits { sockets: 2, ..limits() });
    let held = rig.io.adopt_listener(Fd::new(50), super::local(8000)).expect("held listener");
    for pidfd in [Fd::new(51), Fd::new(52)] {
        let spawning =
            rig.down(Request::Spawn { owner: owner(1), spawn: command(&[(3, Way::Given(held))]) }).take(Kind::Spawn);
        let Op::Spawn { spawn } = &spawning.kind else { panic!("spawn") };
        assert_eq!(spawn.pipes[0].parent, Some(Fd::new(50)));
        let mut out = rig.complete(spawning, Ok(Done::Spawned { pidfd }));
        let [Event::Spawned { pipes, .. }] = out.events.as_slice() else { panic!("spawned") };
        assert!(pipes.is_empty());
        let waiting = out.take(Kind::Wait);
        rig.next().nothing();
        let mut out = rig.complete(waiting, Ok(Done::Exit(Exit::Code(0))));
        assert_eq!(out.events, [Event::Exited { owner: owner(1), exit: Exit::Code(0) }]);
        let killing = out.take(Kind::Signal);
        let reaping = rig.complete(killing, Ok(Done::Nothing)).take(Kind::Wait);
        let closing = rig.complete(reaping, Ok(Done::Exit(Exit::Code(0)))).take(Kind::Close);
        assert_eq!(closing.kind, Op::Close { fd: pidfd });
        assert_eq!(rig.complete(closing, Ok(Done::Nothing)).events, [Event::Closed { owner: owner(1) }]);
        rig.next().nothing();
        assert_eq!(rig.io.entities.len(), 1);
    }
    let closing = rig.down(Request::Close { entity: held }).take(Kind::Close);
    assert_eq!(closing.kind, Op::Close { fd: Fd::new(50) });
    assert_eq!(rig.complete(closing, Ok(Done::Nothing)).events, [Event::Closed { owner: held }]);
    rig.empty();
}

fn refused_spawn(rig: &mut Rig, specs: &[(u32, Way)]) {
    let entities = rig.io.entities.len();
    rig.down(Request::Spawn { owner: owner(99), spawn: command(specs) }).nothing();
    assert_eq!(rig.io.entities.len(), entities, "refused before making anything");
    assert_eq!(
        rig.next().events,
        [Event::Failed { owner: owner(99), error: crate::Error::Busy }, Event::Closed { owner: owner(99) }]
    );
}

#[test]
fn given_refuses_missing_pipe_stream_claimed_and_closing_tokens() {
    let mut rig = Rig::new(Limits { sockets: 8, ..limits() });
    refused_spawn(&mut rig, &[(3, Way::Given(owner(400)))]);
    let pipe = rig.io.adopt_write_pipe(Fd::new(41), None).expect("pipe");
    rig.next().nothing();
    refused_spawn(&mut rig, &[(3, Way::Given(pipe))]);
    let (stream, _) = super::connected(&mut rig, owner(1), Fd::new(42));
    refused_spawn(&mut rig, &[(3, Way::Given(stream))]);
    let held = rig.io.adopt_listener(Fd::new(43), super::local(8000)).expect("held");
    rig.down(Request::Listen { owner: owner(2), addr: super::addr(8000) }).nothing();
    rig.next().take(Kind::Accept);
    refused_spawn(&mut rig, &[(3, Way::Given(held))]);
    let held = rig.io.adopt_listener(Fd::new(44), super::local(8001)).expect("held");
    rig.down(Request::Close { entity: held }).take(Kind::Close);
    refused_spawn(&mut rig, &[(3, Way::Given(held))]);
}

#[test]
fn duplicate_child_descriptors_are_refused_for_pipes_and_given_entries() {
    let mut rig = Rig::new(Limits { sockets: 4, ..limits() });
    let held = rig.io.adopt_listener(Fd::new(50), super::local(8000)).expect("held");
    for specs in [
        [(3, Way::In), (3, Way::Out)],
        [(3, Way::Given(held)), (3, Way::Out)],
        [(3, Way::Given(held)), (3, Way::Given(held))],
    ] {
        refused_spawn(&mut rig, &specs);
    }
}

#[test]
fn a_failed_spawn_releases_only_its_pipes_and_keeps_the_given_listener() {
    let mut rig = Rig::new(Limits { sockets: 3, ..limits() });
    let held = rig.io.adopt_listener(Fd::new(50), super::local(8000)).expect("held");
    let spawning = rig
        .down(Request::Spawn { owner: owner(1), spawn: command(&[(3, Way::Given(held)), (1, Way::Out)]) })
        .take(Kind::Spawn);
    assert_eq!(
        rig.complete(spawning, Err(crate::kernel::Error::NotFound)).events,
        [Event::Failed { owner: owner(1), error: crate::Error::Other }, Event::Closed { owner: owner(1) }]
    );
    rig.next().nothing();
    assert_eq!(rig.io.entities.len(), 1);
    let spawning =
        rig.down(Request::Spawn { owner: owner(2), spawn: command(&[(3, Way::Given(held))]) }).take(Kind::Spawn);
    let Op::Spawn { spawn } = &spawning.kind else { panic!("spawn") };
    assert_eq!(spawn.pipes[0].parent, Some(Fd::new(50)));
    rig.complete(spawning, Err(crate::kernel::Error::NotFound));
    let closing = rig.down(Request::Close { entity: held }).take(Kind::Close);
    rig.complete(closing, Ok(Done::Nothing));
    rig.empty();
}
