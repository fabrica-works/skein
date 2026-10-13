//! Closing the connection and handling failure (tls.md, 3.3).
use super::below::ended;
use super::{Buffers, Demand, Error, Event, Handshake, Open, Reading, Room, State, Writing};
use crate::held::Held;
use crate::session;
use skein_lib::Queue;
use skein_lib::stream::{Down, Read, Up};
/// A close sending what TLS owes.
#[derive(Debug)]
pub(super) struct Closing {
    /// The demand outstanding below, if any: one for room is kept, as an
    /// answer of room may be on its way; one that only reads is withdrawn.
    pub(super) demand: Option<Demand>,
}

/// The connection failed: the plaintext stream is told, unless the side
/// above withdrew its demand, then `Failed`; the client waits for its close.
/// rustls's connection is dropped, and nothing more is sent.
pub(super) fn fail(open: Open, error: Error, above: &mut Queue<Event>) -> State {
    match open.above.reading {
        Reading::Withdrawn => {}
        Reading::Open | Reading::Ended => above.push(Event::Stream(Up::Failed(error.fault()))),
    }
    above.push(Event::Failed(error));
    State::Failed(open.below.demand)
}

/// `Close`, in any state.
pub(super) fn close(state: State, buffers: &mut Buffers, above: &mut Queue<Event>, below: &mut Queue<Down>) -> State {
    match state {
        State::Fresh(_) => closed(None, above, below),
        State::Failed(demand) => closed(demand, above, below),
        State::Open(open) => close_open(open, buffers, above, below),
        State::Closing(_) | State::Closed => unreachable!("a Close after Close"),
    }
}

/// `Close` of an open connection: `close_notify`, unless the handshake is not
/// done or it was written already, then what TLS owes goes below.
pub(super) fn close_open(
    mut open: Open,
    buffers: &mut Buffers,
    above: &mut Queue<Event>,
    below: &mut Queue<Down>,
) -> State {
    if open.handshake == Handshake::Running {
        return closed(open.below.demand, above, below);
    }
    match open.writing {
        Writing::Open | Writing::Finishing => {
            let written = session::close_notify(&mut open.tls, &mut buffers.records, &mut buffers.owed);
            if written.is_err() {
                return closed(open.below.demand, above, below);
            }
        }
        Writing::Flushing => {}
        Writing::Finished => return closed(open.below.demand, above, below),
    }
    // A demand that only reads is withdrawn; one for room is kept, as its
    // answer may be on its way, and what TLS owes goes within it.
    let demand = match open.below.demand {
        Some(Demand { room: Room::None, .. }) => {
            below.push(Down::Demand { read: Read::Nothing, room: 0 });
            None
        }
        Some(Demand { room: room @ (Room::Owed(_) | Room::Above { .. }), read }) => Some(Demand { read, room }),
        None => None,
    };
    // Room the side above held and did not send within is TLS's now.
    if open.below.granted > 0 {
        below.push(Down::Send(buffers.owed.take(open.below.granted)));
    }
    closing_settle(Closing { demand }, &mut buffers.owed, above, below)
}

/// Closed at once: the demand outstanding below withdrawn, if any.
pub(super) fn closed(demand: Option<Demand>, above: &mut Queue<Event>, below: &mut Queue<Down>) -> State {
    if demand.is_some() {
        below.push(Down::Demand { read: Read::Nothing, room: 0 });
    }
    above.push(Event::Closed);
    State::Closed
}

/// An event below while closing.
pub(super) fn closing_up(
    mut closing: Closing,
    owed: &mut Held,
    ev: Up,
    above: &mut Queue<Event>,
    below: &mut Queue<Down>,
) -> State {
    match ev {
        // An answer to the demand outstanding, if it reads; otherwise one on
        // its way to the demand withdrawn. Either is dropped.
        Up::Bytes(_) => {
            closing.demand = match closing.demand {
                Some(Demand { read: Read::Nothing, room }) => Some(Demand { read: Read::Nothing, room }),
                Some(Demand {
                    read: Read::Fill(_) | Read::Scan { .. } | Read::Line { .. } | Read::Arrived { .. },
                    ..
                })
                | None => None,
            };
        }
        Up::Room => {
            let demand = closing.demand.take().expect("room answers the demand outstanding");
            let room = demand.room.bytes();
            assert!(room > 0, "room answers a demand for room");
            below.push(Down::Send(owed.take(room)));
        }
        Up::End => closing.demand = ended(closing.demand, below),
        // Nothing more can be sent.
        Up::Failed(_) => {
            above.push(Event::Closed);
            return State::Closed;
        }
    }
    closing_settle(closing, owed, above, below)
}

/// Where a close goes: `Closed` once nothing is owed; otherwise room
/// demanded for it, if no demand is outstanding.
pub(super) fn closing_settle(
    mut closing: Closing,
    owed: &mut Held,
    above: &mut Queue<Event>,
    below: &mut Queue<Down>,
) -> State {
    if owed.is_empty() {
        return closed(closing.demand, above, below);
    }
    if closing.demand.is_none() {
        let demand = Demand { read: Read::Nothing, room: Room::Owed(owed.len()) };
        below.push(Down::Demand { read: demand.read, room: demand.room.bytes() });
        closing.demand = Some(demand);
    }
    State::Closing(closing)
}
