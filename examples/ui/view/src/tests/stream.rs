use super::fixtures;
use crate::{Report, State};
use skein_lib::{Time, Wall, Writer};

const EVENT: &[u8] = b"data: AAAAAAAAAAEAAAABQQAAAAAAAAAAAAAAAAA=\n\n";

#[test]
fn a_note_seen_twice_shows_once() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    for _ in 0_u32..2 {
        state.step(Report::StreamPiece { bytes: EVENT.into() }, Time::ZERO, Wall::EPOCH, &mut fixtures::duties());
    }
    assert_eq!(state.notes.notes().len(), 1);
}

#[test]
fn a_stream_note_crosses_browser_pieces() {
    for split in 1..EVENT.len() {
        let mut state = State::start(1, b"/", &[], 0, b"en");
        for bytes in [&EVENT[..split], &EVENT[split..]] {
            state.step(Report::StreamPiece { bytes: bytes.into() }, Time::ZERO, Wall::EPOCH, &mut fixtures::duties());
        }
        assert_eq!(state.notes.notes().len(), 1, "split {split}");
    }
}

#[test]
fn live_from_the_lists_answer_until_the_stream_ends() {
    let notes = fixtures::notes();
    let mut writer = Writer::new(usize::try_from(notes.measure()).unwrap());
    notes.encode(&mut writer).unwrap();
    let mut state = State::start(1, b"/", &[], 0, b"en");
    let mut seen = skein_lib::List::with_capacity(5);
    seen.push(state.live).unwrap();
    for report in [
        Report::StreamOpened,
        Report::Answered { exchange: 0, status: 200, body: writer.finish() },
        Report::Failed { exchange: 1 },
        Report::StreamEnded,
    ] {
        state.step(report, Time::ZERO, Wall::EPOCH, &mut fixtures::duties());
        seen.push(state.live).unwrap();
    }
    assert_eq!(seen.as_slice(), &[false, false, true, true, false]);
}

#[test]
fn the_list_is_read_after_the_stream_opens() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    let mut out = fixtures::duties();
    state.step(Report::StreamOpened, Time::ZERO, Wall::EPOCH, &mut out);
    assert_eq!(out.exchanges.pop().unwrap().path.as_ref(), b"/api/notes");
}
