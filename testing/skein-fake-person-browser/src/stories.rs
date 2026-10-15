//! Browser-shell stories parameterized by the application's pages (ui-web.md, 14.2).
use crate::{Harness, Outcome, Script, Setup, Step};
use skein_fake_person::{Find, Step as PersonStep};
use skein_lib::Duration;

fn harness(setup: &Setup<'_>, expected: Vec<Box<[u8]>>) -> Harness {
    Harness::new(Setup {
        server: setup.server.clone(),
        args: setup.args.clone(),
        listening: setup.listening,
        chromium: setup.chromium.clone(),
        expected,
    })
}

/// The test address panics on every start; the restart part eventually stops it.
#[must_use]
pub fn panic(setup: &Setup<'_>, address: &[u8], failed: Find) -> Outcome {
    let mut script = Script::new(Box::new([
        Step::Person(PersonStep::Go { address: address.into() }),
        Step::Person(PersonStep::See { find: failed, within: Duration::from_secs(3) }),
    ]));
    harness(setup, vec![Box::from(&b"Uncaught"[..])]).run(&mut [&mut script])
}
