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
    &[skein_ui::look::Names { owner: OWNER, prefix: b"test", styles: &[b"test-mark"], regions: &[b"test-region"] }];
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

pub fn made(role: skein_ui::Role) -> skein_ui::Made {
    let mut builder = skein_ui::Builder::new(&LIMITS);
    builder.open(role);
    builder.close();
    let tree = builder.finish();
    let mut made = skein_ui::Made::of(tree.get(0).expect("fixture root"));
    made.id = skein_ui::NodeId(1);
    made
}
pub fn look() -> crate::Look<'static> {
    crate::Look { names: NAMES, pictures: &[(OWNER, &[b"picture.png"])], draw: no_drawing }
}
fn no_drawing(_: skein_ui::DrawingKind, _: &[u8], _: u16, _: u16, _: &mut skein_ui::scene::Scene) -> bool {
    false
}
