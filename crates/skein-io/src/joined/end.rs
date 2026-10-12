//! One joined end: demand delivery, bounded moves, half-close and drain.
use super::{End, EndState};
use crate::layer::{Entity, Io, Tables};
use crate::stream::{self, Demand, Reader};
use crate::{Event, Limits};
use alloc::boxed::Box;
use core::mem;
use skein_lib::stream::{Down, Fault, Read, Up};
use skein_lib::{Id, Intake, Queue, Token};

#[derive(Debug)]
pub(super) struct Open {
    flow: Flow,
    intake: Intake,
    demand: Demand,
    granted: u32,
}
/// The peer's moves are discarded until its end, while this output flushes.
#[derive(Debug)]
pub(super) struct Closing {
    flow: Flow,
    wake_drain: bool,
}
/// The ordered output and the peer's half-close, held by either active state.
#[derive(Debug)]
struct Flow {
    peer: Id<Entity>,
    reader: Reader,
    queue: Queue<Box<[u8]>>,
    moving: Option<(Box<[u8]>, u32)>,
    bytes: u32,
    finishing: bool,
    shut: bool,
}
impl Open {
    pub(super) fn new(peer: Id<Entity>, limits: &Limits) -> Self {
        Self {
            flow: Flow {
                peer,
                reader: Reader::Full,
                queue: Queue::with_capacity(limits.sends),
                moving: None,
                bytes: 0,
                finishing: false,
                shut: false,
            },
            intake: Intake::with_capacity(limits.intake),
            demand: Demand { read: Read::Nothing, room: 0 },
            granted: 0,
        }
    }
    fn fits(&self, room: u32, limits: &Limits) -> bool {
        self.flow.queue.room() > 0
            && match self.flow.bytes.checked_add(room) {
                Some(n) => n <= limits.output,
                None => false,
            }
    }
}

pub(crate) fn request(current: &mut End, id: Id<Entity>, down: Down, limits: &Limits, tables: &mut Tables) {
    match &mut current.state {
        EndState::Open(open) => match down {
            Down::Demand { read, room } => {
                let withdrawing = read == Read::Nothing && room == 0;
                assert!(withdrawing || (open.demand.read == Read::Nothing && open.demand.room == 0), "one live demand");
                assert!(
                    room <= limits.output && (room == 0 || !open.flow.finishing),
                    "room within output and before Finish"
                );
                match read {
                    Read::Nothing => {}
                    Read::Fill(n) => assert!(n <= limits.intake, "fill within intake"),
                    Read::Scan { until, max } => assert!(
                        max <= limits.intake && usize::try_from(max).expect("u32 index") >= until.as_bytes().len(),
                        "scan within intake"
                    ),
                    Read::Line { max } => assert!(max > 0 && max <= limits.intake, "line within intake"),
                }
                open.demand = Demand { read, room };
            }
            Down::Send(data) => {
                assert!(!open.flow.finishing, "no Send after Finish");
                let len = u32::try_from(data.len()).expect("bounded Send");
                open.granted = open.granted.checked_sub(len).expect("Send spends granted room");
                if data.is_empty() {
                    open.granted = 0;
                } else {
                    assert!(open.fits(len, limits), "Send within output caps");
                    open.flow.bytes = open.flow.bytes.checked_add(len).expect("bounded output");
                    if open.flow.moving.is_none() {
                        open.flow.moving = Some((data, 0));
                    } else {
                        open.flow.queue.push(data);
                    }
                }
            }
            Down::Finish => {
                open.flow.finishing = true;
                open.demand.room = 0;
            }
        },
        EndState::Closing(_) | EndState::Terminating { .. } | EndState::Broken { .. } | EndState::Closed => return,
        EndState::Starting { .. } | EndState::Waiting { .. } | EndState::Announced { .. } => {
            unreachable!("Stream requires a connected or bound end")
        }
    }
    tables.ready.mark(id);
}

pub(crate) fn close(current: &mut End, id: Id<Entity>, abort: bool, tables: &mut Tables) {
    current.state = match mem::replace(&mut current.state, EndState::Closed) {
        EndState::Open(mut open) if !abort => {
            open.flow.finishing = true;
            EndState::Closing(Closing { flow: open.flow, wake_drain: true })
        }
        EndState::Closing(closing) if !abort => EndState::Closing(closing),
        EndState::Open(open) => EndState::Terminating { waiting: None, error: None, peer: Some(open.flow.peer) },
        EndState::Closing(closing) => {
            EndState::Terminating { waiting: None, error: None, peer: Some(closing.flow.peer) }
        }
        EndState::Starting { .. } | EndState::Broken { .. } => {
            EndState::Terminating { waiting: None, error: None, peer: None }
        }
        EndState::Waiting { listener, order } => {
            EndState::Terminating { waiting: Some((listener, order)), error: None, peer: None }
        }
        EndState::Announced { .. } => unreachable!("an announced end must be answered"),
        state @ (EndState::Terminating { .. } | EndState::Closed) => state,
    };
    tables.ready.mark(id);
}

pub(super) fn reset(io: &mut Io, id: Id<Entity>, up: &mut Queue<Event>) {
    let Some(Entity::Joined(current)) = io.entities.get_mut(id) else {
        return;
    };
    if current.is_closed() {
        return;
    }
    current.state = match mem::replace(&mut current.state, EndState::Closed) {
        EndState::Open(_) => {
            up.push(Event::Stream { owner: current.owner.expect("bound peer owner"), up: Up::Failed(Fault::Reset) });
            EndState::Broken { told: true }
        }
        EndState::Closing(_) => EndState::Terminating { waiting: None, error: None, peer: None },
        EndState::Announced { listener, peer, .. } => EndState::Announced { listener, peer, reset: true },
        state @ (EndState::Starting { .. }
        | EndState::Waiting { .. }
        | EndState::Terminating { .. }
        | EndState::Broken { .. }
        | EndState::Closed) => state,
    };
    io.tables.ready.mark(id);
}

pub(super) fn resume_open(
    io: &mut Io,
    id: Id<Entity>,
    owner: Token,
    mut open: Open,
    limits: &Limits,
    up: &mut Queue<Event>,
) -> EndState {
    let room = open.fits(open.demand.room, limits) && !open.flow.finishing;
    let before = open.intake.room();
    let _room = stream::delivered(
        owner,
        &mut open.intake,
        &mut open.demand,
        &mut open.flow.reader,
        room,
        &mut open.granted,
        up,
    );
    if open.intake.room() > before {
        wake(io, open.flow.peer);
    }
    if advance(io, id, &mut open.flow) {
        EndState::Open(open)
    } else {
        up.push(Event::Stream { owner, up: Up::Failed(Fault::Reset) });
        EndState::Broken { told: true }
    }
}
pub(super) fn resume_closing(
    io: &mut Io,
    id: Id<Entity>,
    owner: Token,
    mut closing: Closing,
    up: &mut Queue<Event>,
) -> EndState {
    if closing.wake_drain {
        closing.wake_drain = false;
        wake(io, closing.flow.peer);
    }
    let live = advance(io, id, &mut closing.flow);
    let drained = match closing.flow.reader {
        Reader::Ended | Reader::Told => true,
        Reader::Full => false,
        Reader::Receiving(_) | Reader::Stalled => unreachable!("joined readers have no kernel operations"),
    };
    if !live || (closing.flow.shut && drained) {
        up.push(Event::Closed { owner });
        EndState::Closed
    } else {
        EndState::Closing(closing)
    }
}
/// One move, then the half-close once no output remains. False means reset.
fn advance(io: &mut Io, id: Id<Entity>, flow: &mut Flow) -> bool {
    if flow.moving.is_none()
        && let Some(data) = flow.queue.pop()
    {
        flow.moving = Some((data, 0));
    }
    if let Some((data, from)) = flow.moving.take() {
        let left = data.get(usize::try_from(from).expect("u32 index")..).expect("move offset within box");
        let moved = match io.entities.get_mut(flow.peer) {
            Some(Entity::Joined(peer)) => match &mut peer.state {
                EndState::Open(peer) => {
                    let n = peer.intake.room().min(u32::try_from(left.len()).expect("bounded Send"));
                    peer.intake
                        .append(left.get(..usize::try_from(n).expect("u32 index")).expect("move within box"))
                        .expect("move within intake room");
                    n
                }
                EndState::Closing(_) => u32::try_from(left.len()).expect("bounded Send"),
                EndState::Announced { .. } => 0,
                EndState::Broken { .. } | EndState::Terminating { .. } | EndState::Closed => {
                    return false;
                }
                EndState::Starting { .. } | EndState::Waiting { .. } => unreachable!("a joined peer is accepted"),
            },
            None => {
                return false;
            }
            Some(
                Entity::Named(_)
                | Entity::Listener(_)
                | Entity::Stream(_)
                | Entity::Pipe(_)
                | Entity::Child(_)
                | Entity::Signals(_),
            ) => unreachable!("a joined peer is a joined end"),
        };
        let offset = from.checked_add(moved).expect("bounded move");
        flow.bytes = flow.bytes.checked_sub(moved).expect("move spends output");
        if usize::try_from(offset).expect("u32 index") < data.len() {
            flow.moving = Some((data, offset));
        }
        if moved > 0 {
            wake(io, flow.peer);
            io.tables.ready.mark(id);
        }
    }
    if flow.moving.is_none() && flow.queue.is_empty() && flow.finishing && !flow.shut {
        let Some(Entity::Joined(peer)) = io.entities.get_mut(flow.peer) else {
            return false;
        };
        match &mut peer.state {
            EndState::Open(peer) => peer.flow.reader = ended(peer.flow.reader),
            EndState::Closing(peer) => peer.flow.reader = ended(peer.flow.reader),
            EndState::Announced { .. } => return true,
            EndState::Broken { .. } | EndState::Terminating { .. } | EndState::Closed => {
                return false;
            }
            EndState::Starting { .. } | EndState::Waiting { .. } => unreachable!("joined peer is accepted"),
        }
        flow.shut = true;
        wake(io, flow.peer);
    }
    true
}
fn ended(reader: Reader) -> Reader {
    match reader {
        Reader::Told => Reader::Told,
        Reader::Full | Reader::Ended => Reader::Ended,
        Reader::Receiving(_) | Reader::Stalled => unreachable!("joined readers have no kernel operations"),
    }
}

pub(super) fn wake(io: &mut Io, id: Id<Entity>) {
    let active = match io.entities.get(id) {
        Some(Entity::Joined(peer)) => !peer.is_closed(),
        None => false,
        Some(
            Entity::Named(_)
            | Entity::Listener(_)
            | Entity::Stream(_)
            | Entity::Pipe(_)
            | Entity::Child(_)
            | Entity::Signals(_),
        ) => unreachable!("a joined peer is a joined end"),
    };
    if active {
        io.tables.ready.mark(id);
    }
}
