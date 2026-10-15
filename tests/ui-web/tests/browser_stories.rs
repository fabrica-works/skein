//! The example supplies its names to the kit's browser-shell stories.
use skein_fake_person::Find;
use skein_fake_person_browser::{Outcome, Setup, stories};
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
