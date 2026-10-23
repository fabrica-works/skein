//! The accessible facts the browser face relies on (ui-web.md, 14.1).
#[path = "support/real.rs"]
pub mod real;

use real::{Profile, Rig, Scenario, chromium};
use skein_ui::{Builder, EntryKind, Limits, Made, NodeId, Role, Run, State};
use skein_ui_web::{
    Look,
    dom::{self, Element},
    mapping::{self, Primary, Words},
};

struct Row {
    role: Role,
    entry: Option<EntryKind>,
    name: &'static str,
}

const ROWS: &[Row] = &[
    Row { role: Role::Page, entry: None, name: "case-page" },
    Row { role: Role::ColumnHeader, entry: None, name: "case-columnheader" },
    Row { role: Role::RowHeader, entry: None, name: "case-rowheader" },
    Row { role: Role::Select, entry: None, name: "case-select" },
    Row { role: Role::Option, entry: None, name: "case-option" },
    Row { role: Role::Banner, entry: None, name: "case-banner" },
    Row { role: Role::Navigation, entry: None, name: "case-navigation" },
    Row { role: Role::Main, entry: None, name: "case-main" },
    Row { role: Role::Complementary, entry: None, name: "case-complementary" },
    Row { role: Role::ContentInfo, entry: None, name: "case-contentinfo" },
    Row { role: Role::Form, entry: None, name: "case-form" },
    Row { role: Role::Search, entry: None, name: "case-search" },
    Row { role: Role::Region, entry: None, name: "case-region" },
    Row { role: Role::Heading { level: 1 }, entry: None, name: "case-heading_level_1" },
    Row { role: Role::Heading { level: 2 }, entry: None, name: "case-heading_level_2" },
    Row { role: Role::Heading { level: 3 }, entry: None, name: "case-heading_level_3" },
    Row { role: Role::Heading { level: 4 }, entry: None, name: "case-heading_level_4" },
    Row { role: Role::Heading { level: 5 }, entry: None, name: "case-heading_level_5" },
    Row { role: Role::Heading { level: 6 }, entry: None, name: "case-heading_level_6" },
    Row { role: Role::Heading { level: 7 }, entry: None, name: "case-heading_level_7" },
    Row { role: Role::Paragraph, entry: None, name: "case-paragraph" },
    Row { role: Role::Text, entry: None, name: "case-text" },
    Row { role: Role::List, entry: None, name: "case-list" },
    Row { role: Role::Item, entry: None, name: "case-item" },
    Row { role: Role::Table, entry: None, name: "case-table" },
    Row { role: Role::Row, entry: None, name: "case-row" },
    Row { role: Role::Cell, entry: None, name: "case-cell" },
    Row { role: Role::Term, entry: None, name: "case-term" },
    Row { role: Role::Description, entry: None, name: "case-description" },
    Row { role: Role::Button, entry: None, name: "case-button" },
    Row { role: Role::Link, entry: None, name: "case-link" },
    Row { role: Role::TextField, entry: None, name: "case-textfield" },
    Row { role: Role::TextArea, entry: None, name: "case-textarea" },
    Row { role: Role::Choice, entry: None, name: "case-choice" },
    Row { role: Role::ChoiceGroup, entry: None, name: "case-choicegroup" },
    Row { role: Role::Check, entry: None, name: "case-check" },
    Row { role: Role::Dialog, entry: None, name: "case-dialog" },
    Row { role: Role::Status, entry: None, name: "case-status" },
    Row { role: Role::Alert, entry: None, name: "case-alert" },
    Row { role: Role::Log, entry: None, name: "case-log" },
    Row { role: Role::Tree, entry: None, name: "case-tree" },
    Row { role: Role::TreeItem, entry: None, name: "case-treeitem" },
    Row { role: Role::Tabs, entry: None, name: "case-tabs" },
    Row { role: Role::Tab, entry: None, name: "case-tab" },
    Row { role: Role::Progress, entry: None, name: "case-progress" },
    Row { role: Role::Time, entry: None, name: "case-time" },
    Row { role: Role::Code, entry: None, name: "case-code" },
    Row { role: Role::Quote, entry: None, name: "case-quote" },
    Row { role: Role::Separator, entry: None, name: "case-separator" },
    Row { role: Role::Menu, entry: None, name: "case-menu" },
    Row { role: Role::MenuItem, entry: None, name: "case-menuitem" },
    Row { role: Role::Disclosure, entry: None, name: "case-disclosure" },
    Row { role: Role::Image, entry: None, name: "case-image" },
    Row { role: Role::Figure, entry: None, name: "case-figure" },
    Row { role: Role::Slot, entry: None, name: "case-slot" },
    Row { role: Role::Drawing, entry: None, name: "case-drawing" },
    Row { role: Role::TextField, entry: Some(EntryKind::Text), name: "case-entry_text" },
    Row { role: Role::TextField, entry: Some(EntryKind::Number), name: "case-entry_number" },
    Row { role: Role::TextField, entry: Some(EntryKind::Email), name: "case-entry_email" },
    Row { role: Role::TextField, entry: Some(EntryKind::Telephone), name: "case-entry_telephone" },
    Row { role: Role::TextField, entry: Some(EntryKind::Url), name: "case-entry_url" },
    Row { role: Role::TextField, entry: Some(EntryKind::Date), name: "case-entry_date" },
    Row { role: Role::TextField, entry: Some(EntryKind::Time), name: "case-entry_time" },
    Row { role: Role::TextField, entry: Some(EntryKind::Secret), name: "case-entry_secret" },
];
const LIMITS: Limits = Limits {
    nodes: 4,
    depth: 4,
    text_per_node: 128,
    text_bytes: 512,
    drawing_bytes: 0,
    styles_per_node: 0,
    patches: 8,
    bindings: 0,
    slots: 0,
    markdown_depth: 0,
    scene_marks: 0,
    scene_bytes: 0,
};

fn no_drawing(_: skein_ui::DrawingKind, _: &[u8], _: u16, _: u16, _: &mut skein_ui::scene::Scene) -> bool {
    false
}
fn mapped(row: &Row, id: NodeId, description: Option<&[u8]>) -> Element {
    let mut builder = Builder::new(&LIMITS);
    builder.open(row.role);
    builder.name(row.name.as_bytes());
    if let Some(description) = description {
        builder.description(description);
    }
    if let Some(entry) = row.entry {
        builder.entry(entry);
    }
    if row.role == Role::Text {
        builder.run(Run::Plain, row.name.as_bytes());
    }
    builder.close();
    let tree = builder.finish();
    let mut made = Made::of(tree.get(0).expect("row root"));
    made.id = id;
    let look = Look { components: &[], names: &[], pictures: &[], draw: no_drawing, elements: &[] };
    let mut element = mapping::element(&made, &look, b"/", Primary::Control, &Words::ENGLISH);
    if row.role == Role::Menu {
        for child in &mut element.children {
            if let dom::Child::Element(child) = child {
                child.remove("hidden");
            }
        }
    }
    if row.role == Role::Dialog {
        element.set("open", "");
    }
    element
}
// The completion list, its item and text, and the placeholder field reserve four ids.
const COMPLETED_AND_PLACEHOLDER_NODES: u32 = 4;

fn fixed_page() -> Vec<u8> {
    let mut body = String::from(
        "<!doctype html><html><body><select aria-label=Pick><option>First</option><option>Second</option><option>Third</option></select>",
    );
    for (number, row) in (0..).zip(ROWS) {
        let html = dom::print(&mapped(row, NodeId(number), None));
        let html = match row.role {
            Role::Row => format!("<table>{html}</table>"),
            Role::Cell | Role::ColumnHeader | Role::RowHeader => format!("<table><tr>{html}</tr></table>"),
            Role::Option => format!("<select size=2>{html}</select>"),
            _ => html,
        };
        body.push_str(&html);
    }
    let described = Row { role: Role::Disclosure, entry: None, name: "case-disclosure-named" };
    let id = NodeId(u32::try_from(ROWS.len()).expect("fixed role rows fit u32") + COMPLETED_AND_PLACEHOLDER_NODES);
    body.push_str(&dom::print(&mapped(&described, id, Some(b"case-disclosure-described"))));
    body.push_str("<style>");
    body.push_str(include_str!("../../../crates/skein-ui-bundle/skein-ui.css"));
    body.push_str("</style>");
    body.push_str(&dom::print(&completed_and_placeholder()));
    body.push_str("</body></html>");
    body.into_bytes()
}
fn snapshot(scenario: Scenario) -> String {
    let chromium = chromium();
    let profile = Profile::fresh();
    let mut rig = Rig::new(&chromium, &profile.0, scenario);
    rig.fixed_page(fixed_page());
    rig.run();
    assert_eq!(rig.choice_failure, None, "the choice succeeds");
    String::from_utf8(rig.snapshot.into_vec()).expect("UTF-8 snapshot")
}

#[test]
fn chromium_computes_each_roles_accessible_role() {
    let snapshot = snapshot(Scenario::Facts);
    let mut missing = Vec::new();
    for row in ROWS {
        let roles = mapping::accessible(row.role);
        let found = snapshot.lines().any(|line| {
            roles.iter().any(|role| {
                let role = std::str::from_utf8(role).expect("role words");
                line.starts_with(&format!("{role} ")) && line.contains(row.name)
            })
        });
        if !found {
            missing.push(row.name);
        }
    }
    assert_eq!(missing, [] as [&str; 0], "snapshot:\n{snapshot}");
    assert!(
        snapshot
            .lines()
            .any(|line| line.trim_end() == "DisclosureTriangle case-disclosure-named case-disclosure-described"),
        "snapshot:\n{snapshot}"
    );
}

#[test]
fn chromium_chooses_an_option_by_the_kits_keys() {
    let snapshot = snapshot(Scenario::Choose { name: b"Second" });
    let selected = snapshot.lines().find(|line| line.starts_with("combobox Pick "));
    assert_eq!(selected, Some("combobox Pick Second"), "snapshot:\n{snapshot}");
}

#[test]
fn chromium_reports_both_labels_when_type_ahead_chooses_a_shared_prefix() {
    let chromium = chromium();
    let profile = Profile::fresh();
    let mut rig = Rig::new(&chromium, &profile.0, Scenario::Choose { name: b"task" });
    rig.fixed_page(br#"<!doctype html><html><body><select aria-label="Pick"><option>idea</option><option>tasks</option><option>task</option></select></body></html>"#.to_vec());
    rig.run();
    assert_eq!(rig.choice_failure.as_deref(), Some("type-ahead requested option 'task', but selected 'tasks'"));
    let snapshot = String::from_utf8(rig.snapshot.into_vec()).expect("UTF-8 snapshot");
    assert!(snapshot.lines().any(|line| line == "combobox Pick tasks"), "snapshot:\n{snapshot}");
}

#[test]
fn chromium_presses_a_select_inside_its_shadow_tree() {
    let chromium = chromium();
    let profile = Profile::fresh();
    let mut rig = Rig::new(&chromium, &profile.0, Scenario::Select);
    rig.fixed_page(br#"<!doctype html><html><body><select aria-label="Pick" style="width:512px"><option>First</option><option>Second</option></select></body></html>"#.to_vec());
    rig.run();
}

const NATIVE_VALUES: &[&str] = &["Date case-entry_date 2026-10-15", "InputTime case-entry_time 14:30"];
#[test]
fn chromium_takes_a_date_and_a_time_by_the_kits_keys() {
    let snapshot = snapshot(Scenario::Dates);
    for expected in NATIVE_VALUES {
        assert!(
            snapshot.lines().any(|line| line.trim().starts_with(expected)),
            "native field {expected:?}, snapshot:\n{snapshot}"
        );
    }
}

fn completed_and_placeholder() -> Element {
    let mut b = Builder::new(&LIMITS);
    b.open(Role::List);
    b.open(Role::Item);
    b.state(State::Completed);
    b.text(Run::Plain, b"Branch pushed");
    b.close();
    b.close();
    let tree = b.finish();
    let look = Look { components: &[], names: &[], pictures: &[], draw: no_drawing, elements: &[] };
    let mapped = |node: &skein_ui::Node, offset: u32| {
        let mut made = Made::of(node);
        // The fixed role rows reserve ids below ROWS.len().
        made.id = NodeId(u32::try_from(ROWS.len()).expect("fixed role rows fit u32") + offset);
        mapping::element(&made, &look, b"/", Primary::Control, &Words::ENGLISH)
    };
    let mut list = mapped(tree.get(0).expect("list"), 0);
    let mut item = mapped(tree.get(1).expect("item"), 1);
    item.children.insert(0, dom::Child::Element(mapped(tree.get(2).expect("text"), 2)));
    list.append(item);
    let mut b = Builder::new(&LIMITS);
    b.open(Role::TextArea);
    b.name(b"Message");
    b.placeholder("Message T6…".as_bytes());
    b.close();
    let tree = b.finish();
    let mut page = Element::new("");
    page.append(list);
    page.append(mapped(tree.get(0).expect("text area"), 3));
    page
}

#[test]
fn chromium_reads_completion_and_keeps_a_placeholders_field_name() {
    let chromium = chromium();
    let profile = Profile::fresh();
    let mut rig = Rig::new(&chromium, &profile.0, Scenario::Facts);
    rig.fixed_page(fixed_page());
    rig.capture_tree();
    rig.run();
    let tree = rig.tree.expect("accessibility tree requested");
    let field = tree
        .iter()
        .find(|entry| entry.role.as_ref() == b"textbox" && entry.name.as_ref() == b"Message")
        .expect("the placeholder leaves the field named exactly Message");
    assert!(field.multiline);
    let entries = tree.as_slice();
    let item = entries
        .iter()
        .position(|entry| entry.role.as_ref() == b"listitem" && entry.name.is_empty())
        .expect("the completed list item");
    let depth = entries[item].depth;
    let words: Vec<_> = entries
        .iter()
        .skip(item + 1)
        .take_while(|entry| entry.depth > depth)
        .filter(|entry| entry.role.as_ref() == b"StaticText")
        .map(|entry| entry.name.as_ref())
        .collect();
    assert_eq!(words, [b"Branch pushed".as_slice(), b"completed"], "only non-ignored words are read in item order");
}
