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

pub const OWNER: skein_ui::Owner = skein_ui::Owner { number: 1 };
pub const NAMES: &[skein_ui::look::Names<'_>] =
    &[skein_ui::look::Names { owner: OWNER, prefix: b"test", styles: &[b"test-mark"], regions: &[] }];
pub const LIMITS: skein_ui::Limits = skein_ui::Limits {
    nodes: 32,
    depth: 8,
    text_per_node: 256,
    text_bytes: 4096,
    drawing_bytes: 128,
    styles_per_node: 4,
    patches: 128,
    bindings: 32,
    slots: 4,
    markdown_depth: 8,
    scene_marks: 16,
    scene_bytes: 256,
};
