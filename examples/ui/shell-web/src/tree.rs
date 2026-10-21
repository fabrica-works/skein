//! Debug-only views for live keyed patches through the tree's DOM groups.
use super::NAMES;
use skein_lib::{Queue, Token};
use skein_ui::{Act, Binding, Builder, Key, NodeId, Owner, Role, Run, View};
use skein_ui_web::{
    Look,
    inbox::Inbox,
    page::{Page, web::Dom},
};
use std::{cell::RefCell, rc::Rc};

const OWNER: Owner = Owner { number: 1 };
const NEXT: Binding = Binding { owner: OWNER, token: Token::new(1) };

enum Report {
    Start,
    Act(NodeId, Act),
}

pub(super) fn start(mount: web_sys::Element, path: &str, primary: skein_ui_web::mapping::Primary) {
    let case = path.strip_prefix("/test/tree/").expect("tree fixture route").to_owned();
    assert!(["first", "middle", "move", "rename", "last", "mixed"].contains(&case.as_str()));
    let mut limits = skein_ui_example_view::limits::limits();
    limits.depth = 7; // Page, main, tree, three item levels, and their text.
    let page = Rc::new(RefCell::new(Page::new(
        Look { names: NAMES, pictures: &[], components: &[], elements: &[], draw: skein_ui_example_view::draw::draw },
        &limits,
        b"notes",
        primary,
    )));
    let inbox = Inbox::new();
    let dom = Dom::new(mount, &page, &inbox, Report::Act, &[]);
    let mut view = View::new(&limits);
    let mut revision = 0_u64;
    inbox.start(Box::new(move |inbox| {
        while let Some(report) = inbox.next() {
            match report {
                Report::Start => {}
                Report::Act(node, act) => {
                    if let Some(bound) = view.act(node, act)
                        && bound.binding == NEXT
                        && bound.act == Act::Press
                    {
                        revision = revision.checked_add(1).expect("fixture revision fits");
                    }
                }
            }
        }
        if !view.stale(revision) {
            return;
        }
        let mut builder = view.builder();
        build(&mut builder, &case, revision);
        let mut patches = Queue::with_capacity(limits.patches);
        view.finish(builder, revision, &mut patches);
        let patches: Vec<_> = std::iter::from_fn(|| patches.pop()).collect();
        let mut ops = Vec::new();
        page.borrow_mut().apply(&patches, &mut ops);
        dom.apply(&ops);
    }));
    inbox.raise(Report::Start);
}

fn item(builder: &mut Builder, key: u64, name: &[u8]) {
    builder.open(Role::TreeItem);
    builder.key(Key { owner: OWNER, kind: 0, a: key, b: 0 });
    builder.name(name);
    builder.text(Run::Plain, name);
}

fn build(builder: &mut Builder, case: &str, revision: u64) {
    builder.open(Role::Page);
    builder.open(Role::Main);
    builder.open(Role::Button);
    builder.name(b"Patch tree");
    builder.bind(NEXT);
    builder.close();
    builder.open(Role::Status);
    let revision_text = format!("Tree revision {revision}");
    builder.name(revision_text.as_bytes());
    builder.text(Run::Plain, revision_text.as_bytes());
    builder.close();
    builder.open(Role::Tree);
    builder.name(b"Patch tree");
    item(builder, 100, b"Parent");
    let changed = revision != 0;
    let children: &[u64] = match (case, changed) {
        ("first", false) | ("last", true) => &[],
        ("first", true) | ("last", false) => &[1],
        ("middle" | "mixed", false) => &[1, 3],
        ("move", true) => &[3, 1, 2],
        ("mixed", true) if revision == 1 => &[1, 0, 3],
        ("mixed", true) if revision == 2 => &[1, 2, 0, 3, 4],
        ("mixed", true) => &[1, 2, 3, 4],
        ("middle" | "move" | "rename", _) => &[1, 2, 3],
        _ => unreachable!("declared fixture cases"),
    };
    for key in children {
        if *key == 0 {
            builder.open(Role::Paragraph);
            builder.name(b"Between runs");
            builder.text(Run::Plain, b"Between runs");
        } else {
            let name: &[u8] = match key {
                1 => b"One",
                2 if case == "rename" && changed => b"Renamed two",
                2 => b"Two",
                3 => b"Three",
                4 => b"Four",
                _ => unreachable!("declared child keys"),
            };
            item(builder, *key, name);
            // The keyed item keeps its own nested group and descendant.
            if *key == 1 {
                item(builder, 10, b"Grandchild");
                builder.close();
            }
        }
        builder.close();
    }
    builder.close();
    item(builder, 200, b"Stable");
    item(builder, 201, b"Stable leaf");
    builder.close();
    builder.close();
    builder.close();
    builder.close();
    builder.close();
}
