//! The output rights and their settling (tls.md, 3.6).

use super::{
    Client, Error, Event, Handshake, Limits, LowerRequest, State, Writing, fail, open, room_for, withdraw_read,
};
use crate::session;
use alloc::boxed::Box;
use core::mem;
use skein_lib::stream::{OutputDown, OutputOutcome, OutputUp};
use skein_lib::{Queue, Token};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UpperOutput {
    Idle,
    Wanted { right: Token, bytes: u32 },
    Granted { right: Token, bytes: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Purpose {
    Owed,
    Upper { right: Token, bytes: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cancellation {
    None,
    Requested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LowerOutput {
    Idle,
    Wanted { right: Token, bytes: u32, purpose: Purpose, cancellation: Cancellation },
    Granted { right: Token, bytes: u32 },
}

pub(super) fn upper_terminal(client: &mut Client, outcome: OutputOutcome, above: &mut Queue<Event>) {
    let output = mem::replace(&mut client.upper_output, UpperOutput::Idle);
    match output {
        UpperOutput::Wanted { right, .. } => above.push(Event::Output(OutputUp::Settled { right, outcome })),
        UpperOutput::Idle | UpperOutput::Granted { .. } => {}
    }
}

pub(super) fn output_down(
    client: &mut Client,
    output: OutputDown,
    limits: &Limits,
    above: &mut Queue<Event>,
    below: &mut Queue<LowerRequest>,
) {
    match output {
        OutputDown::Room { right, bytes } => {
            let Some(open) = open(&mut client.state) else { unreachable!("Room requires live native writing") };
            assert!(open.writing == Writing::Open, "native output before Finish");
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

pub(super) fn send_plain(
    client: &mut Client,
    plain: Box<[u8]>,
    above: &mut Queue<Event>,
    below: &mut Queue<LowerRequest>,
) {
    let lower = mem::replace(&mut client.lower_output, LowerOutput::Idle);
    let (right, bytes) = match lower {
        LowerOutput::Granted { right, bytes } => (right, bytes),
        LowerOutput::Idle | LowerOutput::Wanted { .. } => unreachable!("upper grant retains lower backing"),
    };
    let Some(open) = open(&mut client.state) else { unreachable!("matching live native Send") };
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

pub(super) fn cancel_lower(client: &mut Client, below: &mut Queue<LowerRequest>) {
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

pub(super) fn output_up(
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
                    State::Open(_) | State::Failed => {
                        client.state = State::TransportClosing;
                        client.buffers.owed.clear();
                        above.push(Event::TransportClosing);
                    }
                    State::Fresh(_) | State::TransportClosing | State::Closed | State::TransportClosed => {}
                }
                withdraw_read(client, below);
            }
        }
        OutputOutcome::Failed(fault) => match client.state {
            State::Open(_) => fail(client, Error::Stream(fault), above, below),
            State::Closing => client.buffers.owed.clear(),
            State::Failed | State::Fresh(_) | State::TransportClosing | State::Closed | State::TransportClosed => {}
        },
    }
}

pub(super) fn request_room(
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

pub(super) fn settle_upper(client: &mut Client, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
    let Some(open) = open(&mut client.state) else { return };
    let ready = open.handshake == Handshake::Done && open.writing == Writing::Open;
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

pub(super) fn settle_owed(client: &mut Client, above: &mut Queue<Event>, below: &mut Queue<LowerRequest>) {
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

pub(super) fn upper_wanted(output: UpperOutput) -> bool {
    match output {
        UpperOutput::Wanted { .. } => true,
        UpperOutput::Idle | UpperOutput::Granted { .. } => false,
    }
}

pub(super) fn upper_granted(output: UpperOutput) -> bool {
    match output {
        UpperOutput::Granted { .. } => true,
        UpperOutput::Idle | UpperOutput::Wanted { .. } => false,
    }
}

pub(super) fn lower_wanted(output: LowerOutput) -> bool {
    match output {
        LowerOutput::Wanted { .. } => true,
        LowerOutput::Idle | LowerOutput::Granted { .. } => false,
    }
}

pub(super) fn lower_granted(output: LowerOutput) -> bool {
    match output {
        LowerOutput::Granted { .. } => true,
        LowerOutput::Idle | LowerOutput::Wanted { .. } => false,
    }
}
