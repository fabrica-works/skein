//! The example's patches mapped by the real host page part.
use skein_lib::{List, Queue, Time, Wall, Writer};
use skein_ui::{Owner, Role, Style, View, look::Names};
use skein_ui_example_view::{Duties, Place, Report, State, limits, places, records};
use skein_ui_web::{Look, mapping::Primary, page::Page};
const OWNER: Owner = Owner { number: 1 };
const NAMES: &[Names<'_>] = &[Names { owner: OWNER, prefix: b"notes", styles: &[b"notes-card"], regions: &[] }];
fn no_drawing(_: skein_ui::DrawingKind, _: &[u8], _: u16, _: u16, _: &mut skein_ui::scene::Scene) -> bool {
    false
}
fn state() -> State {
    let mut notes = List::with_capacity(64);
    notes
        .push(
            records::Note::new(
                &records::CEILINGS,
                records::NoteParts {
                    number: 7,
                    title: b"Groceries".as_slice().into(),
                    body: b"A note".as_slice().into(),
                    due: b"2026-10-15".as_slice().into(),
                    minutes: 30,
                    label: records::Label::Task,
                },
            )
            .unwrap(),
        )
        .unwrap();
    let notes = records::Notes::new(&records::CEILINGS, records::NotesParts { notes }).unwrap();
    let mut writer = Writer::new(usize::try_from(notes.measure()).unwrap());
    notes.encode(&mut writer).unwrap();
    let mut state = State::start(1, b"/", &[], 0, b"en");
    state.step(
        Report::Answered { exchange: 0, status: 200, body: writer.finish() },
        Time::ZERO,
        Wall::EPOCH,
        &mut duties(),
    );
    state
}
fn duties() -> Duties {
    Duties {
        writes: Queue::with_capacity(8),
        removes: Queue::with_capacity(8),
        exchanges: Queue::with_capacity(8),
        show: None,
        stream: None,
    }
}
fn html(state: &State) -> String {
    let limits = limits::limits();
    let mut view = View::new(&limits);
    let mut patches = Queue::with_capacity(limits.patches);
    state.build(&mut view, &mut patches);
    let mut page = Page::new(
        Look {
            names: NAMES,
            pictures: &[],
            draw: no_drawing,
            elements: &[(OWNER, Role::Region, Style { owner: OWNER, token: 0 }, "article")],
        },
        &limits,
        b"notes",
        Primary::Control,
    );
    page.apply(&patches.into_iter().cloned().collect::<Vec<_>>(), &mut Vec::new());
    for node in page.links().to_vec() {
        let binding = view.tree().bound(node).expect("bound link");
        let place = state.address(binding).expect("example link");
        page.set_href(node, &places::bytes(place), &mut Vec::new());
    }
    page.html()
}
#[test]
fn the_list_page_maps_as_its_golden_html() {
    assert_eq!(html(&state()), include_str!("../golden/list.html").trim_end());
}
#[test]
fn the_note_page_maps_as_its_golden_html() {
    let mut state = state();
    state.step(Report::Place(Place::Note { number: 7 }), Time::ZERO, Wall::EPOCH, &mut duties());
    assert_eq!(html(&state), include_str!("../golden/note.html").trim_end());
}
