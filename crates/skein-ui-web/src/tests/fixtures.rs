use std::cell::RefCell;
use std::rc::Rc;

use crate::inbox::Inbox;

pub fn collecting() -> (Rc<Inbox<u8>>, Rc<RefCell<Vec<u8>>>) {
    let inbox = Inbox::new();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&seen);
    inbox.start(Box::new(move |inbox| {
        while let Some(report) = inbox.next() {
            output.borrow_mut().push(report);
        }
    }));
    (inbox, seen)
}
