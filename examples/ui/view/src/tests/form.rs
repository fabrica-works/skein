//! The form's validation and option acts, through the view's boundary.
use super::fixtures;
use crate::{Report, State, limits, view};
use skein_lib::{Queue, Time, Wall};
use skein_ui::{Act, Binding, BoundAct, Node, View};

fn act(state: &mut State, binding: Binding, act: Act) -> crate::Duties {
    let mut duties = fixtures::duties();
    state.step(Report::Act(BoundAct { binding, act }), Time::ZERO, Wall::EPOCH, &mut duties);
    duties
}
fn input(state: &mut State, binding: Binding, value: &[u8]) {
    act(state, binding, Act::Input { value: value.into() });
}
fn field(state: &State, binding: Binding) -> Node {
    let mut view = View::new(&limits::limits());
    state.build(&mut view, &mut Queue::with_capacity(1200));
    for node in view.tree().nodes() {
        if node.binding == Some(binding) {
            return node.clone();
        }
    }
    unreachable!("form field")
}
#[test]
fn an_empty_title_is_refused_in_words() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    act(&mut state, view::ADD, Act::Press);
    let title = field(&state, view::TITLE);
    assert_eq!(
        (title.states.as_slice(), title.description.as_deref()),
        (
            &[skein_ui::State::SubmitsOnEnter, skein_ui::State::Required, skein_ui::State::Invalid][..],
            Some(b"Enter a title.".as_slice())
        )
    );
}
#[test]
fn an_empty_title_sends_no_add() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    assert_eq!(act(&mut state, view::ADD, Act::Press).exchanges.len(), 0);
}
const BAD_MINUTES: &[&[u8]] = &[b"abc", b"3.5", b"-1", b"4294967296", b"18446744073709551616"];
#[test]
fn minutes_that_are_not_a_number_are_invalid() {
    for value in BAD_MINUTES {
        let mut state = State::start(1, b"/", &[], 0, b"en");
        input(&mut state, view::MINUTES, value);
        let minutes = field(&state, view::MINUTES);
        assert_eq!(
            (minutes.states.as_slice(), minutes.description.as_deref()),
            (&[skein_ui::State::Invalid][..], Some(b"Enter a whole number of minutes.".as_slice())),
            "{value:?}"
        );
    }
}
const DATES: &[(&[u8], bool)] = &[
    (b"", true),
    (b"2026-10-15", true),
    (b"2024-02-29", true),
    (b"2000-02-29", true),
    (b"2026-02-29", false),
    (b"1900-02-29", false),
    (b"2026-04-31", false),
    (b"2026-00-15", false),
    (b"2026-13-15", false),
    (b"2026-10-00", false),
    (b"0000-01-01", false),
    (b"2026-1-15", false),
    (b"2026/10/15", false),
    (b"yyyy-mm-dd", false),
];
#[test]
fn a_due_date_requires_a_calendar_date() {
    for &(value, expected) in DATES {
        let mut state = State::start(1, b"/", &[], 0, b"en");
        input(&mut state, view::DUE, value);
        assert_eq!(state.due_valid(), expected, "{value:?}");
    }
}
#[test]
fn an_invalid_due_date_is_explained_in_words() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    input(&mut state, view::DUE, b"2026-02-30");
    let due = field(&state, view::DUE);
    assert_eq!(
        (due.states.as_slice(), due.description.as_deref()),
        (&[skein_ui::State::Invalid][..], Some(b"Use YYYY-MM-DD for the due date.".as_slice()))
    );
}
#[test]
fn a_label_is_chosen_by_its_option() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    let (_, task) = view::LABELS[1];
    act(&mut state, task, Act::Press);
    assert_eq!(field(&state, task).states.as_slice(), &[skein_ui::State::Selected]);
}
#[test]
fn an_add_carries_each_form_value() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    for (binding, value) in
        [(view::TITLE, b"Shop".as_slice()), (view::BODY, b"Milk"), (view::DUE, b"2026-10-15"), (view::MINUTES, b"30")]
    {
        input(&mut state, binding, value);
    }
    act(&mut state, view::LABELS[1].1, Act::Press);
    let sent = act(&mut state, view::ADD, Act::Press).exchanges.pop().expect("add sent");
    let add = crate::records::Add::decode(&crate::records::CEILINGS, &mut skein_lib::Reader::new(&sent.body)).unwrap();
    assert_eq!(
        (add.title(), add.body(), add.due(), add.minutes(), add.label()),
        (b"Shop".as_slice(), b"Milk".as_slice(), b"2026-10-15".as_slice(), 30, &crate::records::Label::Task)
    );
}
#[test]
fn clearing_the_form_replaces_its_values() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    input(&mut state, view::BODY, b"Milk");
    act(&mut state, view::CLEAR, Act::Press);
    act(&mut state, view::DISCARD, Act::Press);
    let body = field(&state, view::BODY).value.expect("body value");
    assert_eq!((body.text.as_ref(), body.written), (b"".as_slice(), 1));
}

const DRAFT_FIELDS: &[(Binding, &[u8])] =
    &[(view::TITLE, b"Shop"), (view::BODY, b"Milk"), (view::DUE, b"2026-10-15"), (view::MINUTES, b"30")];
#[test]
fn clearing_words_asks_in_a_dialog() {
    for &(binding, value) in DRAFT_FIELDS {
        let mut state = State::start(1, b"/", &[], 0, b"en");
        input(&mut state, binding, value);
        act(&mut state, view::CLEAR, Act::Press);
        let mut tree = View::new(&limits::limits());
        state.build(&mut tree, &mut Queue::with_capacity(1200));
        let mut dialogs = skein_lib::List::with_capacity(1);
        for node in tree.tree().nodes() {
            if node.role == skein_ui::Role::Dialog {
                dialogs.push(node.name.as_deref()).expect("one dialog");
            }
        }
        assert_eq!(dialogs.as_slice(), &[Some(b"Discard draft?".as_slice())], "{binding:?}");
    }
}
#[test]
fn the_keep_button_names_escape() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    input(&mut state, view::TITLE, b"Shop");
    act(&mut state, view::CLEAR, Act::Press);
    assert_eq!(field(&state, view::KEEP).shortcut, Some(skein_ui::Shortcut::ESCAPE));
}
#[test]
fn an_add_moves_the_focus_to_the_title() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    for title in [b"Shop".as_slice(), b"Read"] {
        input(&mut state, view::TITLE, title);
        act(&mut state, view::ADD, Act::Press);
    }
    assert_eq!(field(&state, view::TITLE).focus, Some(2));
}
#[test]
fn a_discard_moves_the_focus_to_the_title() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    input(&mut state, view::BODY, b"Milk");
    act(&mut state, view::CLEAR, Act::Press);
    act(&mut state, view::DISCARD, Act::Press);
    assert_eq!(field(&state, view::TITLE).focus, Some(1));
}
#[test]
fn keeping_the_draft_preserves_its_words() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    input(&mut state, view::TITLE, b"Shop");
    act(&mut state, view::CLEAR, Act::Press);
    act(&mut state, view::KEEP, Act::Press);
    assert_eq!(field(&state, view::TITLE).value.unwrap().text.as_ref(), b"Shop");
}

#[test]
fn keeping_the_draft_closes_the_dialog() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    input(&mut state, view::TITLE, b"Shop");
    act(&mut state, view::CLEAR, Act::Press);
    act(&mut state, view::KEEP, Act::Press);
    let mut tree = View::new(&limits::limits());
    state.build(&mut tree, &mut Queue::with_capacity(1200));
    let mut count = 0_u32;
    for node in tree.tree().nodes() {
        if node.role == skein_ui::Role::Dialog {
            count += 1;
        }
    }
    assert_eq!(count, 0);
}
#[test]
fn keeping_the_draft_returns_focus_to_the_title() {
    let mut state = State::start(1, b"/", &[], 0, b"en");
    input(&mut state, view::TITLE, b"Shop");
    act(&mut state, view::CLEAR, Act::Press);
    act(&mut state, view::KEEP, Act::Press);
    assert_eq!(field(&state, view::TITLE).focus, Some(1));
}
