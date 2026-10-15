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
        listening: b"skein-ui-example: listening at ",
        chromium: skein_ui_web_tests::chromium(),
        expected: vec![b"Failed to load resource: net::ERR_CONNECTION_REFUSED".as_slice().into()],
    });
    let outcome = harness.run(&mut [&mut person]);
    assert_eq!(outcome, Outcome::Done);
}
