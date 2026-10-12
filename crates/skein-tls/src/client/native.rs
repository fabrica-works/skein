//! The TLS client with independent output on both sides: for a side above
//! that must write while its read waits (tls.md, 3.6; lib.md, 7.1).
//!
//! It shares the classic client's session and buffers. Its four cells each
//! hold at most one obligation: the read above, the output right above, the
//! read below and the output right below. Tokens below are checked, never
//! reused and never wrapped. A grant above keeps its bytes and one Send slot
//! below until it is spent; what TLS owes goes before the data within it.
//!
//! The owner closes the stream below and reports `LowerEvent::Closed` after
//! every right below settles. Unsolicited cancellation below reports
//! `TransportClosing`; without a close pending, closure reports
//! `TransportClosed`. With one pending, `Closed` answers it. A close after
//! `TransportClosed` does nothing. The client keeps no timer or descriptor.
//!
//! `Finish` comes after `Ready`, with no read above but one that crossed
//! `End`, and no output right pending. A withdrawn read may already be
//! answered below: its bytes are dropped before deciphering or sealing.
//! Each caller reserves the entry point's maximum output and drains it.

use alloc::boxed::Box;
use core::mem;

use skein_lib::stream::{Down, OutputDown, OutputOutcome, OutputUp, Read, Up};
use skein_lib::{Env, Intake, Queue, Token};

use super::{
    Buffers, Error, FLIGHT, Fresh, HEADER, Handshake, LARGEST_READ, Limits, Line, MAX_PLAINTEXT, Peer, Reading, Record,
    Waiting, Writing,
};
use crate::held::Held;
use crate::session::{self, Connection};
use crate::{Config, MaxOut, Name};

/// The read, handshake and close above, or its output right.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Request {
    /// Handshake once, a read-only Stream, or Close. A demand of room or a
    /// Send on the classic stream is a bug, asserted before effects.
    Client(super::Request),

    /// An independent plaintext output right, within `Limits::send`. A
    /// terminal answers Room; Send or Release spends a grant.
    Output(OutputDown),
}

/// Events to the side above.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Event {
    /// Ready, plaintext Bytes, End or Failed, a connection failure, or Closed.
    /// A handshake may also end with `TransportClosed`.
    Client(super::Event),

    /// One terminal for an admitted output right. A Granted that went up
    /// stays its terminal through cancellation or closing.
    Output(OutputUp),

    /// An unsolicited cancellation below makes the stream below unusable.
    /// The owner closes it and requests Close.
    TransportClosing,

    /// The stream below closed, with no Close pending. It first cancels an
    /// output right pending above, then ends the connection. No Closed follows.
    TransportClosed,
}

/// Requests to the side below.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum LowerRequest {
    /// A read-only Fill, a withdrawal, or Finish. The owner closes the
    /// stream below.
    Stream(Down),

    /// An output right below, independent of the read. The client makes its
    /// tokens without reuse.
    Output(OutputDown),
}

/// Events from the side below.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum LowerEvent {
    /// Bytes that answer a Fill, End, or Failed. Room is a bug: this stream
    /// only reads.
    Stream(Up),

    /// The terminal of a right below, also after a failure or Close, before
    /// Closed.
    Output(OutputUp),

    /// The owner closed the stream below, after every right below settled.
    /// A pending right above is cancelled first. The owner checks that the
    /// closure is for this client's stream.
    Closed,
}

/// Above, Ready, an output failure, a stream failure and a client failure.
/// Below, a withdrawal or Send, then the next output right.
pub const UP_MAX_OUT: MaxOut = MaxOut { above: 4, below: 3 };

/// Above, a pending output terminal, a stream failure and a client failure.
/// Below, a withdrawal, `close_notify` sent and Finish.
pub const DOWN_MAX_OUT: MaxOut = MaxOut { above: 3, below: 3 };

/// The room below for the plaintext's records and everything TLS may owe
/// before them, within FLIGHT.
#[must_use]
pub fn room_for(plaintext: u32) -> Option<u32> {
    super::room_for(plaintext)?.checked_add(FLIGHT)
}

/// The configured output cap needed below, for `Limits::send`.
#[must_use]
pub fn largest_room(limits: &Limits) -> Option<u32> {
    Some(room_for(limits.send)?.max(FLIGHT))
}

/// The classic worst case, with its delivery replaced by the larger of
/// `LARGEST_READ` and the room below, plus this client's size. The bytes read
/// are dropped before deciphering or sealing; allocations that overlap
/// are counted as their sum. The caller counts its plaintext and queues.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<u64> {
    let work = u64::from(largest_room(limits)?.max(LARGEST_READ));
    super::worst_case(limits)?
        .checked_sub(u64::from(LARGEST_READ))?
        .checked_add(work)?
        .checked_add(u64::try_from(size_of::<Client>()).ok()?)
}

#[derive(Debug)]
enum State {
    Fresh(Fresh),
    Open(Open),
    Failed,
    Closing,
    TransportClosing,
    Closed,
    TransportClosed,
}

#[derive(Debug)]
struct Open {
    tls: Connection,
    handshake: Handshake,
    peer: Peer,
    writing: Writing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpperOutput {
    Idle,
    Wanted { right: Token, bytes: u32 },
    Granted { right: Token, bytes: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    Owed,
    Upper { right: Token, bytes: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cancellation {
    None,
    Requested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LowerOutput {
    Idle,
    Wanted { right: Token, bytes: u32, purpose: Purpose, cancellation: Cancellation },
    Granted { right: Token, bytes: u32 },
}

/// The configured caps of the stream below, checked before construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LowerLimits {
    /// The largest Fill accepted below, at least `LARGEST_READ`.
    pub read: u32,

    /// The output byte cap, at least `largest_room(limits)`. It counts
    /// boxes queued and in flight.
    pub output: u32,

    /// Queued Send slots, at least one beside the one in flight or stalled.
    pub sends: u32,
}

/// A client with four independent cells and the classic client's engine.
/// Its size is counted by `worst_case`.
#[derive(Debug)]
pub struct Client {
    state: State,
    buffers: Buffers,
    upper_read: Read,
    reading: Reading,
    lower_read: Option<Read>,
    // A withdrawn Fill may already be answered; withdrawal needs no answer.
    withdrawn_read: Option<Read>,
    record: Record,
    lower_ended: bool,
    upper_output: UpperOutput,
    lower_output: LowerOutput,
    next_lower: Option<u64>,
}

impl Client {
    /// Refuse caps below that cannot honour the read, output bytes or queued
    /// Send slots, before allocating or cloning. Credit short at a given
    /// moment leaves an admitted right waiting.
    #[must_use]
    pub fn new(config: &Config, name: Name, limits: &Limits, lower: &LowerLimits) -> Option<Client> {
        worst_case(limits)?;
        if lower.read < LARGEST_READ || lower.output < largest_room(limits)? || lower.sends == 0 {
            return None;
        }
        Some(Client {
            state: State::Fresh(Fresh { config: config.clone(), name, line: Line::Open }),
            buffers: Buffers {
                intake: Intake::with_capacity(super::intake_cap(limits)?),
                records: Held::with_capacity(limits.records),
                owed: Held::with_capacity(FLIGHT),
            },
            upper_read: Read::Nothing,
            reading: Reading::Open,
            lower_read: None,
            withdrawn_read: None,
            record: Record::Header,
            lower_ended: false,
            upper_output: UpperOutput::Idle,
            lower_output: LowerOutput::Idle,
            next_lower: Some(0),
        })
    }

    /// What it waits for: a function of its state. The owner arms timers.
    #[must_use]
    pub fn waiting(&self) -> Waiting {
        match &self.state {
            State::Fresh(_) => Waiting::Handshake,
            State::Open(open) => match open.handshake {
                Handshake::Running => Waiting::Handshaking,
                Handshake::Done => match self.lower_output {
                    LowerOutput::Wanted { .. } => Waiting::Room,
                    LowerOutput::Idle | LowerOutput::Granted { .. } => {
                        if self.lower_read.is_some() {
                            Waiting::Bytes
                        } else {
                            Waiting::Above
                        }
                    }
                },
            },
            State::Closing => match self.lower_output {
                LowerOutput::Wanted { .. } | LowerOutput::Granted { .. } => Waiting::Room,
                LowerOutput::Idle => Waiting::Close,
            },
            State::Failed | State::TransportClosing => Waiting::Close,
            State::Closed | State::TransportClosed => Waiting::Nothing,
        }
    }
}

/// An event from below. The caller reserves `UP_MAX_OUT` and drains
/// every right below, also after failure or closing.
pub fn up(
    client: &mut Client,
    _env: &Env<Limits>,
    event: LowerEvent,
    above: &mut Queue<Event>,
    below: &mut Queue<LowerRequest>,
) {
    match event {
        LowerEvent::Stream(stream) => stream_up(client, stream, above, below),
        LowerEvent::Output(OutputUp::Settled { right, outcome }) => output_up(client, right, outcome, above, below),
        LowerEvent::Closed => transport_closed(client, above),
    }
    settle(client, above, below);
}

/// A request from above. The caller reserves `DOWN_MAX_OUT`. Identity
/// is checked before size; Close after `TransportClosed` does nothing.
pub fn down(
    client: &mut Client,
    env: &Env<Limits>,
    request: Request,
    above: &mut Queue<Event>,
    below: &mut Queue<LowerRequest>,
) {
    match request {
        Request::Client(super::Request::Handshake) => start(client, env, above, below),
        Request::Client(super::Request::Stream(stream)) => stream_down(client, stream, &env.limits, above, below),
        Request::Client(super::Request::Close) => close(client, above, below),
        Request::Output(output) => output_down(client, output, &env.limits, above, below),
    }
    settle(client, above, below);
}

fn start(client: &mut Client, env: &Env<Limits>, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    let state = mem::replace(&mut client.state, State::Closed);
    let fresh = match state {
        State::Fresh(fresh) => fresh,
        State::Open(_)
        | State::Failed
        | State::Closing
        | State::TransportClosing
        | State::Closed
        | State::TransportClosed => unreachable!("one native Handshake before closing"),
    };
    let connected = match fresh.line {
        Line::Open => session::connect(&fresh.config, fresh.name, env.wall),
        Line::Ended => Err(Error::Truncated),
        Line::Failed(fault) => Err(Error::Stream(fault)),
    };
    match connected {
        Ok(tls) => {
            client.state =
                State::Open(Open { tls, handshake: Handshake::Running, peer: Peer::Open, writing: Writing::Open });
            process(client, above, below);
        }
        Err(error) => fail(client, error, above, below),
    }
}

fn process(client: &mut Client, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    let open = match &mut client.state {
        State::Open(open) => open,
        State::Fresh(_)
        | State::Failed
        | State::Closing
        | State::TransportClosing
        | State::Closed
        | State::TransportClosed => unreachable!("only open rustls processes records"),
    };
    match session::process(
        &mut open.tls,
        &mut client.buffers.records,
        &mut client.buffers.intake,
        &mut client.buffers.owed,
    ) {
        Ok(true) => open.peer = Peer::Closed,
        Ok(false) => {}
        Err(error) => {
            fail(client, error, above, below);
            return;
        }
    }
    if open.handshake == Handshake::Running && !session::handshaking(&open.tls) {
        open.handshake = Handshake::Done;
        above.push(Event::Client(super::Event::Ready(session::agreed(&open.tls))));
    }
}

fn stream_up(client: &mut Client, event: Up, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    match event {
        Up::Room => unreachable!("native lower stream reads only"),
        Up::Bytes(bytes) => received(client, bytes, above, below),
        Up::End => {
            client.lower_ended = true;
            withdraw_read(client, below);
            match &mut client.state {
                State::Fresh(fresh) => fresh.line = Line::Ended,
                State::Open(open) => {
                    if open.handshake == Handshake::Running {
                        fail(client, Error::Truncated, above, below);
                    } else if open.peer == Peer::Open {
                        open.peer = Peer::Cut;
                    }
                }
                State::Failed | State::Closing | State::TransportClosing | State::Closed | State::TransportClosed => {}
            }
        }
        Up::Failed(fault) => {
            client.lower_read = None;
            client.lower_ended = true;
            match &mut client.state {
                State::Fresh(fresh) => fresh.line = Line::Failed(fault),
                State::Open(_) => fail(client, Error::Stream(fault), above, below),
                State::Closing => {
                    client.buffers.owed.clear();
                    cancel_lower(client, below);
                }
                State::Failed | State::TransportClosing | State::Closed | State::TransportClosed => {}
            }
        }
    }
}

fn received(client: &mut Client, bytes: Box<[u8]>, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    match &client.state {
        State::Open(open) if open.handshake == Handshake::Done && client.reading == Reading::Withdrawn => {
            assert!(client.lower_read.is_none(), "ready withdrawal leaves no active ciphertext Fill");
            let read = client.withdrawn_read.take().expect("queued Bytes answers an withdrawn Fill");
            assert!(read == Read::Fill(super::fill(&bytes)), "withdrawn ciphertext Fill is answered exactly");
            return;
        }
        State::Open(_) => {}
        State::Fresh(_) => unreachable!("no native ciphertext read before Handshake"),
        State::Failed | State::Closing | State::TransportClosing | State::Closed | State::TransportClosed => {
            client.lower_read = None;
            return;
        }
    }
    let read = client.lower_read.take().expect("Bytes answers native ciphertext Fill");
    assert!(read == Read::Fill(super::fill(&bytes)), "ciphertext Fill is answered exactly");
    client.buffers.records.append(&bytes).expect("native read fits retained records");
    client.record = match client.record {
        Record::Header => match super::length(&bytes) {
            0 => Record::Header,
            length => Record::Body(length),
        },
        Record::Body(length) => {
            assert!(
                client.buffers.intake.room() >= length.min(MAX_PLAINTEXT),
                "plaintext room before native record read"
            );
            Record::Header
        }
    };
    drop(bytes);
    process(client, above, below);
}

fn stream_down(
    client: &mut Client,
    stream: Down,
    limits: &Limits,
    above: &mut Queue<Event>,
    below: &mut Queue<LowerRequest>,
) {
    match stream {
        Down::Send(_) => unreachable!("native plaintext uses named Output Send"),
        Down::Demand { read, room } => {
            assert!(room == 0, "native plaintext Demand is read-only");
            match &client.state {
                State::Open(open) => {
                    assert!(open.writing == Writing::Open, "no native read after Finish withdrawal");
                }
                State::Failed => return,
                State::Fresh(_) | State::Closing | State::TransportClosing | State::Closed | State::TransportClosed => {
                    unreachable!("native read in live admitted Handshake only")
                }
            }
            assert!(read_maximum(read) <= limits.read, "native plaintext read within cap");
            if read == Read::Nothing {
                assert!(client.upper_read != Read::Nothing, "only outstanding plaintext read is withdrawn");
                client.upper_read = Read::Nothing;
                client.reading = Reading::Withdrawn;
                match &client.state {
                    State::Open(open) if open.handshake == Handshake::Running => {}
                    State::Open(_) => withdraw_read(client, below),
                    State::Fresh(_)
                    | State::Failed
                    | State::Closing
                    | State::TransportClosing
                    | State::Closed
                    | State::TransportClosed => unreachable!("live native read withdrawal"),
                }
            } else {
                assert!(
                    client.reading != Reading::Withdrawn && client.upper_read == Read::Nothing,
                    "one upper native read before withdrawal"
                );
                client.upper_read = read;
            }
        }
        Down::Finish => finish(client, above, below),
    }
}

fn upper_terminal(client: &mut Client, outcome: OutputOutcome, above: &mut Queue<Event>) {
    let output = mem::replace(&mut client.upper_output, UpperOutput::Idle);
    match output {
        UpperOutput::Wanted { right, .. } => above.push(Event::Output(OutputUp::Settled { right, outcome })),
        UpperOutput::Idle | UpperOutput::Granted { .. } => {}
    }
}

fn output_down(
    client: &mut Client,
    output: OutputDown,
    limits: &Limits,
    above: &mut Queue<Event>,
    below: &mut Queue<LowerRequest>,
) {
    match output {
        OutputDown::Room { right, bytes } => {
            match &client.state {
                State::Open(open) => assert!(open.writing == Writing::Open, "native output before Finish"),
                State::Fresh(_)
                | State::Failed
                | State::Closing
                | State::TransportClosing
                | State::Closed
                | State::TransportClosed => unreachable!("Room requires live native writing"),
            }
            assert!(bytes > 0 && bytes <= limits.send, "positive plaintext reservation within cap");
            assert!(client.upper_output == UpperOutput::Idle, "one upper native output right");
            client.upper_output = UpperOutput::Wanted { right, bytes };
        }
        OutputDown::Cancel { right } => match client.upper_output {
            UpperOutput::Wanted { right: pending, .. } if right == pending => {
                upper_terminal(client, OutputOutcome::Cancelled, above);
                match client.lower_output {
                    LowerOutput::Wanted { purpose: Purpose::Upper { right: upper, .. }, .. } if upper == right => {
                        cancel_lower(client, below);
                    }
                    LowerOutput::Idle | LowerOutput::Wanted { .. } | LowerOutput::Granted { .. } => {}
                }
            }
            UpperOutput::Idle | UpperOutput::Wanted { .. } | UpperOutput::Granted { .. } => {}
        },
        OutputDown::Send { right, bytes } => match client.upper_output {
            UpperOutput::Granted { right: granted, bytes: cap } if granted == right => {
                assert!(
                    bytes.len() <= usize::try_from(cap).expect("plaintext cap fits"),
                    "matching native Send within grant"
                );
                client.upper_output = UpperOutput::Idle;
                send_plain(client, bytes, above, below);
            }
            UpperOutput::Idle | UpperOutput::Wanted { .. } | UpperOutput::Granted { .. } => {}
        },
        OutputDown::Release { right } => match client.upper_output {
            UpperOutput::Granted { right: granted, .. } if granted == right => {
                client.upper_output = UpperOutput::Idle;
            }
            UpperOutput::Idle | UpperOutput::Wanted { .. } | UpperOutput::Granted { .. } => {}
        },
    }
}

fn send_plain(client: &mut Client, plain: Box<[u8]>, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    let lower = mem::replace(&mut client.lower_output, LowerOutput::Idle);
    let (right, bytes) = match lower {
        LowerOutput::Granted { right, bytes } => (right, bytes),
        LowerOutput::Idle | LowerOutput::Wanted { .. } => unreachable!("upper grant retains lower backing"),
    };
    let open = match &mut client.state {
        State::Open(open) => open,
        State::Fresh(_)
        | State::Failed
        | State::Closing
        | State::TransportClosing
        | State::Closed
        | State::TransportClosed => unreachable!("matching live native Send"),
    };
    match session::encrypt(&mut open.tls, &mut client.buffers.records, &plain, &mut client.buffers.owed) {
        Ok(Some(sealed)) => {
            assert!(
                sealed.len() <= usize::try_from(bytes).expect("ciphertext cap fits"),
                "complete native ciphertext within full owed envelope"
            );
            below.push(LowerRequest::Output(OutputDown::Send { right, bytes: sealed }));
        }
        Ok(None) => below.push(LowerRequest::Output(OutputDown::Release { right })),
        Err(error) => {
            below.push(LowerRequest::Output(OutputDown::Release { right }));
            fail(client, error, above, below);
        }
    }
}

fn cancel_lower(client: &mut Client, below: &mut Queue<LowerRequest>) {
    client.lower_output = match client.lower_output {
        LowerOutput::Wanted { right, bytes, purpose, cancellation: Cancellation::None } => {
            below.push(LowerRequest::Output(OutputDown::Cancel { right }));
            LowerOutput::Wanted { right, bytes, purpose, cancellation: Cancellation::Requested }
        }
        LowerOutput::Granted { right, .. } => {
            below.push(LowerRequest::Output(OutputDown::Release { right }));
            LowerOutput::Idle
        }
        LowerOutput::Idle => LowerOutput::Idle,
        wanted @ LowerOutput::Wanted { cancellation: Cancellation::Requested, .. } => wanted,
    };
}

fn output_up(
    client: &mut Client,
    right: Token,
    outcome: OutputOutcome,
    above: &mut Queue<Event>,
    below: &mut Queue<LowerRequest>,
) {
    let (expected, bytes, purpose, cancellation) = match client.lower_output {
        LowerOutput::Wanted { right, bytes, purpose, cancellation } => (right, bytes, purpose, cancellation),
        LowerOutput::Idle | LowerOutput::Granted { .. } => unreachable!("one terminal for admitted lower right"),
    };
    assert!(right == expected, "lower token matches the admitted obligation");
    client.lower_output = LowerOutput::Idle;
    match outcome {
        OutputOutcome::Granted => {
            client.lower_output = LowerOutput::Granted { right, bytes };
            if cancellation == Cancellation::None {
                match purpose {
                    Purpose::Upper { right: upper, bytes: plaintext } => match client.upper_output {
                        UpperOutput::Wanted { right: wanted, bytes: asked }
                            if wanted == upper && asked == plaintext =>
                        {
                            client.upper_output = UpperOutput::Granted { right: upper, bytes: plaintext };
                            above.push(Event::Output(OutputUp::Settled {
                                right: upper,
                                outcome: OutputOutcome::Granted,
                            }));
                        }
                        UpperOutput::Idle | UpperOutput::Wanted { .. } | UpperOutput::Granted { .. } => {}
                    },
                    Purpose::Owed => {}
                }
            }
        }
        OutputOutcome::Cancelled => {
            if cancellation == Cancellation::None {
                upper_terminal(client, OutputOutcome::Cancelled, above);
                match client.state {
                    State::Closing => client.buffers.owed.clear(),
                    State::Fresh(_) | State::Open(_) | State::Failed => {
                        client.state = State::TransportClosing;
                        client.buffers.owed.clear();
                        above.push(Event::TransportClosing);
                    }
                    State::TransportClosing | State::Closed | State::TransportClosed => {}
                }
                withdraw_read(client, below);
            }
        }
        OutputOutcome::Failed(fault) => match client.state {
            State::Open(_) | State::Fresh(_) => fail(client, Error::Stream(fault), above, below),
            State::Closing => client.buffers.owed.clear(),
            State::Failed | State::TransportClosing | State::Closed | State::TransportClosed => {}
        },
    }
}

fn withdraw_read(client: &mut Client, below: &mut Queue<LowerRequest>) {
    if let Some(read) = client.lower_read.take() {
        assert!(client.withdrawn_read.replace(read).is_none(), "one withdrawn ciphertext Fill winner");
        below.push(LowerRequest::Stream(Down::Demand { read: Read::Nothing, room: 0 }));
    }
}

fn fail(client: &mut Client, error: Error, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    upper_terminal(client, OutputOutcome::Failed(error.fault()), above);
    if client.reading != Reading::Withdrawn {
        above.push(Event::Client(super::Event::Stream(Up::Failed(error.fault()))));
    }
    above.push(Event::Client(super::Event::Failed(error)));
    client.state = State::Failed;
    client.buffers.owed.clear();
    cancel_lower(client, below);
    withdraw_read(client, below);
    client.upper_read = Read::Nothing;
}

fn transport_closed(client: &mut Client, above: &mut Queue<Event>) {
    assert!(!lower_wanted(client.lower_output), "lower terminal precedes physical Closed");
    client.lower_output = LowerOutput::Idle;
    client.lower_read = None;
    client.withdrawn_read = None;
    client.upper_read = Read::Nothing;
    client.buffers.owed.clear();
    upper_terminal(client, OutputOutcome::Cancelled, above);
    match client.state {
        State::Closing => {
            above.push(Event::Client(super::Event::Closed));
            client.state = State::Closed;
        }
        State::Fresh(_) | State::Open(_) | State::Failed | State::TransportClosing => {
            above.push(Event::TransportClosed);
            client.state = State::TransportClosed;
        }
        State::Closed | State::TransportClosed => {}
    }
}

fn close(client: &mut Client, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    match client.state {
        State::TransportClosed => return,
        State::Closing | State::Closed => unreachable!("one native local Close"),
        State::Fresh(_) | State::Open(_) | State::Failed | State::TransportClosing => {}
    }
    upper_terminal(client, OutputOutcome::Cancelled, above);
    client.upper_read = Read::Nothing;
    client.reading = Reading::Withdrawn;
    withdraw_read(client, below);
    let written = match &mut client.state {
        State::Open(open) if open.handshake == Handshake::Done && open.writing != Writing::Finished => {
            if open.writing == Writing::Open || open.writing == Writing::Finishing {
                session::close_notify(&mut open.tls, &mut client.buffers.records, &mut client.buffers.owed).is_ok()
            } else {
                true
            }
        }
        State::Fresh(_) | State::Open(_) | State::Failed | State::TransportClosing => false,
        State::Closing | State::Closed | State::TransportClosed => unreachable!("checked before local Close"),
    };
    if !written {
        client.buffers.owed.clear();
    }
    client.state = State::Closing;
    if client.buffers.owed.is_empty() {
        cancel_lower(client, below);
    }
}

fn finish(client: &mut Client, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    let open = match &mut client.state {
        State::Open(open) => open,
        State::Fresh(_)
        | State::Failed
        | State::Closing
        | State::TransportClosing
        | State::Closed
        | State::TransportClosed => unreachable!("native Finish after Ready only"),
    };
    assert!(open.handshake == Handshake::Done && open.writing == Writing::Open, "native Finish after Ready once");
    assert!(
        client.upper_read == Read::Nothing || client.reading == Reading::Ended,
        "native Finish with no live upper read"
    );
    assert!(!upper_wanted(client.upper_output), "native Finish with no pending output");
    client.upper_output = UpperOutput::Idle;
    client.upper_read = Read::Nothing;
    client.reading = Reading::Withdrawn;
    open.writing = Writing::Flushing;
    match session::close_notify(&mut open.tls, &mut client.buffers.records, &mut client.buffers.owed) {
        Ok(()) => withdraw_read(client, below),
        Err(error) => fail(client, error, above, below),
    }
}

fn answer(client: &mut Client, above: &mut Queue<Event>) -> bool {
    let peer = match &client.state {
        State::Open(open) if open.handshake == Handshake::Done => open.peer,
        State::Fresh(_)
        | State::Open(_)
        | State::Failed
        | State::Closing
        | State::TransportClosing
        | State::Closed
        | State::TransportClosed => return true,
    };
    if client.reading != Reading::Open {
        return true;
    }
    if client.upper_read != Read::Nothing {
        if let Some(bytes) = client.buffers.intake.meet(client.upper_read) {
            client.upper_read = Read::Nothing;
            above.push(Event::Client(super::Event::Stream(Up::Bytes(bytes))));
            return true;
        }
    } else if !client.buffers.intake.is_empty() {
        return true;
    }
    match peer {
        Peer::Open => true,
        Peer::Cut => false,
        Peer::Closed => {
            client.reading = Reading::Ended;
            above.push(Event::Client(super::Event::Stream(Up::End)));
            true
        }
    }
}

fn request_room(
    client: &mut Client,
    bytes: u32,
    purpose: Purpose,
    above: &mut Queue<Event>,
    below: &mut Queue<LowerRequest>,
) {
    let Some(sequence) = client.next_lower else {
        match client.state {
            State::Closing => client.buffers.owed.clear(),
            State::Open(_) => fail(client, Error::Other, above, below),
            State::Fresh(_) | State::Failed | State::TransportClosing | State::Closed | State::TransportClosed => {
                unreachable!("only usable output needs a fresh lower token")
            }
        }
        return;
    };
    client.next_lower = sequence.checked_add(1);
    let right = Token::new(sequence);
    client.lower_output = LowerOutput::Wanted { right, bytes, purpose, cancellation: Cancellation::None };
    below.push(LowerRequest::Output(OutputDown::Room { right, bytes }));
}

fn settle(client: &mut Client, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    match client.state {
        State::Open(_) => {
            if !answer(client, above) {
                fail(client, Error::Truncated, above, below);
                return;
            }
        }
        State::Closing => {}
        State::Fresh(_) | State::Failed | State::TransportClosing | State::Closed | State::TransportClosed => {
            if lower_granted(client.lower_output) {
                cancel_lower(client, below);
            }
            return;
        }
    }
    settle_owed(client, above, below);
    settle_upper(client, above, below);
    match &mut client.state {
        State::Open(open) => {
            if open.writing == Writing::Flushing
                && client.buffers.owed.is_empty()
                && client.lower_output == LowerOutput::Idle
            {
                below.push(LowerRequest::Stream(Down::Finish));
                open.writing = Writing::Finished;
            }
        }
        State::Fresh(_)
        | State::Failed
        | State::Closing
        | State::TransportClosing
        | State::Closed
        | State::TransportClosed => {}
    }
    next_read(client, above, below);
}

fn settle_upper(client: &mut Client, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    let ready = match &client.state {
        State::Open(open) => open.handshake == Handshake::Done && open.writing == Writing::Open,
        State::Fresh(_)
        | State::Failed
        | State::Closing
        | State::TransportClosing
        | State::Closed
        | State::TransportClosed => false,
    };
    if !ready || !client.buffers.owed.is_empty() {
        return;
    }
    match client.upper_output {
        UpperOutput::Wanted { right, bytes } => match client.lower_output {
            LowerOutput::Idle => request_room(
                client,
                room_for(bytes).expect("checked native cap"),
                Purpose::Upper { right, bytes },
                above,
                below,
            ),
            LowerOutput::Wanted { .. } | LowerOutput::Granted { .. } => {}
        },
        UpperOutput::Idle | UpperOutput::Granted { .. } => {}
    }
}

fn settle_owed(client: &mut Client, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    if upper_granted(client.upper_output) {
        return;
    }
    match client.lower_output {
        LowerOutput::Granted { right, bytes } => {
            client.lower_output = LowerOutput::Idle;
            if client.buffers.owed.is_empty() {
                below.push(LowerRequest::Output(OutputDown::Release { right }));
            } else {
                below.push(LowerRequest::Output(OutputDown::Send { right, bytes: client.buffers.owed.take(bytes) }));
            }
        }
        LowerOutput::Idle | LowerOutput::Wanted { .. } => {}
    }
    if !client.buffers.owed.is_empty() && client.lower_output == LowerOutput::Idle {
        match client.state {
            State::Open(_) | State::Closing => {
                request_room(client, client.buffers.owed.len(), Purpose::Owed, above, below);
            }
            State::Fresh(_) | State::Failed | State::TransportClosing | State::Closed | State::TransportClosed => {}
        }
    }
}

fn next_read(client: &mut Client, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    if client.lower_ended || client.lower_read.is_some() {
        return;
    }
    let reads = match &client.state {
        State::Open(open) => {
            open.writing == Writing::Open
                && open.peer == Peer::Open
                && match open.handshake {
                    Handshake::Running => true,
                    Handshake::Done => client.reading == Reading::Open && client.upper_read != Read::Nothing,
                }
        }
        State::Fresh(_)
        | State::Failed
        | State::Closing
        | State::TransportClosing
        | State::Closed
        | State::TransportClosed => false,
    };
    if !reads {
        return;
    }
    let piece = match client.record {
        Record::Header => HEADER,
        Record::Body(bytes) => bytes,
    };
    if client.buffers.records.room() < piece {
        fail(client, Error::TooLong, above, below);
        return;
    }
    let read = Read::Fill(piece);
    client.lower_read = Some(read);
    below.push(LowerRequest::Stream(Down::Demand { read, room: 0 }));
}

fn upper_wanted(output: UpperOutput) -> bool {
    match output {
        UpperOutput::Wanted { .. } => true,
        UpperOutput::Idle | UpperOutput::Granted { .. } => false,
    }
}

fn upper_granted(output: UpperOutput) -> bool {
    match output {
        UpperOutput::Granted { .. } => true,
        UpperOutput::Idle | UpperOutput::Wanted { .. } => false,
    }
}

fn lower_wanted(output: LowerOutput) -> bool {
    match output {
        LowerOutput::Wanted { .. } => true,
        LowerOutput::Idle | LowerOutput::Granted { .. } => false,
    }
}

fn lower_granted(output: LowerOutput) -> bool {
    match output {
        LowerOutput::Granted { .. } => true,
        LowerOutput::Idle | LowerOutput::Wanted { .. } => false,
    }
}

fn read_maximum(read: Read) -> u32 {
    match read {
        Read::Nothing => 0,
        Read::Fill(bytes) | Read::Scan { max: bytes, .. } | Read::Line { max: bytes } => bytes,
    }
}

#[cfg(test)]
mod tests;
