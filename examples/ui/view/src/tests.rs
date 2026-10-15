//! The list, addresses and capacities (examples.md, 9.5).
mod adds;
mod drift;
mod fixtures;
mod person;
mod text;
use crate::{Place, State, limits, places};
use skein_lib::{Queue, Writer};
use skein_ui::{Role, View};

const PATHS: &[(&[u8], Place, &[u8])] = &[
    (b"/", Place::List, b"/"),
    (b"/notes/7", Place::Note { number: 7 }, b"/notes/7"),
    (b"/notes/07", Place::Missing, b"/missing"),
    (b"/x", Place::Missing, b"/missing"),
    (b"/?x=1", Place::List, b"/"),
    (b"/notes/0", Place::Note { number: 0 }, b"/notes/0"),
    (b"/notes/18446744073709551615", Place::Note { number: u64::MAX }, b"/notes/18446744073709551615"),
    (b"/notes/18446744073709551616", Place::Missing, b"/missing"),
    (b"/notes/7/", Place::Missing, b"/missing"),
    (b"/notes/", Place::Missing, b"/missing"),
];
#[test]
fn places_print_and_parse_back() {
    for &(path, expected, printed) in PATHS {
        assert_eq!(places::parse(path), expected, "path {path:?}");
        let mut out = Writer::new(printed.len());
        places::print(expected, &mut out);
        let bytes = out.finish();
        assert_eq!(bytes.as_ref(), printed, "path {path:?}");
        assert_eq!(places::parse(&bytes), expected, "printed {path:?}");
    }
}

#[test]
fn a_paths_box_holds_exactly_its_printed_bytes() {
    for &(_, place, printed) in PATHS {
        assert_eq!(places::bytes(place).as_ref(), printed, "place {place:?}");
    }
}
#[test]
fn the_list_shows_each_note_by_its_key() {
    let state = fixtures::state();
    let mut view = View::new(&limits::limits());
    state.build(&mut view, &mut Queue::with_capacity(1200));
    let mut keys = skein_lib::List::with_capacity(2);
    for node in view.tree().nodes() {
        if node.role == Role::Item {
            keys.push(node.key.expect("item keyed").a).expect("two items fit");
        }
    }
    assert_eq!(keys.as_slice(), &[7, 9]);
}
#[test]
fn the_empty_list_says_no_notes_yet() {
    let state = State::start(1, b"/", &[], 0, b"en");
    let mut view = View::new(&limits::limits());
    state.build(&mut view, &mut Queue::with_capacity(1200));
    let mut found = false;
    for node in view.tree().nodes() {
        for run in &node.text {
            if run.text.as_ref() == b"No notes yet." {
                found = true;
            }
        }
    }
    assert!(found, "empty page explains itself");
}
#[test]
fn the_views_worst_case_fits_its_limits() {
    let mut state = fixtures::state();
    let mut notes = skein_lib::List::with_capacity(64);
    for number in 0..64 {
        let mut parts = fixtures::notes().notes().get(0).expect("fixture note").clone().into_parts();
        parts.number = number;
        parts.title = Box::from([b't'; 256]);
        parts.body = Box::from([b'b'; 4096]);
        parts.due = Box::from([b'd'; 32]);
        notes
            .push(crate::records::Note::new(&crate::records::CEILINGS, parts).expect("ceiling fits"))
            .expect("64 notes fit");
    }
    state.notes = crate::records::Notes::new(&crate::records::CEILINGS, crate::records::NotesParts { notes })
        .expect("64 notes fit");
    let mut view = View::new(&limits::limits());
    state.build(&mut view, &mut Queue::with_capacity(1200));
    assert_eq!(view.tree().len(), 461);
    assert!(limits::worst_case().expect("worst case fits u64") < 100_000_000, "view fits its 100 MB memory bound");
}
