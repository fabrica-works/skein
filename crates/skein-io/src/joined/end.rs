//! One joined end: demand delivery, bounded moves, half-close and drain.
use super::{End, EndState};
use crate::layer::{Entity, Io, Tables, Timer};
use crate::output::Reservation;
use crate::stream::{self, Demand, Reader};
use crate::{Event, Limits};
use alloc::boxed::Box;
use core::mem;
use skein_lib::stream::{Down, Fault, OutputDown, OutputOutcome, Read, Up};
use skein_lib::{Env, Id, Intake, Queue, Token};

#[derive(Debug)]
pub(super) struct Open {
    flow: Flow,
    intake: Intake,
    demand: Demand,
    granted: u32,
    classic_grant: bool,
    independent: Reservation,
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
            classic_grant: false,
            independent: Reservation::Idle,
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
                assert!(room == 0 || open.independent.idle(), "classic room cannot overlap independent output");
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
            Down::Send(data) => send(open, data, limits),
            Down::Finish => {
                open.flow.finishing = true;
                open.demand.room = 0;
                open.classic_grant = false;
                open.independent.retire(OutputOutcome::Cancelled);
            }
        },
        EndState::Closing(_) | EndState::Terminating { .. } | EndState::Broken { .. } | EndState::Closed => return,
        EndState::Starting { .. }
        | EndState::Waiting { .. }
        | EndState::Announced { .. }
        | EndState::Established { .. } => {
            unreachable!("Stream requires a connected or bound end")
        }
    }
    tables.ready.mark(id);
}

pub(crate) fn close(current: &mut End, id: Id<Entity>, abort: bool, env: &Env<Limits>, tables: &mut Tables) {
    if current.is_closed() {
        return;
    }
    let was_closing = match current.state {
        EndState::Closing(_) => true,
        EndState::Starting { .. }
        | EndState::Waiting { .. }
        | EndState::Announced { .. }
        | EndState::Established { .. }
        | EndState::Open(_)
        | EndState::Broken { .. }
        | EndState::Terminating { .. }
        | EndState::Closed => false,
    };
    match &mut current.state {
        EndState::Open(open) => {
            open.independent.retire(OutputOutcome::Cancelled);
            current.owed = open.independent.take_terminal();
        }
        EndState::Starting { .. }
        | EndState::Waiting { .. }
        | EndState::Announced { .. }
        | EndState::Established { .. }
        | EndState::Closing(_)
        | EndState::Broken { .. }
        | EndState::Terminating { .. }
        | EndState::Closed => {}
    }
    current.state = match mem::replace(&mut current.state, EndState::Closed) {
        EndState::Open(mut open) if !abort => {
            open.flow.finishing = true;
            EndState::Closing(Closing { flow: open.flow, wake_drain: true })
        }
        EndState::Closing(closing) if !abort => EndState::Closing(closing),
        EndState::Established { open, .. } | EndState::Open(open) => {
            EndState::Terminating { waiting: None, error: None, peer: Some(open.flow.peer) }
        }
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
    match current.state {
        EndState::Closing(_) => {
            if !was_closing {
                tables
                    .deadlines
                    .arm((id, Timer::Close), env.now.saturating_add(env.limits.close_timeout))
                    .expect("one close deadline per end");
            }
        }
        EndState::Starting { .. }
        | EndState::Waiting { .. }
        | EndState::Announced { .. }
        | EndState::Established { .. }
        | EndState::Open(_)
        | EndState::Broken { .. }
        | EndState::Terminating { .. }
        | EndState::Closed => {
            tables.deadlines.cancel((id, Timer::Close));
        }
    }
    tables.ready.mark(id);
}

pub(super) fn reset(io: &mut Io, id: Id<Entity>) {
    let Some(Entity::Joined(current)) = io.entities.get_mut(id) else {
        return;
    };
    if current.is_closed() {
        return;
    }
    current.state = match mem::replace(&mut current.state, EndState::Closed) {
        EndState::Open(mut open) => {
            open.independent.retire(OutputOutcome::Failed(Fault::Reset));
            current.owed = open.independent.take_terminal();
            EndState::Broken { told: false }
        }
        EndState::Established { mut open, .. } => {
            open.intake.clear();
            EndState::Established { open, reset: true }
        }
        EndState::Closing(_) => {
            io.tables.deadlines.cancel((id, Timer::Close));
            EndState::Terminating { waiting: None, error: None, peer: None }
        }
        EndState::Announced { listener, peer, .. } => EndState::Announced { listener, peer, reset: true },
        state @ (EndState::Starting { .. }
        | EndState::Waiting { .. }
        | EndState::Terminating { .. }
        | EndState::Broken { .. }
        | EndState::Closed) => state,
    };
    io.tables.ready.mark(id);
}

pub(super) fn resume_established(
    io: &mut Io,
    id: Id<Entity>,
    owner: Token,
    open: Open,
    reset: bool,
    limits: &Limits,
    up: &mut Queue<Event>,
) -> EndState {
    up.push(Event::Connected { owner });
    if reset {
        up.push(Event::Stream { owner, up: Up::Failed(Fault::Reset) });
        EndState::Broken { told: true }
    } else {
        resume_open(io, id, owner, open, limits, up)
    }
}

pub(super) fn resume_open(
    io: &mut Io,
    id: Id<Entity>,
    owner: Token,
    mut open: Open,
    limits: &Limits,
    up: &mut Queue<Event>,
) -> EndState {
    if !advance(io, id, &mut open.flow) {
        open.independent.retire(OutputOutcome::Failed(Fault::Reset));
        if let Some(terminal) = open.independent.take_terminal() {
            up.push(Event::Output { owner, up: terminal });
        }
        up.push(Event::Stream { owner, up: Up::Failed(Fault::Reset) });
        return EndState::Broken { told: true };
    }
    let room = open.fits(open.demand.room, limits) && !open.flow.finishing;
    let before = open.intake.room();
    let room_granted = stream::delivered(
        owner,
        &mut open.intake,
        &mut open.demand,
        &mut open.flow.reader,
        room,
        &mut open.granted,
        up,
    );
    if room_granted {
        open.classic_grant = true;
    }
    if let Some(terminal) = open.independent.take_terminal() {
        up.push(Event::Output { owner, up: terminal });
    } else if let Some((_, bytes)) = open.independent.wanted()
        && !open.flow.finishing
        && open.fits(bytes, limits)
    {
        up.push(Event::Output { owner, up: open.independent.grant() });
    }
    if open.intake.room() > before {
        wake(io, open.flow.peer);
    }
    EndState::Open(open)
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
                EndState::Established { reset: true, .. } => return false,
                EndState::Open(peer) | EndState::Established { open: peer, reset: false } => {
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
            flow.shut = true;
            return true;
        };
        match &mut peer.state {
            EndState::Open(peer) | EndState::Established { open: peer, reset: false } => {
                peer.flow.reader = ended(peer.flow.reader);
            }
            EndState::Closing(peer) => peer.flow.reader = ended(peer.flow.reader),
            EndState::Announced { .. } => return true,
            EndState::Established { reset: true, .. }
            | EndState::Broken { .. }
            | EndState::Terminating { .. }
            | EndState::Closed => {}
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

fn send(open: &mut Open, data: Box<[u8]>, limits: &Limits) {
    assert!(!open.flow.finishing, "no Send after Finish");
    assert!(open.independent.idle(), "classic Send cannot spend an independent grant");
    open.classic_grant = false;
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

pub(crate) fn output_request(
    current: &mut End,
    id: Id<Entity>,
    down: OutputDown,
    limits: &Limits,
    tables: &mut Tables,
) {
    let open = match &mut current.state {
        EndState::Open(open) => open,
        EndState::Starting { .. }
        | EndState::Waiting { .. }
        | EndState::Announced { .. }
        | EndState::Established { .. }
        | EndState::Closing(_)
        | EndState::Broken { .. }
        | EndState::Terminating { .. }
        | EndState::Closed => return,
    };
    match down {
        OutputDown::Room { right, bytes } => {
            if bytes == 0
                || bytes > limits.output
                || open.flow.finishing
                || !open.independent.idle()
                || open.demand.room > 0
                || open.classic_grant
            {
                return;
            }
            open.granted = 0;
            open.independent.admit(right, bytes);
            tables.ready.mark(id);
        }
        OutputDown::Cancel { right } => {
            if open.independent.cancel(right) {
                tables.ready.mark(id);
            }
        }
        OutputDown::Send { right, bytes } => {
            let Some(grant) = open.independent.granted(right) else {
                return;
            };
            assert!(
                u32::try_from(bytes.len()).expect("bounded independent Send") <= grant,
                "matching Send fits its grant"
            );
            open.independent.release(right);
            open.granted = grant;
            send(open, bytes, limits);
            open.granted = 0;
            tables.ready.mark(id);
        }
        OutputDown::Release { right } => open.independent.release(right),
    }
}
pub(crate) fn expired(io: &mut Io, id: Id<Entity>, up: &mut Queue<Event>) {
    let current = super::end(io, id);
    let owner = current.owner.expect("bound closing end");
    if let Some(terminal) = current.owed.take() {
        up.push(Event::Output { owner, up: terminal });
    }
    let closing = match mem::replace(&mut current.state, EndState::Closed) {
        EndState::Closing(closing) => closing,
        EndState::Starting { .. }
        | EndState::Waiting { .. }
        | EndState::Announced { .. }
        | EndState::Established { .. }
        | EndState::Open(_)
        | EndState::Broken { .. }
        | EndState::Terminating { .. }
        | EndState::Closed => unreachable!("only a closing end has a deadline"),
    };
    let flushing = match io.entities.get(closing.flow.peer) {
        Some(Entity::Joined(peer)) => match &peer.state {
            EndState::Open(open) | EndState::Established { open, .. } => {
                open.flow.moving.is_some() || !open.flow.queue.is_empty()
            }
            EndState::Closing(closing) => !closing.flow.shut,
            EndState::Starting { .. }
            | EndState::Waiting { .. }
            | EndState::Announced { .. }
            | EndState::Broken { .. }
            | EndState::Terminating { .. }
            | EndState::Closed => false,
        },
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
    if !closing.flow.shut || flushing {
        reset(io, closing.flow.peer);
    }
    up.push(Event::Closed { owner });
}
