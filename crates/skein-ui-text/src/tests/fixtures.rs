use skein_ui::{Binding, Builder, Limits, Owner, Region, Role, Run, State, Tree};

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
};
pub const STATUS: Region = Region { owner: Owner { number: 1 }, number: 1 };

pub fn binding() -> Binding {
    Binding { owner: Owner { number: 1 }, token: Token::new(1) }
}
pub fn page(main: fn(&mut Builder)) -> Tree {
    let mut b = Builder::new(&LIMITS);
    b.open(Role::Page);
    b.open(Role::Banner);
    b.close();
    b.open(Role::Main);
    main(&mut b);
    b.close();
    b.close();
    let mut tree = b.finish();
    let mut patches = Queue::with_capacity(LIMITS.patches);
    skein_ui::diff(&Tree::new(LIMITS.nodes), &mut tree, &LIMITS, &mut 1, &mut patches);
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
    remaining(b);
    nested_item(b);
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
    leaf(b, Role::Cell, b"left");
    leaf(b, Role::Cell, b"right");
    b.close();
    b.close();
    leaf(b, Role::Cell, b"outside");
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
            b.state(State::SubmitsOnEnter);
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
