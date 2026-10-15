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
