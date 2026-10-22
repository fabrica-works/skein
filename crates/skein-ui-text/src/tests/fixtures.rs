use skein_ui::{Binding, Builder, Limits, NodeId, Owner, Region, Role, Run, State, Tree};

use skein_lib::{Queue, Token};

pub const LIMITS: Limits = Limits {
    nodes: 160,
    depth: 12,
    text_per_node: 128,
    text_bytes: 2048,
    drawing_bytes: 8,
    styles_per_node: 2,
    patches: 960,
    bindings: 32,
    slots: 2,
    markdown_depth: 2,
    scene_marks: 16,
    scene_bytes: 256,
};
pub const STATUS: Region = Region { owner: Owner { number: 1 }, number: 1 };

pub fn binding() -> Binding {
    Binding { owner: Owner { number: 1 }, token: Token::new(1) }
}
pub fn page(main: fn(&mut Builder)) -> Tree {
    page_with_limits(main, &LIMITS)
}
pub fn page_with_limits(main: fn(&mut Builder), limits: &Limits) -> Tree {
    let mut b = Builder::new(limits);
    b.open(Role::Page);
    b.open(Role::Banner);
    b.close();
    b.open(Role::Main);
    main(&mut b);
    b.close();
    b.close();
    let mut tree = b.finish();
    let mut patches = Queue::with_capacity(limits.patches);
    skein_ui::diff(&Tree::new(limits.nodes), &mut tree, limits, &mut 1, &mut patches);
    tree
}
pub fn button(b: &mut Builder, name: &[u8], enabled: bool) {
    b.open(Role::Button);
    b.name(name);
    b.bind(binding());
    if !enabled {
        b.state(State::Disabled);
    }
    b.close();
}
pub fn leaf(b: &mut Builder, role: Role, text: &[u8]) {
    b.open(role);
    b.text(Run::Plain, text);
    b.close();
}
pub fn roles(b: &mut Builder) {
    groups(b);
    bodies(b);
    controls(b);
    select(b);
    remaining(b);
    nested_item(b);
    completed_items(b);
}
fn groups(b: &mut Builder) {
    leaf(b, Role::Heading { level: 1 }, b"Every role");
    for (role, name) in [
        (Role::Banner, b"Banner".as_slice()),
        (Role::Navigation, b"Navigation"),
        (Role::Main, b"Main"),
        (Role::Complementary, b"Aside"),
        (Role::ContentInfo, b"Footer"),
        (Role::Form, b"Form"),
        (Role::Search, b"Search"),
        (Role::Region, b"Region"),
        (Role::Figure, b"Figure"),
        (Role::Dialog, b"Dialog"),
        (Role::Disclosure, b"Disclosure"),
        (Role::List, b"List"),
        (Role::Tree, b"Tree"),
        (Role::Menu, b"Menu"),
        (Role::ChoiceGroup, b"Choices"),
        (Role::Tabs, b"Tabs"),
        (Role::Log, b"Log"),
    ] {
        b.open(role);
        b.name(name);
        leaf(b, Role::Paragraph, b"child");
        b.close();
    }
}
fn bodies(b: &mut Builder) {
    leaf(b, Role::Heading { level: 2 }, b"Details");
    b.open(Role::Paragraph);
    b.open(Role::Text);
    for (run, text) in [
        (Run::Plain, b"plain ".as_slice()),
        (Run::Emphasis, b"emphasis "),
        (Run::Strong, b"strong "),
        (Run::Code, b"code"),
        (Run::Link { to: Token::new(1) }, b" link"),
    ] {
        b.run(run, text);
    }
    b.close();
    b.close();
    b.open(Role::List);
    b.open(Role::Item);
    leaf(b, Role::Paragraph, b"first\ncontinued");
    leaf(b, Role::Paragraph, b"second");
    b.close();
    leaf(b, Role::TreeItem, b"tree item");
    b.close();
    b.open(Role::Table);
    b.open(Role::Row);
    b.open(Role::ColumnHeader);
    b.sort(skein_ui::Sort::Ascending);
    b.text(Run::Plain, b"Name");
    b.close();
    leaf(b, Role::ColumnHeader, b"Value");
    b.close();
    b.open(Role::Row);
    leaf(b, Role::RowHeader, b"Row");
    leaf(b, Role::Cell, b"left");
    leaf(b, Role::Cell, b"right");
    b.close();
    b.close();
    leaf(b, Role::Cell, b"outside");
    leaf(b, Role::ColumnHeader, b"Outside header");
    leaf(b, Role::Term, b"Term");
    b.open(Role::Description);
    b.text(Run::Plain, b"Description");
    b.text(Run::Plain, b"Another line");
    b.close();
}
fn controls(b: &mut Builder) {
    for role in [Role::Button, Role::Tab, Role::MenuItem, Role::Link] {
        b.open(role);
        b.name(b"Act");
        if role == Role::Link {
            b.link(binding());
        } else {
            b.bind(binding());
        }
        b.close();
    }
    button(b, b"Unavailable", false);
    b.open(Role::Button);
    b.name(b"Unbound");
    b.close();
    b.open(Role::Link);
    b.external(b"https://example.test/");
    b.text(Run::Plain, b"external");
    b.close();
    for role in [Role::TextField, Role::TextArea] {
        b.open(role);
        b.name(b"Message");
        b.bind(binding());
        if role == Role::TextField {
            b.value(b"hello", 1);
            b.state(State::Required);
            b.state(State::SubmitsOnEnter);
        } else {
            b.placeholder("Message T6…".as_bytes());
        }
        b.close();
    }
    for role in [Role::Choice, Role::Check] {
        for selected in [false, true] {
            b.open(role);
            b.name(b"Option");
            b.bind(binding());
            if selected {
                b.state(State::Selected);
                b.state(State::Pressed);
            }
            b.close();
        }
    }
}
fn select(b: &mut Builder) {
    label_select(b, false);
}

pub const SECOND_OPTION: NodeId = NodeId(7); // page (2), banner, main, select, one, two
pub fn label_select(b: &mut Builder, disabled: bool) {
    b.open(Role::Select);
    b.name(b"Select");
    if disabled {
        b.state(State::Disabled);
    }
    for (name, selected) in [(b"One".as_slice(), false), (b"Two", true), (b"Three", false)] {
        b.open(Role::Option);
        b.name(name);
        b.bind(binding());
        if selected {
            b.state(State::Selected);
        }
        b.close();
    }
    b.close();
}
fn remaining(b: &mut Builder) {
    leaf(b, Role::Status, b"Ready");
    b.open(Role::Status);
    b.name(b"Loading");
    b.close();
    leaf(b, Role::Alert, b"Warning");
    b.open(Role::Progress);
    b.name(b"Progress");
    b.text(Run::Plain, b"half");
    b.close();
    leaf(b, Role::Time, b"today");
    leaf(b, Role::Code, b"one\ntwo");
    leaf(b, Role::Quote, b"one\ntwo");
    b.open(Role::Separator);
    b.close();
    b.open(Role::Slot);
    b.text(Run::Plain, b"fragment");
    b.close();
    b.drawing(skein_ui::DrawingKind { owner: Owner { number: 1 }, kind: 1 }, b"Drawing", b"", 8);
    b.text(Run::Plain, b"fallback");
    b.close();
    b.open(Role::Image);
    b.name(b"Picture");
    b.picture(skein_ui::Picture { owner: Owner { number: 1 }, number: 2 });
    b.close();
    b.open(Role::Button);
    b.name(b"States");
    b.bind(binding());
    for state in
        [State::Selected, State::Pressed, State::Invalid, State::Current, State::Busy, State::Expanded, State::Disabled]
    {
        b.state(state);
    }
    b.style(skein_ui::Style { owner: Owner { number: 1 }, token: 999 });
    b.shortcut(skein_ui::Shortcut { key: b's', modifier: skein_ui::Modifier::Alt });
    b.close();
}

pub fn nested_item(b: &mut Builder) {
    b.open(Role::Item);
    b.open(Role::Region);
    b.name(b"Card");
    button(b, b"Nested", true);
    b.close();
    b.close();
}

pub fn properties(b: &mut Builder) {
    b.description(b"Main description");
    b.open(Role::Region);
    b.name(b"Fields");
    b.description(b"Field descriptions");
    property_fields(b);
    b.close();
    b.open(Role::Heading { level: 2 });
    b.description(b"Heading description");
    b.open(Role::Text);
    b.text(Run::Plain, b"Collections");
    b.description(b"Heading text description");
    b.close();
    b.close();
    property_collections(b);
}
fn property_fields(b: &mut Builder) {
    use skein_ui::EntryKind;
    for (kind, name, value) in [
        (EntryKind::Text, b"Text".as_slice(), b"hello".as_slice()),
        (EntryKind::Number, b"Number", b"42"),
        (EntryKind::Email, b"Email", b"person@example.test"),
        (EntryKind::Telephone, b"Telephone", b"+123"),
        (EntryKind::Url, b"URL", b"https://example.test/"),
        (EntryKind::Date, b"Date", b"2026-10-15"),
        (EntryKind::Time, b"Time", b"13:00"),
        (EntryKind::Secret, b"Secret", b"private-token"),
        (EntryKind::Secret, b"Empty secret", b""),
    ] {
        b.open(Role::TextField);
        b.name(name);
        b.entry(kind);
        b.value(value, 1);
        b.bind(binding());
        if kind == EntryKind::Text {
            b.description(b"Text field description");
        }
        if kind == EntryKind::Email {
            b.state(State::Invalid);
        }
        b.close();
    }
}
fn property_collections(b: &mut Builder) {
    b.open(Role::List);
    b.name(b"Results");
    b.count(5);
    b.open(Role::Item);
    b.position(2);
    b.description(b"Item description");
    b.open(Role::Paragraph);
    b.description(b"Paragraph description");
    b.open(Role::Text);
    b.text(Run::Plain, b"first\ncontinued");
    b.description(b"Inline description");
    b.close();
    b.close();
    leaf(b, Role::Paragraph, b"second paragraph");
    b.close();
    b.open(Role::Item);
    b.position(5);
    b.description(b"Card item description");
    b.open(Role::Region);
    b.name(b"Card");
    b.description(b"Card description");
    leaf(b, Role::Paragraph, b"child");
    b.close();
    b.close();
    b.open(Role::List);
    b.open(Role::Item);
    b.position(3);
    b.text(Run::Plain, b"Nested item");
    b.close();
    b.close();
    b.close();
    b.open(Role::Table);
    b.count(9);
    b.description(b"Table description");
    b.open(Role::Row);
    b.position(4);
    b.state(State::Selected);
    b.description(b"Row description");
    b.open(Role::Cell);
    b.text(Run::Plain, b"left");
    b.description(b"Left cell description");
    b.close();
    leaf(b, Role::Cell, b"right");
    b.close();
    b.close();
    b.open(Role::Tree);
    b.open(Role::TreeItem);
    b.position(7);
    b.text(Run::Plain, b"Tree item");
    b.close();
    b.close();
}

pub const EVERY_LIMITS: Limits = Limits { nodes: 256, text_bytes: 4096, bindings: 64, ..LIMITS };
pub fn every_node(b: &mut Builder) {
    roles(b);
    properties(b);
    b.open(Role::TextField);
    b.name(b"Required");
    b.value(b"present", 1);
    b.state(State::Required);
    b.bind(binding());
    b.focus(1);
    b.close();
    b.open(Role::Table);
    b.open(Role::Row);
    b.open(Role::ColumnHeader);
    b.sort(skein_ui::Sort::Descending);
    b.spans(skein_ui::Spans { rows: 2, columns: 3 });
    b.text(Run::Plain, b"Descending");
    b.close();
    b.open(Role::Cell);
    b.spans(skein_ui::Spans { rows: 3, columns: 4 });
    b.text(Run::Plain, b"spanning");
    b.close();
    b.close();
    b.close();
    b.open(Role::Image);
    b.name(b"Remote picture");
    b.image_address(b"https://example.test/picture.png");
    b.close();
    b.open(Role::Dialog);
    b.name(b"Dismiss");
    b.open(Role::Button);
    b.name(b"Close");
    b.bind(binding());
    b.shortcut(skein_ui::Shortcut::ESCAPE);
    b.close();
    b.close();
}
pub fn first_lines(b: &mut Builder) {
    leaf(b, Role::Heading { level: 1 }, b"Inbox");
    leaf(b, Role::Paragraph, b"No status region");
}
pub fn control_bytes(b: &mut Builder) {
    b.open(Role::Region);
    b.name(b"Name\x1b");
    b.description(b"Description\x00");
    leaf(b, Role::Paragraph, b"Run\x7f");
    b.open(Role::TextField);
    b.name(b"Field");
    b.value(b"Value\xc2\x80", 1);
    b.description(b"Detail\xff");
    b.bind(binding());
    b.close();
    b.close();
}

fn completed_items(b: &mut Builder) {
    b.open(Role::List);
    b.count(4);
    for child in [false, true] {
        b.open(Role::Item);
        b.position(1);
        b.state(State::Completed);
        if child {
            b.text(Run::Plain, b"Branch pushed");
        } else {
            b.name(b"Branch pushed");
        }
        b.close();
    }
    b.close();
    b.open(Role::Tree);
    b.open(Role::TreeItem);
    b.state(State::Expanded);
    b.text(Run::Plain, b"Expanded branch");
    b.close();
    b.close();
}
