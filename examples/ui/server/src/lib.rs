//! Bounded in-memory notes, separate from the server's routes (examples.md, 9.3).
#![forbid(unsafe_code)]
use skein_lib::{List, Writer};
use skein_ui_example_view::records::{CEILINGS, Notes, NotesParts};
pub mod bundle;
pub mod routes;

#[derive(Debug)]
pub struct State {
    notes: Notes,
}
impl State {
    #[must_use]
    pub fn new() -> State {
        State {
            notes: Notes::new(&CEILINGS, NotesParts { notes: List::with_capacity(CEILINGS.notes_notes) })
                .expect("empty notes fit"),
        }
    }
    #[must_use]
    pub fn list(&self) -> Box<[u8]> {
        let mut writer = Writer::new(usize::try_from(self.notes.measure()).expect("record fits usize"));
        self.notes.encode(&mut writer).expect("measured record encodes");
        writer.finish()
    }
}
impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
#[cfg(test)]
mod tests;
