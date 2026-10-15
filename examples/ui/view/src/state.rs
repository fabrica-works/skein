//! The page's facts and platform reports (examples.md, 9.2).
use crate::{Place, places, records};
use alloc::boxed::Box;
use skein_lib::{List, Queue, Reader, Time, Wall};
use skein_ui::{Act, Binding, BoundAct, Patch, View};

#[derive(Debug)]
pub struct Record {
    pub key: Box<[u8]>,
    pub value: Box<[u8]>,
}
#[derive(Debug)]
pub enum Report {
    Act(BoundAct),
    Place(Place),
    Answered { exchange: u32, status: u16, body: Box<[u8]> },
    Failed { exchange: u32 },
    Due,
}
#[derive(Debug)]
pub struct Exchange {
    pub id: u32,
    pub post: bool,
    pub path: Box<[u8]>,
    pub body: Box<[u8]>,
    pub bound: u32,
}
#[derive(Debug)]
pub struct Duties {
    pub writes: Queue<Record>,
    pub removes: Queue<Box<[u8]>>,
    pub exchanges: Queue<Exchange>,
    pub show: Option<(Place, bool)>,
}
#[derive(Debug)]
pub struct State {
    pub(crate) notes: records::Notes,
    pub(crate) place: Place,
    pub(crate) live: bool,
    pub(crate) words: skein_ui::phrases::Phrasebook,
    shown: u64,
}
impl State {
    #[must_use]
    pub fn start(_seed: u64, path: &[u8], _records: &[Record], _offset: i32, _languages: &[u8]) -> State {
        let notes = records::Notes::new(&records::CEILINGS, records::NotesParts { notes: List::with_capacity(64) })
            .expect("empty notes fit");
        State { notes, place: places::parse(path), live: false, words: crate::view::words(), shown: 0 }
    }
    pub fn step(&mut self, report: Report, _now: Time, _wall: Wall, duties: &mut Duties) {
        match report {
            Report::Place(place) => self.place = place,
            Report::Act(bound) => match bound.act {
                Act::Press => {
                    if let Some(place) = self.address(bound.binding) {
                        self.place = place;
                        duties.show = Some((place, false));
                    }
                }
                Act::Input { .. } | Act::Submit | Act::Toggle | Act::Drawing { .. } => {}
            },
            Report::Answered { exchange, status, body } => {
                if exchange == 0 && status == 200 {
                    let mut reader = Reader::new(&body);
                    if let Ok(notes) = records::Notes::decode(&records::CEILINGS, &mut reader) {
                        self.notes = notes;
                    }
                }
            }
            Report::Failed { .. } => self.live = false,
            Report::Due => {
                let bound =
                    u32::try_from(records::Notes::worst_case_bytes(&records::CEILINGS).expect("record bound fits"))
                        .expect("record bound fits u32");
                duties.exchanges.push(Exchange {
                    id: 0,
                    post: false,
                    path: Box::from(&b"/api/notes"[..]),
                    body: Box::from([]),
                    bound,
                });
            }
        }
        self.shown = self.shown.checked_add(1).expect("shown count cannot exhaust");
    }
    pub fn build(&self, view: &mut View, out: &mut Queue<Patch>) {
        if !view.stale(self.shown) {
            return;
        }
        let mut builder = view.builder();
        crate::view::page(self, &mut builder);
        view.finish(builder, self.shown, out);
    }
    #[must_use]
    pub fn address(&self, binding: Binding) -> Option<Place> {
        if binding.owner != crate::view::OWNER {
            return None;
        }
        for note in self.notes.notes() {
            if note.number() == binding.token.raw() {
                return Some(Place::Note { number: note.number() });
            }
        }
        None
    }
    #[must_use]
    pub fn deadline(&self) -> Option<Time> {
        None
    }
    #[must_use]
    pub fn closing_loses(&self) -> bool {
        false
    }
}
