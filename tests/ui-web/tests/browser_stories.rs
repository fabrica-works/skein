//! The example supplies its names to the kit's browser-shell stories.
use skein_fake_person::{Find, Step};
use skein_fake_person_browser::{Outcome, Script, Setup, harness, stories};
use skein_lib::Duration;
use skein_ui::Role;
use skein_ui_web_tests::{chromium, example_bundle, server_binary};

fn setup() -> Setup<'static> {
    Setup {
        server: server_binary(),
        args: vec!["127.0.0.1:0".into(), "--bundle".into(), example_bundle().into_os_string()],
        listening: b"skein-ui-example: listening at ",
        chromium: chromium(),
        expected: vec![],
    }
}

#[test]
fn a_panic_reloads_then_says_it_failed() {
    assert_eq!(stories::panic(&setup(), b"/test/panic", Find::text(b"The application failed.")), Outcome::Done);
}

#[test]
fn a_refused_write_makes_no_note() {
    let mut writer = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::text(b"Live"), within: Duration::from_secs(5) }),
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"Groceries".as_slice().into(),
        }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
    ]));
    assert_eq!(
        stories::write_refused(
            &setup(),
            b"/test/small-store",
            &mut writer,
            Find::exactly_named(Role::Link, b"Groceries"),
            Find::text(b"Live")
        ),
        Outcome::Done
    );
}

#[test]
fn typing_in_the_title_survives_a_note_arriving() {
    let mut writer = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::text(b"Live"), within: Duration::from_secs(5) }),
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"Groceries".as_slice().into(),
        }),
        harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Add") }),
        harness::Step::Person(Step::See {
            find: Find::exactly_named(Role::Link, b"Groceries"),
            within: Duration::from_secs(5),
        }),
    ]));
    assert_eq!(
        stories::typing_while_patched(
            &setup(),
            Find::exactly_named(Role::TextField, b"Title"),
            &mut writer,
            Find::exactly_named(Role::Link, b"Groceries"),
            Find::text(b"Live")
        ),
        Outcome::Done
    );
}
#[test]
fn back_and_forward_move_between_the_list_and_a_note() {
    assert_eq!(
        stories::back_and_forward(
            &setup(),
            stories::Shown { address: b"/", sees: Find::exactly_named(Role::Heading { level: 1 }, b"Notes") },
            stories::Shown {
                address: b"/notes/7",
                sees: Find::exactly_named(Role::Heading { level: 1 }, b"No such note.")
            }
        ),
        Outcome::Done
    );
}
