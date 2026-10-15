//! Reports are taken in order, without re-entering a turn (ui-web.md, 5.3).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

/// The application's turn, called synchronously by the inbox.
pub type Turn<R> = Box<dyn FnMut(&Inbox<R>)>;

/// Reports queued for the application's single running turn.
pub struct Inbox<R> {
    reports: RefCell<VecDeque<R>>,
    running: Cell<bool>,
    turn: RefCell<Option<Turn<R>>>,
}

impl<R> Inbox<R> {
    #[must_use]
    pub fn new() -> Rc<Self> {
        Rc::new(Self { reports: RefCell::new(VecDeque::new()), running: Cell::new(false), turn: RefCell::new(None) })
    }

    /// Give the inbox its turn once, and take any reports already queued.
    pub fn start(&self, turn: Turn<R>) {
        let mut slot = self.turn.borrow_mut();
        assert!(slot.is_none(), "the inbox's turn is given only once");
        *slot = Some(turn);
        drop(slot);
        self.run();
    }

    /// Queue a report and call the turn now unless one is running.
    pub fn raise(&self, report: R) {
        self.reports.borrow_mut().push_back(report);
        self.run();
    }

    /// Take the oldest report, including reports raised during this turn.
    pub fn next(&self) -> Option<R> {
        self.reports.borrow_mut().pop_front()
    }

    fn run(&self) {
        if self.running.get() || self.reports.borrow().is_empty() {
            return;
        }
        self.running.set(true);
        if let Some(turn) = self.turn.borrow_mut().as_mut() {
            turn(self);
        }
        self.running.set(false);
    }
}
