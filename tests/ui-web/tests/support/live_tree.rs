//! DOM structure and backend-node identity, before and after each browser-driven patch.
use skein_fake_person::{Face, Find, Found, Next, Step, Why};
use skein_fake_person_browser::{
    BrowserFace, Outcome, Script,
    harness::{self, Play},
};
use skein_lib::Time;
use skein_ui::{NodeId, Role};
use skein_ui_web_tests::SECOND;
use std::collections::{BTreeMap, VecDeque};

enum Part {
    Script(Box<Script>),
    Identity { key: &'static str, name: &'static str },
}

struct TreeStory {
    parts: VecDeque<Part>,
    nodes: BTreeMap<&'static str, NodeId>,
    at: u32,
}
impl TreeStory {
    fn new(case: &str) -> Self {
        let mut story = Self { parts: VecDeque::new(), nodes: BTreeMap::new(), at: 0 };
        story
            .steps(vec![harness::Step::Person(Step::Go { address: format!("/test/tree/{case}").into_bytes().into() })]);
        story.revision(0);
        story
    }
    fn steps(&mut self, steps: Vec<harness::Step>) {
        self.parts.push_back(Part::Script(Box::new(Script::new(steps.into()))));
    }
    fn identity(&mut self, name: &'static str) {
        self.parts.push_back(Part::Identity { key: name, name });
    }
    fn revision(&mut self, revision: u64) {
        self.steps(vec![harness::Step::Person(Step::See {
            find: Find::exactly_named(Role::Status, format!("Tree revision {revision}").as_bytes()),
            within: SECOND,
        })]);
    }
    fn patch(&mut self, revision: u64) {
        self.steps(vec![harness::Step::Person(Step::Press { find: Find::exactly_named(Role::Button, b"Patch tree") })]);
        self.revision(revision);
    }
    fn dom(&mut self, selector: String, attribute: &str, expected: &str) {
        self.steps(vec![harness::Step::Keep {
            selector: selector.into_bytes().into(),
            attribute: Some(attribute.as_bytes().into()),
            name: expected.as_bytes().into(),
        }]);
    }
    fn layout(&mut self, parent: &str, runs: &[&[&str]]) {
        let mut selector = format!("[role=treeitem][aria-label='{parent}']:has(> span:first-child)");
        // Every item has its visible caption first; the remaining direct children are these runs.
        selector.push_str(&format!(":not(:has(> :nth-child({})))", runs.len() + 2));
        for (run, names) in runs.iter().enumerate() {
            if names.is_empty() {
                selector.push_str(&format!(":has(> p[aria-label='Between runs']:nth-child({}))", run + 2));
                continue;
            }
            let group = format!("> [role=group]:not([data-node]):nth-child({})", run + 2);
            for (at, name) in names.iter().enumerate() {
                selector.push_str(&format!(
                    ":has({group} > [role=treeitem][aria-label='{name}']:nth-child({}){})",
                    at + 1,
                    if at + 1 == names.len() { ":last-child" } else { "" },
                ));
            }
        }
        self.dom(selector, "aria-label", parent);
    }
    fn tree(&mut self, children: &[&[&'static str]]) {
        self.dom(
            "[role=tree][aria-label='Patch tree']:has(> [aria-label=Parent]:first-child):has(> [aria-label=Stable]:nth-child(2):last-child):not(:has([role=group]:empty))".into(),
            "aria-label", "Patch tree",
        );
        self.layout("Parent", children);
        self.layout("Stable", &[&["Stable leaf"]]);
        self.layout("Stable leaf", &[]);
        for name in ["Parent", "Stable", "Stable leaf"] {
            self.identity(name);
        }
        for name in children.iter().flat_map(|run| run.iter()) {
            self.layout(name, if *name == "One" { &[&["Grandchild"]] } else { &[] });
            self.identity(name);
            if *name == "One" {
                self.layout("Grandchild", &[]);
                self.identity("Grandchild");
            }
        }
    }
    fn run(mut self) {
        let mut harness = harness::Harness::new(super::setup());
        assert_eq!(harness.run(&mut [&mut self]), Outcome::Done);
        assert!(!harness.kept().is_empty());
        for kept in harness.kept() {
            assert_eq!(kept.value, kept.name, "live DOM selector's attribute");
        }
    }
}
impl Play for TreeStory {
    fn next(&mut self, face: &mut BrowserFace<'_>, now: Time) -> Next<Box<[u8]>> {
        let Some(part) = self.parts.front_mut() else { return Next::Done };
        match part {
            Part::Script(script) => match script.next(face, now) {
                Next::Done => {}
                next => return next,
            },
            Part::Identity { key, name } => {
                let find = Find::exactly_named(Role::TreeItem, name.as_bytes());
                match face.find(&find) {
                    Found::Pending => return Next::Wait { until: None },
                    Found::Absent => return Next::Failed { step: self.at, why: Why::Missing { find } },
                    Found::Node(node) => {
                        // BrowserFace's ids are interned by Chromium backend node id, not data-node.
                        if let Some(previous) = self.nodes.insert(key, node) {
                            assert_eq!(previous, node, "{key} keeps its live DOM node as {name}");
                        }
                    }
                }
            }
        }
        self.parts.pop_front();
        self.at += 1;
        Next::Wait { until: None }
    }
    fn at(&self) -> u32 {
        self.at
    }
    fn check_deadline(&mut self, face: &mut BrowserFace<'_>, now: Time) {
        if let Some(Part::Script(script)) = self.parts.front_mut() {
            script.check_deadline(face, now);
        }
    }
}

#[test]
fn the_first_child_gets_a_group_in_the_drawn_tree() {
    let mut story = TreeStory::new("first");
    story.tree(&[]);
    story.patch(1);
    story.tree(&[&["One"]]);
    story.run();
}

#[test]
fn a_child_is_inserted_between_two_live_keyed_children() {
    let mut story = TreeStory::new("middle");
    story.tree(&[&["One", "Three"]]);
    story.patch(1);
    story.tree(&[&["One", "Two", "Three"]]);
    story.run();
}

#[test]
fn a_keyed_child_moves_within_its_live_group() {
    let mut story = TreeStory::new("move");
    story.tree(&[&["One", "Two", "Three"]]);
    story.patch(1);
    story.tree(&[&["Three", "One", "Two"]]);
    story.run();
}

#[test]
fn a_childs_name_changes_on_the_same_live_node() {
    let mut story = TreeStory::new("rename");
    story.tree(&[&["One", "Two", "Three"]]);
    story.patch(1);
    story.parts.push_back(Part::Identity { key: "Two", name: "Renamed two" });
    story.tree(&[&["One", "Renamed two", "Three"]]);
    story.steps(vec![harness::Step::Person(Step::Gone {
        find: Find::exactly_named(Role::TreeItem, b"Two"),
        within: SECOND,
    })]);
    story.run();
}

#[test]
fn removing_the_last_child_leaves_no_empty_live_group() {
    let mut story = TreeStory::new("last");
    story.tree(&[&["One"]]);
    story.patch(1);
    story.tree(&[]);
    for name in ["One", "Grandchild"] {
        story.steps(vec![harness::Step::Person(Step::Gone {
            find: Find::exactly_named(Role::TreeItem, name.as_bytes()),
            within: SECOND,
        })]);
    }
    story.run();
}

#[test]
fn a_paragraph_splits_two_live_runs_which_grow_and_merge_in_order() {
    let mut story = TreeStory::new("mixed");
    story.tree(&[&["One", "Three"]]);
    story.patch(1);
    story.tree(&[&["One"], &[], &["Three"]]);
    story.patch(2);
    story.tree(&[&["One", "Two"], &[], &["Three", "Four"]]);
    story.patch(3);
    story.tree(&[&["One", "Two", "Three", "Four"]]);
    story.run();
}

#[test]
fn clicking_the_visible_field_label_focuses_its_named_input() {
    let mut person = Script::new(Box::new([
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::TextField, b"Title"), within: SECOND }),
        harness::Step::Keep {
            selector: b"form[aria-label=Add] > label:first-child".as_slice().into(),
            attribute: Some(b"for".as_slice().into()),
            name: b"caption".as_slice().into(),
        },
        harness::Step::Keep {
            selector: b"form[aria-label=Add] > label:first-child + input:not([aria-label]):not([aria-labelledby])"
                .as_slice()
                .into(),
            attribute: Some(b"id".as_slice().into()),
            name: b"input".as_slice().into(),
        },
        harness::Step::Person(Step::See { find: Find::exactly_named(Role::Text, b"Title"), within: SECOND }),
        harness::Step::Person(Step::Type {
            find: Find::exactly_named(Role::TextField, b"Title"),
            words: b"Draft".as_slice().into(),
        }),
        harness::Step::Key(skein_browser::boundary::Key::Tab),
        harness::Step::Focused(Find::exactly_named(Role::TextArea, b"Body")),
        // The caption is the form's first grid row. Its DOM element owns the text's hit target.
        harness::Step::PressAt {
            find: Find::exactly_named(Role::Form, b"Add"),
            at: skein_browser::boundary::Point { x: 8, y: 8 },
        },
        harness::Step::Focused(Find::exactly_named(Role::TextField, b"Title")),
    ]));
    let mut harness = harness::Harness::new(super::setup());
    assert_eq!(harness.run(&mut [&mut person]), Outcome::Done);
    let kept = harness.kept();
    assert_eq!(kept.len(), 2);
    assert!(!kept[0].value.is_empty());
    assert_eq!(kept[0].value, kept[1].value, "caption's for names this input");
}
