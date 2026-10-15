//! Browser-shell stories parameterized by the application's pages (ui-web.md, 14.2).
use crate::{BrowserFace, Harness, Outcome, Play, Script, Setup, Step};
use skein_fake_person::Next;
use skein_fake_person::{Find, Step as PersonStep};
use skein_lib::{Duration, Time};
use std::{cell::Cell, rc::Rc};

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

struct Finishes<'a> {
    before: Script,
    person: &'a mut dyn Play,
    began: bool,
    done: Rc<Cell<bool>>,
}
impl Play for Finishes<'_> {
    fn next(&mut self, face: &mut BrowserFace<'_>, now: Time) -> Next<Box<[u8]>> {
        if !self.began {
            match self.before.next(face, now) {
                Next::Done => self.began = true,
                other @ (Next::Do(_) | Next::Wait { .. } | Next::Failed { .. }) => return other,
            }
        }
        let next = self.person.next(face, now);
        if matches!(next, Next::Done) {
            self.done.set(true);
        }
        next
    }
    fn at(&self) -> u32 {
        self.before.at().saturating_add(self.person.at())
    }
    fn server_action(&mut self) -> Option<crate::harness::ServerAction> {
        self.person.server_action()
    }
}
struct After {
    ready: Rc<Cell<bool>>,
    person: Script,
}
impl Play for After {
    fn next(&mut self, face: &mut BrowserFace<'_>, now: Time) -> Next<Box<[u8]>> {
        if self.ready.get() { self.person.next(face, now) } else { Next::Wait { until: None } }
    }
    fn at(&self) -> u32 {
        self.person.at()
    }
}
/// A refused write stays absent in a second person's fully loaded page.
#[must_use]
pub fn write_refused(setup: &Setup<'_>, address: &[u8], script: &mut dyn Play, absent: Find, loaded: Find) -> Outcome {
    let done = Rc::new(Cell::new(false));
    let mut writer = Finishes {
        before: Script::new(Box::new([Step::Person(PersonStep::Go { address: address.into() })])),
        person: script,
        began: false,
        done: Rc::clone(&done),
    };
    let mut reader = After {
        ready: done,
        person: Script::new(Box::new([
            Step::Person(PersonStep::Go { address: b"/".as_slice().into() }),
            Step::Person(PersonStep::See { find: loaded, within: Duration::from_secs(5) }),
            Step::Person(PersonStep::Gone { find: absent, within: Duration::from_secs(1) }),
        ])),
    };
    harness(setup, vec![]).run(&mut [&mut writer, &mut reader])
}
