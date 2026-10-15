//! Bounded in-memory notes, separate from the server's routes (examples.md, 9.3).
#![forbid(unsafe_code)]
use skein_lib::{List, Reader, Writer};
use skein_ui_example_view::records::{Add, CEILINGS, Note, NoteParts, Notes, NotesParts};
use std::collections::BTreeMap;
pub mod bundle;
pub mod routes;

#[derive(Debug)]
pub struct State {
    notes: Notes,
    adds: BTreeMap<u64, Note>,
    made: Option<Box<[u8]>>,
}
impl State {
    #[must_use]
    pub fn new() -> State {
        State {
            adds: BTreeMap::new(),
            made: None,
            notes: Notes::new(&CEILINGS, NotesParts { notes: List::with_capacity(CEILINGS.notes_notes) })
                .expect("empty notes fit"),
        }
    }
    pub fn add(&mut self, body: &[u8]) -> Result<Box<[u8]>, u16> {
        let add = Add::decode(&CEILINGS, &mut Reader::new(body)).map_err(|_| 400_u16)?;
        let mut made = false;
        let note = if let Some(note) = self.adds.get(&add.key()) {
            note.clone()
        } else {
            let mut notes = self.notes.clone().into_parts();
            if notes.notes.room() == 0 {
                return Err(409);
            }
            let number = u64::from(notes.notes.len()).checked_add(1).expect("64 notes fit");
            let parts = add.into_parts();
            let note = Note::new(
                &CEILINGS,
                NoteParts {
                    number,
                    title: parts.title,
                    body: parts.body,
                    due: parts.due,
                    minutes: parts.minutes,
                    label: parts.label,
                },
            )
            .expect("add and note have the same bounds");
            notes.notes.push(note.clone()).expect("note capacity checked");
            self.notes = Notes::new(&CEILINGS, notes).expect("bounded notes");
            self.adds.insert(parts.key, note.clone());
            made = true;
            note
        };
        let mut writer = Writer::new(usize::try_from(note.measure()).expect("note fits"));
        note.encode(&mut writer).expect("measured note fits");
        let bytes = writer.finish();
        if made {
            self.made = Some(bytes.clone());
        }
        Ok(bytes)
    }
    pub fn take_made(&mut self) -> Option<Box<[u8]>> {
        self.made.take()
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
