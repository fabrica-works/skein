//! The accessible facts the browser face relies on (ui-web.md, 14.1).
#[path = "support/real.rs"]
pub mod real;

use real::{Profile, Rig, Scenario, chromium};
use skein_ui::{Builder, EntryKind, Limits, Made, NodeId, Role, Run};
use skein_ui_web::{
    Look,
    dom::{self, Element},
    mapping::{self, Primary},
};

struct Row {
    id: NodeId,
    role: Role,
    entry: Option<EntryKind>,
    name: &'static str,
}

const PAGE: NodeId = NodeId(0); // fixed page, page row
const COLUMNHEADER: NodeId = NodeId(1); // fixed page, columnheader row
const ROWHEADER: NodeId = NodeId(2); // fixed page, rowheader row
const SELECT: NodeId = NodeId(3); // fixed page, select row
const OPTION: NodeId = NodeId(4); // fixed page, option row
const BANNER: NodeId = NodeId(5); // fixed page, banner row
const NAVIGATION: NodeId = NodeId(6); // fixed page, navigation row
const MAIN: NodeId = NodeId(7); // fixed page, main row
const COMPLEMENTARY: NodeId = NodeId(8); // fixed page, complementary row
const CONTENTINFO: NodeId = NodeId(9); // fixed page, contentinfo row
const FORM: NodeId = NodeId(10); // fixed page, form row
const SEARCH: NodeId = NodeId(11); // fixed page, search row
const REGION: NodeId = NodeId(12); // fixed page, region row
const HEADING_LEVEL_1: NodeId = NodeId(13); // fixed page, heading_level_1 row
const HEADING_LEVEL_2: NodeId = NodeId(14); // fixed page, heading_level_2 row
const HEADING_LEVEL_3: NodeId = NodeId(15); // fixed page, heading_level_3 row
const HEADING_LEVEL_4: NodeId = NodeId(16); // fixed page, heading_level_4 row
const HEADING_LEVEL_5: NodeId = NodeId(17); // fixed page, heading_level_5 row
const HEADING_LEVEL_6: NodeId = NodeId(18); // fixed page, heading_level_6 row
const HEADING_LEVEL_7: NodeId = NodeId(19); // fixed page, heading_level_7 row
const PARAGRAPH: NodeId = NodeId(20); // fixed page, paragraph row
const TEXT: NodeId = NodeId(21); // fixed page, text row
const LIST: NodeId = NodeId(22); // fixed page, list row
const ITEM: NodeId = NodeId(23); // fixed page, item row
const TABLE: NodeId = NodeId(24); // fixed page, table row
const ROW: NodeId = NodeId(25); // fixed page, row row
const CELL: NodeId = NodeId(26); // fixed page, cell row
const TERM: NodeId = NodeId(27); // fixed page, term row
const DESCRIPTION: NodeId = NodeId(28); // fixed page, description row
const BUTTON: NodeId = NodeId(29); // fixed page, button row
const LINK: NodeId = NodeId(30); // fixed page, link row
const TEXTFIELD: NodeId = NodeId(31); // fixed page, textfield row
const TEXTAREA: NodeId = NodeId(32); // fixed page, textarea row
const CHOICE: NodeId = NodeId(33); // fixed page, choice row
const CHOICEGROUP: NodeId = NodeId(34); // fixed page, choicegroup row
const CHECK: NodeId = NodeId(35); // fixed page, check row
const DIALOG: NodeId = NodeId(36); // fixed page, dialog row
const STATUS: NodeId = NodeId(37); // fixed page, status row
const ALERT: NodeId = NodeId(38); // fixed page, alert row
const LOG: NodeId = NodeId(39); // fixed page, log row
const TREE: NodeId = NodeId(40); // fixed page, tree row
const TREEITEM: NodeId = NodeId(41); // fixed page, treeitem row
const TABS: NodeId = NodeId(42); // fixed page, tabs row
const TAB: NodeId = NodeId(43); // fixed page, tab row
const PROGRESS: NodeId = NodeId(44); // fixed page, progress row
const TIME: NodeId = NodeId(45); // fixed page, time row
const CODE: NodeId = NodeId(46); // fixed page, code row
const QUOTE: NodeId = NodeId(47); // fixed page, quote row
const SEPARATOR: NodeId = NodeId(48); // fixed page, separator row
const MENU: NodeId = NodeId(49); // fixed page, menu row
const MENUITEM: NodeId = NodeId(50); // fixed page, menuitem row
const DISCLOSURE: NodeId = NodeId(51); // fixed page, disclosure row
const IMAGE: NodeId = NodeId(52); // fixed page, image row
const FIGURE: NodeId = NodeId(53); // fixed page, figure row
const SLOT: NodeId = NodeId(54); // fixed page, slot row
const DRAWING: NodeId = NodeId(55); // fixed page, drawing row
const ENTRY_TEXT: NodeId = NodeId(56); // fixed page, entry_text row
const ENTRY_NUMBER: NodeId = NodeId(57); // fixed page, entry_number row
const ENTRY_EMAIL: NodeId = NodeId(58); // fixed page, entry_email row
const ENTRY_TELEPHONE: NodeId = NodeId(59); // fixed page, entry_telephone row
const ENTRY_URL: NodeId = NodeId(60); // fixed page, entry_url row
const ENTRY_DATE: NodeId = NodeId(61); // fixed page, entry_date row
const ENTRY_TIME: NodeId = NodeId(62); // fixed page, entry_time row
const ENTRY_SECRET: NodeId = NodeId(63); // fixed page, entry_secret row

const ROWS: &[Row] = &[
    Row { id: PAGE, role: Role::Page, entry: None, name: "case-page" },
    Row { id: COLUMNHEADER, role: Role::ColumnHeader, entry: None, name: "case-columnheader" },
    Row { id: ROWHEADER, role: Role::RowHeader, entry: None, name: "case-rowheader" },
    Row { id: SELECT, role: Role::Select, entry: None, name: "case-select" },
    Row { id: OPTION, role: Role::Option, entry: None, name: "case-option" },
    Row { id: BANNER, role: Role::Banner, entry: None, name: "case-banner" },
    Row { id: NAVIGATION, role: Role::Navigation, entry: None, name: "case-navigation" },
    Row { id: MAIN, role: Role::Main, entry: None, name: "case-main" },
    Row { id: COMPLEMENTARY, role: Role::Complementary, entry: None, name: "case-complementary" },
    Row { id: CONTENTINFO, role: Role::ContentInfo, entry: None, name: "case-contentinfo" },
    Row { id: FORM, role: Role::Form, entry: None, name: "case-form" },
    Row { id: SEARCH, role: Role::Search, entry: None, name: "case-search" },
    Row { id: REGION, role: Role::Region, entry: None, name: "case-region" },
    Row { id: HEADING_LEVEL_1, role: Role::Heading { level: 1 }, entry: None, name: "case-heading_level_1" },
    Row { id: HEADING_LEVEL_2, role: Role::Heading { level: 2 }, entry: None, name: "case-heading_level_2" },
    Row { id: HEADING_LEVEL_3, role: Role::Heading { level: 3 }, entry: None, name: "case-heading_level_3" },
    Row { id: HEADING_LEVEL_4, role: Role::Heading { level: 4 }, entry: None, name: "case-heading_level_4" },
    Row { id: HEADING_LEVEL_5, role: Role::Heading { level: 5 }, entry: None, name: "case-heading_level_5" },
    Row { id: HEADING_LEVEL_6, role: Role::Heading { level: 6 }, entry: None, name: "case-heading_level_6" },
    Row { id: HEADING_LEVEL_7, role: Role::Heading { level: 7 }, entry: None, name: "case-heading_level_7" },
    Row { id: PARAGRAPH, role: Role::Paragraph, entry: None, name: "case-paragraph" },
    Row { id: TEXT, role: Role::Text, entry: None, name: "case-text" },
    Row { id: LIST, role: Role::List, entry: None, name: "case-list" },
    Row { id: ITEM, role: Role::Item, entry: None, name: "case-item" },
    Row { id: TABLE, role: Role::Table, entry: None, name: "case-table" },
    Row { id: ROW, role: Role::Row, entry: None, name: "case-row" },
    Row { id: CELL, role: Role::Cell, entry: None, name: "case-cell" },
    Row { id: TERM, role: Role::Term, entry: None, name: "case-term" },
    Row { id: DESCRIPTION, role: Role::Description, entry: None, name: "case-description" },
    Row { id: BUTTON, role: Role::Button, entry: None, name: "case-button" },
    Row { id: LINK, role: Role::Link, entry: None, name: "case-link" },
    Row { id: TEXTFIELD, role: Role::TextField, entry: None, name: "case-textfield" },
    Row { id: TEXTAREA, role: Role::TextArea, entry: None, name: "case-textarea" },
    Row { id: CHOICE, role: Role::Choice, entry: None, name: "case-choice" },
    Row { id: CHOICEGROUP, role: Role::ChoiceGroup, entry: None, name: "case-choicegroup" },
    Row { id: CHECK, role: Role::Check, entry: None, name: "case-check" },
    Row { id: DIALOG, role: Role::Dialog, entry: None, name: "case-dialog" },
    Row { id: STATUS, role: Role::Status, entry: None, name: "case-status" },
    Row { id: ALERT, role: Role::Alert, entry: None, name: "case-alert" },
    Row { id: LOG, role: Role::Log, entry: None, name: "case-log" },
    Row { id: TREE, role: Role::Tree, entry: None, name: "case-tree" },
    Row { id: TREEITEM, role: Role::TreeItem, entry: None, name: "case-treeitem" },
    Row { id: TABS, role: Role::Tabs, entry: None, name: "case-tabs" },
    Row { id: TAB, role: Role::Tab, entry: None, name: "case-tab" },
    Row { id: PROGRESS, role: Role::Progress, entry: None, name: "case-progress" },
    Row { id: TIME, role: Role::Time, entry: None, name: "case-time" },
    Row { id: CODE, role: Role::Code, entry: None, name: "case-code" },
    Row { id: QUOTE, role: Role::Quote, entry: None, name: "case-quote" },
    Row { id: SEPARATOR, role: Role::Separator, entry: None, name: "case-separator" },
    Row { id: MENU, role: Role::Menu, entry: None, name: "case-menu" },
    Row { id: MENUITEM, role: Role::MenuItem, entry: None, name: "case-menuitem" },
    Row { id: DISCLOSURE, role: Role::Disclosure, entry: None, name: "case-disclosure" },
    Row { id: IMAGE, role: Role::Image, entry: None, name: "case-image" },
    Row { id: FIGURE, role: Role::Figure, entry: None, name: "case-figure" },
    Row { id: SLOT, role: Role::Slot, entry: None, name: "case-slot" },
    Row { id: DRAWING, role: Role::Drawing, entry: None, name: "case-drawing" },
    Row { id: ENTRY_TEXT, role: Role::TextField, entry: Some(EntryKind::Text), name: "case-entry_text" },
    Row { id: ENTRY_NUMBER, role: Role::TextField, entry: Some(EntryKind::Number), name: "case-entry_number" },
    Row { id: ENTRY_EMAIL, role: Role::TextField, entry: Some(EntryKind::Email), name: "case-entry_email" },
    Row { id: ENTRY_TELEPHONE, role: Role::TextField, entry: Some(EntryKind::Telephone), name: "case-entry_telephone" },
    Row { id: ENTRY_URL, role: Role::TextField, entry: Some(EntryKind::Url), name: "case-entry_url" },
    Row { id: ENTRY_DATE, role: Role::TextField, entry: Some(EntryKind::Date), name: "case-entry_date" },
    Row { id: ENTRY_TIME, role: Role::TextField, entry: Some(EntryKind::Time), name: "case-entry_time" },
    Row { id: ENTRY_SECRET, role: Role::TextField, entry: Some(EntryKind::Secret), name: "case-entry_secret" },
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
fn mapped(row: &Row) -> Element {
    let mut builder = Builder::new(&LIMITS);
    builder.open(row.role);
    builder.name(row.name.as_bytes());
    if let Some(entry) = row.entry {
        builder.entry(entry);
    }
    if row.role == Role::Text {
        builder.run(Run::Plain, row.name.as_bytes());
    }
    builder.close();
    let tree = builder.finish();
    let mut made = Made::of(tree.get(0).expect("row root"));
    made.id = row.id;
    let look = Look { names: &[], pictures: &[], draw: no_drawing };
    let mut element = mapping::element(&made, &look, b"/", Primary::Control);
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
fn fixed_page() -> Vec<u8> {
    let mut body = String::from(
        "<!doctype html><html><body><select aria-label=Pick><option>First</option><option>Second</option><option>Third</option></select>",
    );
    for row in ROWS {
        let html = dom::print(&mapped(row));
        let html = match row.role {
            Role::Row => format!("<table>{html}</table>"),
            Role::Cell | Role::ColumnHeader | Role::RowHeader => format!("<table><tr>{html}</tr></table>"),
            Role::Option => format!("<select size=2>{html}</select>"),
            _ => html,
        };
        body.push_str(&html);
    }
    body.push_str("</body></html>");
    body.into_bytes()
}
fn snapshot(scenario: Scenario) -> String {
    let chromium = chromium();
    let profile = Profile::fresh();
    let mut rig = Rig::new(&chromium, &profile.0, scenario);
    rig.fixed_page(fixed_page());
    rig.run();
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
}

#[test]
fn chromium_chooses_an_option_by_the_kits_keys() {
    let snapshot = snapshot(Scenario::Choose);
    let selected = snapshot.lines().find(|line| line.starts_with("combobox Pick "));
    assert_eq!(selected, Some("combobox Pick Second"), "snapshot:\n{snapshot}");
}
