//! Notes shared by the view tests.
use crate::{
    Duties, Report, State,
    records::{self, Label, Note, NoteParts, Notes, NotesParts},
};
use alloc::boxed::Box;
use skein_lib::{List, Queue, Time, Wall, Writer};
pub const GROCERIES: &[u8] = b"Groceries";
pub const DENTIST: &[u8] = b"Dentist";
pub const ADD_RECORD: &[u8] = b"\0\0\0\0\0\0\0\x01\0\0\0\x01A\0\0\0\0\0\0\0\0\0\0\0\0\0";
pub fn added(state: &mut State) -> Duties {
    let mut out = duties();
    for (binding, act) in [
        (crate::view::TITLE, skein_ui::Act::Input { value: b"A".as_slice().into() }),
        (crate::view::ADD, skein_ui::Act::Press),
    ] {
        state.step(Report::Act(skein_ui::BoundAct { binding, act }), Time::ZERO, Wall::EPOCH, &mut out);
    }
    out
}

pub fn notes() -> Notes {
    let mut notes = List::with_capacity(64);
    for (number, title) in [(7, GROCERIES), (9, DENTIST)] {
        notes
            .push(
                Note::new(
                    &records::CEILINGS,
                    NoteParts {
                        number,
                        title: Box::from(title),
                        body: Box::from(&b"A note"[..]),
                        due: Box::from(&b"2026-10-15"[..]),
                        minutes: 30,
                        label: Label::Task,
                    },
                )
                .expect("fixture fits"),
            )
            .expect("two notes fit");
    }
    Notes::new(&records::CEILINGS, NotesParts { notes }).expect("two notes fit")
}
pub fn duties() -> Duties {
    Duties {
        writes: Queue::with_capacity(8),
        removes: Queue::with_capacity(8),
        exchanges: Queue::with_capacity(8),
        show: None,
        stream: None,
    }
}
pub fn state() -> State {
    let notes = notes();
    let mut writer = Writer::new(usize::try_from(notes.measure()).expect("record fits"));
    notes.encode(&mut writer).expect("fixture encodes");
    let mut state = State::start(1, b"/", &[], 0, b"en");
    state.step(
        Report::Answered { exchange: 0, status: 200, body: writer.finish() },
        Time::from_nanos(0),
        Wall::from_nanos(0),
        &mut duties(),
    );
    state
}
