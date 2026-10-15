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

struct Sequence<'a> {
    before: Script,
    person: &'a mut dyn Play,
    began: bool,
    done: Option<Rc<Cell<bool>>>,
    ready: Option<Rc<Cell<bool>>>,
}
impl Play for Sequence<'_> {
    fn next(&mut self, face: &mut BrowserFace<'_>, now: Time) -> Next<Box<[u8]>> {
        if !self.began {
            match self.before.next(face, now) {
                Next::Done => {
                    self.began = true;
                    if let Some(ready) = &self.ready {
                        ready.set(true);
                    }
                }
                other @ (Next::Do(_) | Next::Wait { .. } | Next::Failed { .. }) => return other,
            }
        }
        let next = self.person.next(face, now);
        if matches!(next, Next::Done)
            && let Some(done) = &self.done
        {
            done.set(true);
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
struct After<'a> {
    ready: Rc<Cell<bool>>,
    person: &'a mut dyn Play,
}
impl Play for After<'_> {
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
    let mut writer = Sequence {
        before: Script::new(Box::new([Step::Person(PersonStep::Go { address: address.into() })])),
        person: script,
        began: false,
        done: Some(Rc::clone(&done)),
        ready: None,
    };
    let mut reading = Script::new(Box::new([
        Step::Person(PersonStep::Go { address: b"/".as_slice().into() }),
        Step::Person(PersonStep::See { find: loaded, within: Duration::from_secs(5) }),
        Step::Person(PersonStep::Gone { find: absent, within: Duration::from_secs(1) }),
    ]));
    let mut reader = After { ready: done, person: &mut reading };
    harness(setup, vec![]).run(&mut [&mut writer, &mut reader])
}

struct Field {
    script: Script,
    find: Find,
    typed: bool,
}
impl Play for Field {
    fn next(&mut self, face: &mut BrowserFace<'_>, now: Time) -> Next<Box<[u8]>> {
        match self.script.next(face, now) {
            Next::Done => {}
            other @ (Next::Do(_) | Next::Wait { .. } | Next::Failed { .. }) => return other,
        }
        let expected: &[u8] = if self.typed { b"abc" } else { b"ab" };
        match skein_fake_person::Face::find(face, &self.find) {
            skein_fake_person::Found::Pending => return Next::Wait { until: None },
            skein_fake_person::Found::Absent => {
                return Next::Failed {
                    step: self.script.at(),
                    why: skein_fake_person::Why::Missing { find: self.find.clone() },
                };
            }
            skein_fake_person::Found::Node(node) => {
                if !face.field_is(node, expected) {
                    face.fail(format!("field lost focus or words {:?}", String::from_utf8_lossy(expected)));
                    return Next::Wait { until: None };
                }
            }
        }
        if self.typed {
            return Next::Done;
        }
        self.typed = true;
        self.script = Script::new(Box::new([Step::Type { find: self.find.clone(), words: b"abc".as_slice().into() }]));
        Next::Wait { until: None }
    }
}
/// A live patch preserves the field's composition, focus and insertion point.
#[must_use]
pub fn typing_while_patched(
    setup: &Setup<'_>,
    field: Find,
    second: &mut dyn Play,
    arrives: Find,
    loaded: Find,
) -> Outcome {
    let ready = Rc::new(Cell::new(false));
    let mut checking = Field {
        script: Script::new(Box::new([Step::Person(PersonStep::See {
            find: arrives.clone(),
            within: Duration::from_secs(5),
        })])),
        find: field.clone(),
        typed: false,
    };
    let mut first = Sequence {
        before: Script::new(Box::new([
            Step::Person(PersonStep::See { find: loaded, within: Duration::from_secs(5) }),
            Step::Compose { find: field, words: b"ab".as_slice().into() },
            Step::Person(PersonStep::Gone { find: arrives, within: Duration::from_secs(1) }),
        ])),
        person: &mut checking,
        began: false,
        done: None,
        ready: Some(Rc::clone(&ready)),
    };
    let mut second = After { ready, person: second };
    harness(setup, vec![]).run(&mut [&mut first, &mut second])
}
/// A directly opened address and each traversal show the application's named page.
pub struct Shown<'a> {
    pub address: &'a [u8],
    pub sees: Find,
}
#[must_use]
pub fn back_and_forward(setup: &Setup<'_>, first: Shown<'_>, second: Shown<'_>) -> Outcome {
    let see = |find| Step::Person(PersonStep::See { find, within: Duration::from_secs(5) });
    let go = |address: &[u8]| Step::Person(PersonStep::Go { address: address.into() });
    let mut script = Script::new(Box::new([
        go(first.address),
        see(first.sees.clone()),
        go(second.address),
        see(second.sees.clone()),
        Step::Back,
        see(first.sees.clone()),
        Step::Forward,
        see(second.sees.clone()),
        go(first.address),
        see(first.sees),
        go(second.address),
        see(second.sees),
    ]));
    harness(setup, vec![]).run(&mut [&mut script])
}
