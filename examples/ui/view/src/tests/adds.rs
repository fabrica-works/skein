use super::fixtures;
use crate::{Record, Report, State};
use skein_lib::{Time, Wall};

#[test]
fn an_add_is_written_before_it_is_sent() {
    let mut out = fixtures::added(&mut State::start(1, b"/", &[], 0, b"en"));
    let written = out.writes.pop().expect("add kept");
    let sent = out.exchanges.pop().expect("add sent");
    assert_eq!(
        (written.key.as_ref(), written.value.as_ref(), sent.body.as_ref()),
        (
            b"add-0000000000000001".as_slice(),
            b"\0\0\0\0\0\0\0\x01\0\0\0\x01A\0\0\0\0\0\0\0\0\0\0\0\0\0".as_slice(),
            b"\0\0\0\0\0\0\0\x01\0\0\0\x01A\0\0\0\0\0\0\0\0\0\0\0\0\0".as_slice()
        )
    );
}
#[test]
fn an_answered_add_leaves_the_store() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    fixtures::added(&mut state);
    let mut out = fixtures::duties();
    state.step(
        Report::Answered { exchange: 1, status: 200, body: fixtures::ADD_RECORD.into() },
        Time::ZERO,
        Wall::EPOCH,
        &mut out,
    );
    assert_eq!(out.removes.pop().expect("answered add removed").as_ref(), b"add-0000000000000001");
}
#[test]
fn a_pending_add_is_sent_again_at_start() {
    let records = [Record { key: b"add-0000000000000001".as_slice().into(), value: fixtures::ADD_RECORD.into() }];
    let mut state = State::start(2, b"/", &records, 0, b"en");
    let mut out = fixtures::duties();
    state.step(Report::Due, Time::ZERO, Wall::EPOCH, &mut out);
    let sent = out.exchanges.pop().expect("pending add resent");
    assert_eq!(sent.body.as_ref(), b"\0\0\0\0\0\0\0\x01\0\0\0\x01A\0\0\0\0\0\0\0\0\0\0\0\0\0");
}

#[test]
fn a_list_answer_keeps_a_note_already_added() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    fixtures::added(&mut state);
    state.step(
        Report::Answered { exchange: 1, status: 200, body: fixtures::ADD_RECORD.into() },
        Time::ZERO,
        Wall::EPOCH,
        &mut fixtures::duties(),
    );
    let notes = fixtures::notes();
    let mut writer = skein_lib::Writer::new(usize::try_from(notes.measure()).unwrap());
    notes.encode(&mut writer).unwrap();
    state.step(
        Report::Answered { exchange: 0, status: 200, body: writer.finish() },
        Time::ZERO,
        Wall::EPOCH,
        &mut fixtures::duties(),
    );
    let mut numbers = skein_lib::List::with_capacity(3);
    for note in state.notes.notes() {
        numbers.push(note.number()).unwrap();
    }
    assert_eq!(numbers.as_slice(), &[1, 7, 9]);
}

#[test]
fn enter_submits_the_title() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    let mut out = fixtures::duties();
    for act in [skein_ui::Act::Input { value: b"A".as_slice().into() }, skein_ui::Act::Submit] {
        state.step(
            Report::Act(skein_ui::BoundAct { binding: crate::view::TITLE, act }),
            Time::ZERO,
            Wall::EPOCH,
            &mut out,
        );
    }
    assert_eq!(out.writes.len(), 1);
}

#[test]
fn the_title_field_has_its_accessible_name() {
    let state = State::start(1, b"/", &[], 0, b"en");
    let mut view = skein_ui::View::new(&crate::limits::limits());
    state.build(&mut view, &mut skein_lib::Queue::with_capacity(1200));
    let mut name = None;
    for node in view.tree().nodes() {
        if node.role == skein_ui::Role::TextField {
            name = node.name.clone();
        }
    }
    assert_eq!(name.as_deref(), Some(b"Title".as_slice()));
}

#[test]
fn the_add_button_has_its_own_name() {
    let state = State::start(1, b"/", &[], 0, b"en");
    let mut view = skein_ui::View::new(&crate::limits::limits());
    state.build(&mut view, &mut skein_lib::Queue::with_capacity(1200));
    let mut name = None;
    for node in view.tree().nodes() {
        if node.role == skein_ui::Role::Button {
            name = node.name.clone();
        }
    }
    assert_eq!(name.as_deref(), Some(b"Add".as_slice()));
}
