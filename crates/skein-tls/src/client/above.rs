//! The side above and its plaintext stream (tls.md, 3.1).
use super::close::fail;
use super::{Buffers, Event, Limits, Open, Peer, State, Writing, room_for, settle};
use crate::session;
use core::mem;
use skein_lib::stream::{Down, Read, Up};
use skein_lib::{Intake, Queue};
/// The side above's side of the plaintext stream.
#[derive(Debug)]
pub(super) struct Above {
    /// Its demand outstanding, if any.
    pub(super) demand: Option<Wanted>,
    /// Room told, which it may send within, once.
    pub(super) grant: Option<u32>,
    pub(super) reading: Reading,
}

/// A demand of the side above's.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) struct Wanted {
    pub(super) read: Read,
    pub(super) room: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum Reading {
    Open,
    /// It withdrew its demand: it reads no more, and is closing.
    Withdrawn,
    /// It was told `End`.
    Ended,
}

/// The plaintext stream, from the side above.
pub(super) fn stream(
    mut open: Open,
    down: Down,
    buffers: &mut Buffers,
    limits: &Limits,
    above: &mut Queue<Event>,
    below: &mut Queue<Down>,
) -> State {
    match down {
        // A withdrawal: the side above reads no more (lib.md, 7), and closes
        // next. What the client reads below for it may still come, and is
        // dropped with the intake. After the end, it gives up the read that
        // crossed it, and may write on.
        Down::Demand { read: Read::Nothing, room: 0 } => {
            open.above.demand = None;
            if open.above.reading == Reading::Open {
                open.above.reading = Reading::Withdrawn;
            }
        }
        Down::Demand { read, room } => {
            assert!(
                open.above.demand.is_none(),
                "one demand at a time, stated after the last was answered (lib.md, 7)"
            );
            let wanted = match read {
                Read::Nothing => 0,
                Read::Fill(n) => n,
                Read::Scan { max, .. } | Read::Line { max } => max,
            };
            assert!(wanted <= limits.read, "no read past Limits::read");
            assert!(
                read == Read::Nothing || open.above.reading != Reading::Withdrawn,
                "no read after a withdrawal: the side above reads no more"
            );
            assert!(room <= limits.send, "no room past Limits::send");
            assert!(room == 0 || open.above.grant.is_none(), "room is demanded once the last grant was sent within");
            assert!(room == 0 || open.writing == Writing::Open, "no room after Finish");
            open.above.demand = Some(Wanted { read, room });
            // Room granted below for a demand the side above then withdrew
            // is held: handed on now if it takes this demand's records, or
            // given up, so that room is asked for again, as a grant not sent
            // within gives way to the next (io.md, 3.3).
            if room > 0 && open.below.granted > 0 {
                if room_for(room).expect("checked by worst_case") <= open.below.granted {
                    open.above.demand = None;
                    open.above.grant = Some(room);
                    above.push(Event::Stream(Up::Room));
                } else {
                    open.below.granted = 0;
                }
            }
        }
        Down::Send(bytes) => return sent(open, &bytes, buffers, above, below),
        Down::Finish => {
            assert!(open.writing == Writing::Open, "one Finish");
            let wanted = match open.above.demand {
                Some(wanted) => wanted,
                None => Wanted { read: Read::Nothing, room: 0 },
            };
            assert!(wanted.room == 0, "Finish with no room demanded");
            // A read the client is still reading below for would hold
            // close_notify behind it, while the server may wait for the end
            // of what it is sent before it answers.
            assert!(
                wanted.read == Read::Nothing || open.above.reading == Reading::Ended,
                "Finish with no read outstanding, but one that crossed the end"
            );
            // Nothing more to send: a grant not sent within is TLS's now.
            open.above.grant = None;
            open.writing = Writing::Finishing;
        }
    }
    settle(open, buffers, above, below)
}

/// A `Send` from the side above: encrypted, after what TLS owes, within the
/// room granted below for it.
pub(super) fn sent(
    mut open: Open,
    bytes: &[u8],
    buffers: &mut Buffers,
    above: &mut Queue<Event>,
    below: &mut Queue<Down>,
) -> State {
    let grant = open.above.grant.take().expect("a Send within the room granted (lib.md, 7)");
    assert!(bytes.len() <= usize::try_from(grant).expect("a u32 fits in a usize"), "a Send within the room granted");
    let room = mem::replace(&mut open.below.granted, 0);
    match session::encrypt(&mut open.tls, &mut buffers.records, bytes, &mut buffers.owed) {
        Ok(Some(sealed)) => {
            assert!(
                sealed.len() <= usize::try_from(room).expect("a u32 fits in a usize"),
                "the records within the room granted below: OVERHEAD and SLACK"
            );
            below.push(Down::Send(sealed));
        }
        Ok(None) => {}
        Err(error) => return fail(open, error, above),
    }
    settle(open, buffers, above, below)
}

/// The side above's demand met from the intake, or the end of what the
/// server sent told: `End` once its `close_notify` was read, and nothing held
/// meets a demand; with none outstanding, once nothing is held (lib.md, 7).
/// `false` where a truncation is due instead.
pub(super) fn answer(open: &mut Open, intake: &mut Intake, above: &mut Queue<Event>) -> bool {
    if open.above.reading != Reading::Open {
        return true;
    }
    let reads = match open.above.demand {
        Some(wanted) => wanted.read,
        None => Read::Nothing,
    };
    if reads != Read::Nothing {
        if let Some(bytes) = intake.meet(reads) {
            open.above.demand = None;
            above.push(Event::Stream(Up::Bytes(bytes)));
            return true;
        }
    } else if !intake.is_empty() {
        return true;
    }
    match open.peer {
        Peer::Open => true,
        Peer::Cut => false,
        Peer::Closed => {
            // The end answers nothing: a read outstanding stays so, never
            // met, until the side above withdraws it, as io keeps it
            // (lib.md, 7); room may still be granted.
            open.above.reading = Reading::Ended;
            above.push(Event::Stream(Up::End));
            true
        }
    }
}
