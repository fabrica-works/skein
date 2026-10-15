//! Native dialogs run through the real Chromium rig.
use super::{EXPECT, Event, FIND, PAGE, PRESS, Request, Rig, Scenario, Token, query};
use skein_browser::boundary::{Expect, Go, Trouble};
use skein_lib::Duration;
const RELOAD: Token = Token::new(20);
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Hold,
    Reload,
    Before,
    HoldAgain,
    Close,
    Ask,
    Answer,
}
pub(super) struct State {
    phase: Phase,
    warned: bool,
    dialog_seen: bool,
    answer_seen: bool,
}
impl State {
    pub(super) fn new(scenario: Scenario) -> Self {
        Self {
            phase: if matches!(scenario, Scenario::Confirm) { Phase::Ask } else { Phase::Hold },
            warned: false,
            dialog_seen: false,
            answer_seen: false,
        }
    }
}
impl Rig {
    fn await_heading(&mut self, name: &[u8]) {
        self.ask(Request::Await {
            page: PAGE,
            op: EXPECT,
            query: query(b"heading", name),
            expect: Expect::Present,
            within: Duration::from_secs(1),
        });
    }
    pub(super) fn dialog_event(&mut self, event: Event) -> Result<(), Event> {
        match event {
            Event::Opened { page } => {
                assert_eq!(page, PAGE);
                self.ask(Request::Find {
                    page: PAGE,
                    op: FIND,
                    query: query(b"button", if self.dialog.phase == Phase::Ask { b"Ask" } else { b"Hold" }),
                });
            }
            Event::Found { op, seen, more } => {
                assert_eq!(op, FIND);
                assert_eq!(more, 0);
                assert_eq!(seen.len(), 1);
                let node = seen.get(0).expect("dialog control").node;
                self.ask(Request::Press { page: PAGE, op: PRESS, node, at: None });
            }
            Event::Done { op: PRESS } => {
                if self.dialog.phase == Phase::Ask {
                    self.dialog.phase = Phase::Answer;
                    self.await_heading(b"Answer: false");
                } else {
                    self.await_heading(b"Held");
                }
            }
            Event::Warned { op } => {
                assert_eq!(op, RELOAD);
                assert_eq!(self.dialog.phase, Phase::Reload);
                assert!(!self.dialog.warned, "one warning per go");
                self.dialog.warned = true;
            }
            Event::Done { op: RELOAD } => {
                assert_eq!(self.dialog.phase, Phase::Reload);
                assert!(self.dialog.warned, "the reload accepted its warning before Done");
                self.dialog.phase = Phase::Before;
                self.await_heading(b"Before");
            }
            Event::Met { op, seen } => {
                assert_eq!(op, EXPECT);
                assert_eq!(seen.len(), 1);
                self.dialog_met();
            }
            Event::Closed { owner: PAGE } if matches!(self.scenario, Scenario::Leave) => {
                assert_eq!(self.dialog.phase, Phase::Close);
                self.close_browser();
            }
            Event::Trouble { page, trouble: Trouble::Dialog, text } => {
                assert_eq!(page, PAGE);
                assert!(matches!(self.dialog.phase, Phase::Ask | Phase::Answer));
                assert_eq!(text.as_ref(), b"confirm: Delete it?");
                self.dialog.dialog_seen = true;
                if self.dialog.answer_seen {
                    self.close_browser();
                }
            }
            other => return Err(other),
        }
        Ok(())
    }
    fn dialog_met(&mut self) {
        match self.dialog.phase {
            Phase::Hold => {
                self.dialog.phase = Phase::Reload;
                self.ask(Request::Go { page: PAGE, op: RELOAD, to: Go::Reload });
            }
            Phase::Before => {
                self.dialog.phase = Phase::HoldAgain;
                self.ask(Request::Find { page: PAGE, op: FIND, query: query(b"button", b"Hold") });
            }
            Phase::HoldAgain => {
                self.dialog.phase = Phase::Close;
                self.ask(Request::Close { entity: PAGE });
            }
            Phase::Answer => {
                self.dialog.answer_seen = true;
                if self.dialog.dialog_seen {
                    self.close_browser();
                }
            }
            Phase::Reload | Phase::Close | Phase::Ask => panic!("unexpected heading at {:?}", self.dialog.phase),
        }
    }
}
