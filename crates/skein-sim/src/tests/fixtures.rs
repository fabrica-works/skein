use alloc::boxed::Box;
use skein_io::kernel::{Done, Fd, Op, Pipe, Spawn, Submit, Way};
use skein_lib::{Queue, Token};

use crate::{Answer, Config, Handle, Pid, Program, Reply, Sim};

pub const INPUT: &[u8; 3] = b"abc";
pub const INPUT_BYTES: usize = INPUT.len();
// One operation, machine call or answer is outstanding at each turn.
pub const ONE_RECORD: u32 = 1;
pub const SPAWN: Token = Token::new(1);
pub const WRITE: Token = Token::new(2);
pub const CLOSE: Token = Token::new(3);
pub const READ: Token = Token::new(4);
pub const END: Token = Token::new(5);
const SEED: u64 = 7;
const ROOT: Handle = Handle::new(1);

pub struct HostedInput {
    pub sender: skein_io::kernel::Sender,
    pub sim: Sim,
    pub parent: Pid,
    pub child: Pid,
    pub writer: Fd,
    pub reader: Fd,
}

pub fn hosted_input() -> HostedInput {
    let mut sim = Sim::new(SEED, Config::calm());
    let parent = sim.spawn_process();
    let root = sim.root(parent, ROOT);
    submit(
        &mut sim,
        parent,
        SPAWN,
        Op::Spawn {
            spawn: Box::new(Spawn {
                program: Box::from(&b"service"[..]),
                args: Box::new([]),
                env: Box::new([]),
                root,
                dir: Box::new([]),
                pipes: Box::new([Pipe { child: 0, way: Way::In, parent: None }]),
            }),
        },
    );
    let mut calls = Queue::with_capacity(ONE_RECORD);
    sim.calls(&mut calls);
    let call = calls.pop().expect("spawn call");
    let mut answers = Queue::with_capacity(ONE_RECORD);
    answers.push(Answer { ticket: call.ticket, result: Ok(Reply::Program(Program::Service)) });
    sim.answer(&mut answers);
    let mut completes = Queue::with_capacity(ONE_RECORD);
    sim.reap(parent, &mut completes);
    let complete = completes.pop().expect("spawned service");
    let Op::Spawn { spawn } = complete.kind else { unreachable!("spawn record") };
    let writer = spawn.pipes.iter().find(|pipe| pipe.child == 0).expect("input pipe").parent.expect("writer");
    let Ok(Done::Spawned { pidfd, sender }) = complete.result else { unreachable!("service spawned") };
    let (child, inherited) = sim.bind_service(parent, pidfd);
    let reader = inherited.into_iter().find(|(number, _)| *number == 0).expect("child input").1;
    HostedInput { sender, sim, parent, child, writer, reader }
}

pub fn submit(sim: &mut Sim, pid: Pid, op: Token, kind: Op) {
    let mut submits = Queue::with_capacity(ONE_RECORD);
    submits.push(Submit { op, kind });
    sim.submit(pid, &mut submits);
}
