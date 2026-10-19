//! A call walks retained script pieces without copying their list.
//!
//! Contract: docs/design/fake-llm.md, sections 2.2 and 5.
use crate::api::{Answer, Failure, Line, Piece, Script, Usage};
use crate::domain::{Request, State};
use alloc::boxed::Box;
use skein_lib::bytes::copy_of;
use skein_lib::{List, Queue, ReplyTo, Token};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Place {
    pub script: usize,
    pub turn: usize,
}

#[derive(Debug)]
pub(crate) struct Stream {
    pub reply_to: ReplyTo,
    pub call: Token,
    pub result: Result<Answer, Failure>,
    pub place: Place,
    pub next: usize,
    pub passed: bool,
}

pub(crate) fn valid(scripts: &[Script]) -> bool {
    for script in scripts {
        for turn in &script.turns {
            let mut ended = false;
            for (index, line) in turn.lines.iter().enumerate() {
                if ended {
                    return false;
                }
                match line {
                    Line::Fail { credentials, .. } => ended = credentials.is_empty(),
                    Line::Pieces { pieces } => {
                        if index != 0 || pieces.is_empty() {
                            return false;
                        }
                        for piece in pieces {
                            if piece.text.is_empty() {
                                return false;
                            }
                            if let Some(name) = &piece.hold
                                && (name.is_empty() || name.len() > 64)
                            {
                                return false;
                            }
                        }
                    }
                    Line::Text { .. } | Line::Opaque { .. } | Line::Call { .. } => {}
                }
            }
        }
    }
    true
}

pub(crate) fn join(pieces: &[Piece]) -> Box<[u8]> {
    let mut len: u32 = 0;
    for piece in pieces {
        len = len
            .checked_add(u32::try_from(piece.text.len()).expect("admitted script bytes"))
            .expect("admitted answer bytes");
    }
    let mut text = List::with_capacity(len);
    for piece in pieces {
        for byte in &piece.text {
            text.push(*byte).expect("room for joined text");
        }
    }
    text.into_boxed()
}

fn at(scripts: &[Script], place: Place) -> &[Piece] {
    let turn = scripts.get(place.script).expect("retained script").turns.get(place.turn).expect("retained turn");
    match turn.lines.first() {
        Some(Line::Pieces { pieces }) => pieces,
        Some(Line::Fail { .. } | Line::Text { .. } | Line::Opaque { .. } | Line::Call { .. }) | None => {
            unreachable!("a stream starts with pieces")
        }
    }
}

pub(crate) fn held_at(state: &State, scripts: &[Script], name: &[u8]) -> bool {
    match state {
        State::Held(stream) => {
            at(scripts, stream.place).get(stream.next).expect("held piece").hold.as_deref() == Some(name)
        }
        State::Streaming(_) | State::Thinking { .. } | State::Closed => false,
    }
}

pub(crate) fn fire(mut stream: Stream, scripts: &[Script], out: &mut Queue<Request>) -> State {
    let pieces = at(scripts, stream.place);
    let piece = pieces.get(stream.next).expect("next piece remains");
    let usage = match &stream.result {
        Ok(answer) => answer.usage,
        Err(_) => Usage::NONE,
    };
    if piece.hold.is_some() && !stream.passed {
        out.push(Request::Hold { call: stream.call, usage });
        return State::Held(stream);
    }
    stream.next = stream.next.checked_add(1).expect("bounded piece index");
    if stream.next == pieces.len() {
        out.push(Request::Reply { to: stream.reply_to, result: stream.result });
        return State::Closed;
    }
    out.push(Request::Piece { call: stream.call, usage, text: copy_of(&piece.text) });
    stream.passed = false;
    State::Streaming(stream)
}
