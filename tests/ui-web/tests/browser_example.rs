//! The reference shell starts the example under the strict browser policy.
use skein_fake_person::{Find, Step};
use skein_fake_person_browser::{Outcome, Script, harness};
use skein_ui::Role;
use skein_ui_web_tests::{SECOND, example_harness};

#[test]
fn a_person_opens_the_example_and_sees_its_heading() {
    let mut person = Script::new(Box::new([
        harness::Step::Person(Step::See {
            find: Find::exactly_named(Role::Heading { level: 1 }, b"Notes"),
            within: SECOND,
        }),
        harness::Step::Person(Step::See { find: Find::text(b"No notes yet."), within: SECOND }),
    ]));
    let outcome = example_harness().run(&mut [&mut person]);
    assert_eq!(outcome, Outcome::Done);
}

#[test]
fn a_reload_keeps_a_pending_add_made_once() {
    let mut person = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::TextField, b"Title"), within: SECOND }),
        harness::Step::StopServer,
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"Groceries".as_slice().into(),
        }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
        harness::Step::Person(Step::Pause { span: skein_lib::Duration::from_millis(100) }),
        harness::Step::StartServer,
        harness::Step::Person(Step::Reload),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"Groceries"), within: SECOND }),
        harness::Step::Person(Step::Reload),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"Groceries"), within: SECOND }),
    ]));
    let mut harness = harness::Harness::new(harness::Setup {
        server: skein_ui_web_tests::server_binary(),
        args: vec!["127.0.0.1:0".into(), "--bundle".into(), skein_ui_web_tests::example_bundle().into_os_string()],
        printed: skein_fake_person_browser::harness::Printed {
            prefix: b"skein-ui-example: listening at ",
            on: skein_fake_person_browser::harness::Output::Stderr,
            names: skein_fake_person_browser::harness::Names::Address,
        },
        restart: vec!["{address}".into(), "--bundle".into(), skein_ui_web_tests::example_bundle().into_os_string()],
        chromium: skein_ui_web_tests::chromium(),
        origins: vec![],
        expected: vec![
            b"Failed to load resource: net::ERR_CONNECTION_REFUSED".as_slice().into(),
            b"Failed to load resource: net::ERR_INCOMPLETE_CHUNKED_ENCODING".as_slice().into(),
        ],
    });
    let outcome = harness.run(&mut [&mut person]);
    assert_eq!(outcome, Outcome::Done);
}

#[test]
fn a_note_added_in_one_page_shows_in_another() {
    let mut reader = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::text(b"Live"), within: SECOND }),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"Groceries"), within: SECOND }),
    ]));
    let mut writer = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::text(b"Live"), within: SECOND }),
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"Groceries".as_slice().into(),
        }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"Groceries"), within: SECOND }),
    ]));
    assert_eq!(example_harness().run(&mut [&mut reader, &mut writer]), Outcome::Done);
}

#[test]
fn a_person_opens_a_note_and_comes_back_by_its_link() {
    let mut person = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::TextField, b"Title"), within: SECOND }),
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"Groceries".as_slice().into(),
        }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"Groceries"), within: SECOND }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Link, b"Groceries") }),
        harness::Step::Person(Step::See {
            find: Find::exactly_named(Role::Heading { level: 1 }, b"Groceries"),
            within: SECOND,
        }),
        harness::Step::Title("Groceries · Notes".as_bytes().into()),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Link, b"Back to notes") }),
        harness::Step::Person(Step::See {
            find: Find::exactly_named(Role::Heading { level: 1 }, b"Notes"),
            within: SECOND,
        }),
    ]));
    assert_eq!(example_harness().run(&mut [&mut person]), Outcome::Done);
}

#[test]
fn a_reload_with_an_add_pending_meets_the_closing_warning() {
    let mut person = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::TextField, b"Title"), within: SECOND }),
        harness::Step::StopServer,
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"Groceries".as_slice().into(),
        }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
        harness::Step::Person(Step::Pause { span: skein_lib::Duration::from_millis(100) }),
        harness::Step::StartServer,
        harness::Step::Person(Step::Reload),
        harness::Step::Warned(true),
    ]));
    let mut harness = harness::Harness::new(harness::Setup {
        server: skein_ui_web_tests::server_binary(),
        args: vec!["127.0.0.1:0".into(), "--bundle".into(), skein_ui_web_tests::example_bundle().into_os_string()],
        printed: skein_fake_person_browser::harness::Printed {
            prefix: b"skein-ui-example: listening at ",
            on: skein_fake_person_browser::harness::Output::Stderr,
            names: skein_fake_person_browser::harness::Names::Address,
        },
        restart: vec!["{address}".into(), "--bundle".into(), skein_ui_web_tests::example_bundle().into_os_string()],
        chromium: skein_ui_web_tests::chromium(),
        origins: vec![],
        expected: vec![
            b"Failed to load resource: net::ERR_CONNECTION_REFUSED".as_slice().into(),
            b"Failed to load resource: net::ERR_INCOMPLETE_CHUNKED_ENCODING".as_slice().into(),
        ],
    });
    assert_eq!(harness.run(&mut [&mut person]), Outcome::Done);
}
#[test]
fn a_reload_once_the_add_is_made_meets_no_closing_warning() {
    let mut person = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::TextField, b"Title"), within: SECOND }),
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"Groceries".as_slice().into(),
        }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"Groceries"), within: SECOND }),
        harness::Step::Person(Step::Reload),
        harness::Step::Warned(false),
    ]));
    assert_eq!(example_harness().run(&mut [&mut person]), Outcome::Done);
}

#[test]
fn a_person_fills_each_entry_kind_and_chooses_a_label() {
    let mut steps = vec![harness::Step::Person(Step::See { find: Find::text(b"Live"), within: SECOND })];
    for (role, name, words) in [
        (Role::TextField, b"Title".as_slice(), b"Shopping".as_slice()),
        (Role::TextArea, b"Body", b"Milk and bread"),
        (Role::TextField, b"Due date", b"2026-10-15"),
        (Role::TextField, b"Minutes", b"30"),
    ] {
        steps.push(harness::Step::Person(Step::Type { find: Find::exactly_named(role, name), words: words.into() }));
    }
    steps.extend([
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Option, b"task") }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"Shopping"), within: SECOND }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Link, b"Shopping") }),
        harness::Step::Person(Step::See { find: Find::text(b"Milk and bread"), within: SECOND }),
        harness::Step::Person(Step::See { find: Find::text(b"2026-10-15"), within: SECOND }),
        harness::Step::Person(Step::See { find: Find::text(b"30"), within: SECOND }),
        harness::Step::Person(Step::See { find: Find::text(b"task"), within: SECOND }),
    ]);
    let mut person = Script::new(steps.into_boxed_slice());
    assert_eq!(example_harness().run(&mut [&mut person]), Outcome::Done);
}

#[test]
fn a_person_chooses_a_label_by_its_option() {
    let mut person = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::text(b"Live"), within: SECOND }),
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"A reference".as_slice().into(),
        }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Option, b"reference") }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"A reference"), within: SECOND }),
        harness::Step::Person(Step::See { find: Find::text(b"reference"), within: SECOND }),
    ]));
    assert_eq!(example_harness().run(&mut [&mut person]), Outcome::Done);
}

#[test]
fn escape_in_the_dialog_keeps_the_draft() {
    let mut person = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::TextField, b"Title"), within: SECOND }),
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"Groceries".as_slice().into(),
        }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Clear") }),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Dialog, b"Discard draft?"), within: SECOND }),
        harness::Step::Key(skein_browser::boundary::Key::Escape),
        harness::Step::Person(Step::Gone {
            find: Find::exactly_named(Role::Dialog, b"Discard draft?"),
            within: SECOND,
        }),
        harness::Step::Focused(Find::exactly_named(Role::TextField, b"Title")),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"Groceries"), within: SECOND }),
    ]));
    assert_eq!(example_harness().run(&mut [&mut person]), Outcome::Done);
}

#[test]
fn a_press_on_a_bar_shows_its_labels_notes() {
    let mut person = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::text(b"Live"), within: SECOND }),
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"An idea".as_slice().into(),
        }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"An idea"), within: SECOND }),
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"A task".as_slice().into(),
        }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Option, b"task") }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"A task"), within: SECOND }),
        harness::Step::PressAt {
            find: Find::exactly_named(Role::Drawing, b"Notes by label"),
            at: skein_browser::boundary::Point { x: 128, y: 60 },
        },
        harness::Step::Person(Step::Gone { find: Find::exactly_named(Role::Link, b"An idea"), within: SECOND }),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"A task"), within: SECOND }),
    ]));
    assert_eq!(example_harness().run(&mut [&mut person]), Outcome::Done);
}
#[test]
fn the_charts_fallback_is_read_by_its_name() {
    let mut person = Script::new(Box::new([
        harness::Step::Person(Step::See {
            find: Find::exactly_named(Role::Drawing, b"Notes by label"),
            within: SECOND,
        }),
        harness::Step::Person(Step::See { find: Find::text(b"idea: 0"), within: SECOND }),
        harness::Step::Person(Step::See { find: Find::text(b"task: 0"), within: SECOND }),
        harness::Step::Person(Step::See { find: Find::text(b"reference: 0"), within: SECOND }),
    ]));
    assert_eq!(example_harness().run(&mut [&mut person]), Outcome::Done);
}

#[test]
fn the_body_grows_with_its_words_and_reports_them() {
    let mut person = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::text(b"Live"), within: SECOND }),
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"Three lines".as_slice().into(),
        }),
        harness::Step::Taller {
            find: Find::exactly_named(Role::TextArea, b"Body"),
            words: b"Milk\nBread\nButter".as_slice().into(),
        },
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Link, b"Three lines"), within: SECOND }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Link, b"Three lines") }),
        harness::Step::Person(Step::See { find: Find::text(b"Milk Bread Butter"), within: SECOND }),
    ]));
    assert_eq!(example_harness().run(&mut [&mut person]), Outcome::Done);
}
