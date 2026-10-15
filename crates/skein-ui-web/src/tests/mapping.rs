use super::fixtures::{self, OWNER};
use crate::{
    dom::{self, Element},
    mapping::{self, Primary},
};
use skein_ui::{Made, Modifier, Picture, Region, Role, Run, Shortcut, State, Style, TextRun, Value};

const ROLES: &[(Role, &str, &[&[u8]])] = &[
    (Role::Page, "<div></div>", &[b"generic"]),
    (Role::Banner, r#"<header data-node="1"></header>"#, &[b"banner"]),
    (Role::Navigation, r#"<nav data-node="1"></nav>"#, &[b"navigation"]),
    (Role::Main, r#"<main data-node="1"></main>"#, &[b"main"]),
    (Role::Complementary, r#"<aside data-node="1"></aside>"#, &[b"complementary"]),
    (Role::ContentInfo, r#"<footer data-node="1"></footer>"#, &[b"contentinfo"]),
    (Role::Form, r#"<form data-node="1"></form>"#, &[b"form"]),
    (Role::Search, r#"<search data-node="1"></search>"#, &[b"search"]),
    (Role::Region, r#"<section data-node="1"></section>"#, &[b"region"]),
    (Role::Heading { level: 1 }, r#"<h1 data-node="1"></h1>"#, &[b"heading"]),
    (Role::Heading { level: 2 }, r#"<h2 data-node="1"></h2>"#, &[b"heading"]),
    (Role::Heading { level: 3 }, r#"<h3 data-node="1"></h3>"#, &[b"heading"]),
    (Role::Heading { level: 4 }, r#"<h4 data-node="1"></h4>"#, &[b"heading"]),
    (Role::Heading { level: 5 }, r#"<h5 data-node="1"></h5>"#, &[b"heading"]),
    (Role::Heading { level: 6 }, r#"<h6 data-node="1"></h6>"#, &[b"heading"]),
    (Role::Heading { level: 7 }, r#"<div data-node="1" role="heading" aria-level="7"></div>"#, &[b"heading"]),
    (Role::Paragraph, r#"<p data-node="1"></p>"#, &[b"paragraph"]),
    (Role::Text, r#"<span data-node="1"></span>"#, &[b"StaticText"]),
    (Role::List, r#"<ul data-node="1"></ul>"#, &[b"list"]),
    (Role::Item, r#"<li data-node="1"></li>"#, &[b"listitem"]),
    (Role::Table, r#"<table data-node="1"></table>"#, &[b"table"]),
    (Role::Row, r#"<tr data-node="1"></tr>"#, &[b"row"]),
    (Role::Cell, r#"<td data-node="1"></td>"#, &[b"cell"]),
    (Role::Term, r#"<div data-node="1" role="term"></div>"#, &[b"term"]),
    (Role::Description, r#"<div data-node="1" role="definition"></div>"#, &[b"definition"]),
    (Role::Button, r#"<button data-node="1" type="button"></button>"#, &[b"button"]),
    (Role::Link, r#"<a data-node="1" href=""></a>"#, &[b"link"]),
    (Role::TextField, r#"<input data-node="1" type="text">"#, &[b"textbox"]),
    (Role::TextArea, r#"<textarea data-node="1"></textarea>"#, &[b"textbox"]),
    (
        Role::Choice,
        r#"<input data-node="1" type="radio" id="skein-ui-label-1"><label for="skein-ui-label-1"></label>"#,
        &[b"radio"],
    ),
    (Role::ChoiceGroup, r#"<div data-node="1" role="radiogroup"></div>"#, &[b"radiogroup"]),
    (
        Role::Check,
        r#"<input data-node="1" type="checkbox" id="skein-ui-label-1"><label for="skein-ui-label-1"></label>"#,
        &[b"checkbox"],
    ),
    (Role::Dialog, r#"<dialog data-node="1"></dialog>"#, &[b"dialog"]),
    (Role::Status, r#"<div data-node="1" role="status"></div>"#, &[b"status"]),
    (Role::Alert, r#"<div data-node="1" role="alert"></div>"#, &[b"alert"]),
    (Role::Log, r#"<div data-node="1" role="log"></div>"#, &[b"log"]),
    (Role::Tree, r#"<div data-node="1" role="tree"></div>"#, &[b"tree"]),
    (Role::TreeItem, r#"<div data-node="1" role="treeitem"></div>"#, &[b"treeitem"]),
    (Role::Tabs, r#"<div data-node="1" role="tablist"></div>"#, &[b"tablist"]),
    (Role::Tab, r#"<a data-node="1" role="tab" href=""></a>"#, &[b"tab"]),
    (Role::Progress, r#"<div data-node="1" role="progressbar"></div>"#, &[b"progressbar"]),
    (Role::Time, r#"<time data-node="1"></time>"#, &[b"time"]),
    (Role::Code, r#"<pre data-node="1"><code></code></pre>"#, &[b"code"]),
    (Role::Quote, r#"<blockquote data-node="1"></blockquote>"#, &[b"blockquote"]),
    (Role::Separator, r#"<hr data-node="1">"#, &[b"separator"]),
    (
        Role::Menu,
        r#"<button type="button" aria-haspopup="menu" aria-controls="skein-ui-menu-1" aria-expanded="false"></button><div data-node="1" role="menu" id="skein-ui-menu-1" hidden=""></div>"#,
        &[b"menu"],
    ),
    (Role::MenuItem, r#"<div data-node="1" role="menuitem"></div>"#, &[b"menuitem"]),
    (Role::Disclosure, r#"<details data-node="1"><summary></summary></details>"#, &[b"group"]),
    (Role::Image, r#"<span data-node="1"></span>"#, &[b"image", b"StaticText"]),
    (Role::Figure, r#"<figure data-node="1"></figure>"#, &[b"figure"]),
    (Role::Slot, r#"<div data-node="1"></div>"#, &[b"generic"]),
    (Role::Drawing, r#"<figure data-node="1"></figure>"#, &[b"figure"]),
];

#[test]
fn each_role_maps_to_its_element() {
    let fixture = fixtures::made(Role::Text);
    let look = fixtures::look();
    for (role, expected, _) in ROLES {
        let mut made = fixture.clone();
        made.attribute.role = *role;
        assert_eq!(
            dom::print(&mapping::element(&made, &look, b"/bundle/digest/", Primary::Control)),
            *expected,
            "{role:?}"
        );
    }
}

#[test]
fn each_role_has_its_accessible_roles() {
    for (role, _, expected) in ROLES {
        assert_eq!(mapping::accessible(*role), *expected, "{role:?}");
    }
}

#[derive(Clone, Copy, Debug)]
enum Property {
    State(State),
    Count,
    Position,
    Spans(skein_ui::Spans),
    Description,
    Entry(skein_ui::EntryKind),
    Focus,
    Address,
    Name,
    Style,
    Region,
    Shortcut(Modifier),
    Primary(Primary),
    Escape,
    Value,
    Picture,
    External,
    Run(Run),
}

const LINK: Run = Run::Link { to: skein_lib::Token::new(1) };

const PROPERTIES: &[(Role, Property, &str)] = &[
    (Role::Table, Property::Count, r#"<table data-node="1" aria-rowcount="20"></table>"#),
    (Role::Item, Property::Position, r#"<li data-node="1" aria-posinset="7"></li>"#),
    (Role::TreeItem, Property::Position, r#"<div data-node="1" role="treeitem" aria-posinset="7"></div>"#),
    (Role::Row, Property::Position, r#"<tr data-node="1" aria-rowindex="7"></tr>"#),
    (
        Role::Cell,
        Property::Spans(skein_ui::Spans { rows: 2, columns: 3 }),
        r#"<td data-node="1" rowspan="2" colspan="3"></td>"#,
    ),
    (Role::Cell, Property::Spans(skein_ui::Spans { rows: 1, columns: 1 }), r#"<td data-node="1"></td>"#),
    (
        Role::Region,
        Property::Description,
        r#"<section data-node="1" aria-describedby="skein-ui-description-1"></section><span id="skein-ui-description-1">Help &amp; &lt;b&gt;</span>"#,
    ),
    (
        Role::Cell,
        Property::Description,
        r#"<td data-node="1" aria-describedby="skein-ui-description-1"><span id="skein-ui-description-1">Help &amp; &lt;b&gt;</span></td>"#,
    ),
    (Role::TextField, Property::Entry(skein_ui::EntryKind::Text), r#"<input data-node="1" type="text">"#),
    (Role::TextField, Property::Entry(skein_ui::EntryKind::Number), r#"<input data-node="1" type="number">"#),
    (Role::TextField, Property::Entry(skein_ui::EntryKind::Email), r#"<input data-node="1" type="email">"#),
    (Role::TextField, Property::Entry(skein_ui::EntryKind::Telephone), r#"<input data-node="1" type="tel">"#),
    (Role::TextField, Property::Entry(skein_ui::EntryKind::Url), r#"<input data-node="1" type="url">"#),
    (Role::TextField, Property::Entry(skein_ui::EntryKind::Date), r#"<input data-node="1" type="date">"#),
    (Role::TextField, Property::Entry(skein_ui::EntryKind::Time), r#"<input data-node="1" type="time">"#),
    (Role::TextField, Property::Entry(skein_ui::EntryKind::Secret), r#"<input data-node="1" type="password">"#),
    (Role::Heading { level: 1 }, Property::Focus, r#"<h1 data-node="1" tabindex="-1"></h1>"#),
    (Role::Image, Property::Address, r#"<img data-node="1" alt="" src="/a.png">"#),
    (Role::Button, Property::State(State::Disabled), r#"<button data-node="1" type="button" disabled=""></button>"#),
    (Role::Region, Property::State(State::Disabled), r#"<section data-node="1" aria-disabled="true"></section>"#),
    (Role::Region, Property::State(State::Expanded), r#"<section data-node="1" aria-expanded="true"></section>"#),
    (Role::Region, Property::State(State::Busy), r#"<section data-node="1" aria-busy="true"></section>"#),
    (Role::Region, Property::State(State::Current), r#"<section data-node="1" aria-current="true"></section>"#),
    (Role::Link, Property::State(State::Current), r#"<a data-node="1" aria-current="page" href=""></a>"#),
    (Role::Region, Property::State(State::Invalid), r#"<section data-node="1" aria-invalid="true"></section>"#),
    (
        Role::Button,
        Property::State(State::Pressed),
        r#"<button data-node="1" type="button" aria-pressed="true"></button>"#,
    ),
    (
        Role::Check,
        Property::State(State::Pressed),
        r#"<input data-node="1" type="checkbox" checked="" id="skein-ui-label-1"><label for="skein-ui-label-1"></label>"#,
    ),
    (Role::Tab, Property::State(State::Selected), r#"<a data-node="1" role="tab" href="" aria-selected="true"></a>"#),
    (
        Role::Choice,
        Property::State(State::Selected),
        r#"<input data-node="1" type="radio" checked="" id="skein-ui-label-1"><label for="skein-ui-label-1"></label>"#,
    ),
    (
        Role::TextArea,
        Property::State(State::SubmitsOnEnter),
        r#"<textarea data-node="1" enterkeyhint="send"></textarea>"#,
    ),
    (Role::Region, Property::Name, r#"<section data-node="1" aria-label="Words &amp; &lt;b&gt;"></section>"#),
    (Role::Button, Property::Name, r#"<button data-node="1" type="button">Words &amp; &lt;b&gt;</button>"#),
    (Role::Disclosure, Property::Name, r#"<details data-node="1"><summary>Words &amp; &lt;b&gt;</summary></details>"#),
    (
        Role::Check,
        Property::Name,
        r#"<input data-node="1" type="checkbox" id="skein-ui-label-1"><label for="skein-ui-label-1">Words &amp; &lt;b&gt;</label>"#,
    ),
    (Role::Image, Property::Name, r#"<span data-node="1">Words &amp; &lt;b&gt;</span>"#),
    (Role::Region, Property::Style, r#"<section data-node="1" class="test-mark"></section>"#),
    (Role::Region, Property::Region, r#"<section data-node="1" data-region="test-region"></section>"#),
    (
        Role::Button,
        Property::Shortcut(Modifier::Alt),
        r#"<button data-node="1" type="button" aria-keyshortcuts="Alt+K"></button>"#,
    ),
    (
        Role::Button,
        Property::Shortcut(Modifier::Plain),
        r#"<button data-node="1" type="button" aria-keyshortcuts="K"></button>"#,
    ),
    (Role::TextField, Property::Value, r#"<input data-node="1" type="text" value="&lt;value&gt;">"#),
    (Role::TextArea, Property::Value, r#"<textarea data-node="1">&lt;value&gt;</textarea>"#),
    (Role::Image, Property::Picture, r#"<img data-node="1" alt="" src="/bundle/digest/pictures/test/picture.png">"#),
    (
        Role::Link,
        Property::External,
        r#"<a data-node="1" href="https://example.com/" rel="noopener noreferrer" target="_blank"></a>"#,
    ),
    (Role::Text, Property::Run(Run::Plain), r#"<span data-node="1">&lt;run&gt;</span>"#),
    (Role::Text, Property::Run(Run::Emphasis), r#"<span data-node="1"><em>&lt;run&gt;</em></span>"#),
    (Role::Text, Property::Run(Run::Strong), r#"<span data-node="1"><strong>&lt;run&gt;</strong></span>"#),
    (Role::Text, Property::Run(Run::Code), r#"<span data-node="1"><code>&lt;run&gt;</code></span>"#),
    (Role::Text, Property::Run(LINK), r#"<span data-node="1"><a href="">&lt;run&gt;</a></span>"#),
    (Role::Button, Property::Escape, r#"<button data-node="1" type="button" aria-keyshortcuts="Escape"></button>"#),
    (
        Role::Button,
        Property::Primary(Primary::Control),
        r#"<button data-node="1" type="button" aria-keyshortcuts="Control+K"></button>"#,
    ),
    (
        Role::Button,
        Property::Primary(Primary::Meta),
        r#"<button data-node="1" type="button" aria-keyshortcuts="Meta+K"></button>"#,
    ),
];

fn property(made: &mut Made, property: Property) {
    match property {
        Property::State(state) => {
            made.attribute.states.push(state).expect("fixture state capacity");
        }
        Property::Count => made.attribute.count = Some(20),
        Property::Position => made.attribute.position = Some(7),
        Property::Spans(spans) => made.attribute.spans = Some(spans),
        Property::Description => made.attribute.description = Some(b"Help & <b>".to_vec().into_boxed_slice()),
        Property::Entry(kind) => made.attribute.entry = Some(kind),
        Property::Focus => made.attribute.focus = Some(1),
        Property::Address => {
            made.attribute.source = Some(skein_ui::Source::Address(b"/a.png".to_vec().into_boxed_slice()));
        }
        Property::Name => made.attribute.name = Some(b"Words & <b>".to_vec().into_boxed_slice()),
        Property::Style => {
            made.attribute.styles.push(Style { owner: OWNER, token: 0 }).expect("fixture style capacity");
        }
        Property::Region => made.attribute.region = Some(Region { owner: OWNER, number: 0 }),
        Property::Shortcut(modifier) => made.attribute.shortcut = Some(Shortcut { key: b'k', modifier }),
        Property::Primary(_) => made.attribute.shortcut = Some(Shortcut { key: b'k', modifier: Modifier::Primary }),
        Property::Escape => made.attribute.shortcut = Some(Shortcut::ESCAPE),
        Property::Value => made.value = Some(Value { text: b"<value>".to_vec().into_boxed_slice(), written: 1 }),
        Property::Picture => {
            made.attribute.source = Some(skein_ui::Source::Picture(Picture { owner: OWNER, number: 0 }));
        }
        Property::External => made.attribute.external = Some(b"https://example.com/".to_vec().into_boxed_slice()),
        Property::Run(run) => {
            made.text = vec![TextRun { run, text: b"<run>".to_vec().into_boxed_slice() }].into_boxed_slice();
        }
    }
}

#[test]
fn each_state_and_property_maps_to_its_attribute() {
    let fixture = fixtures::made(Role::Text);
    let look = fixtures::look();
    for (role, case, expected) in PROPERTIES {
        let mut made = fixture.clone();
        made.attribute.role = *role;
        property(&mut made, *case);
        let primary = match case {
            Property::Primary(primary) => *primary,
            _ => Primary::Control,
        };
        assert_eq!(
            dom::print(&mapping::element(&made, &look, b"/bundle/digest/", primary)),
            *expected,
            "{role:?}: {case:?}"
        );
    }
}

#[test]
fn every_node_maps_as_the_golden_page() {
    let mut limits = fixtures::LIMITS;
    limits.nodes = 128;
    let mut builder = skein_ui::Builder::new(&limits);
    builder.open(Role::Page);
    for (role, _, _) in ROLES.iter().skip(1) {
        builder.open(*role);
        builder.close();
    }
    builder.close();
    let tree = builder.finish();
    let look = fixtures::look();
    let mut page = Element::new("div");
    for node in tree.nodes().iter().skip(1) {
        let mut made = Made::of(node);
        made.id = skein_ui::NodeId(1);
        page.append(mapping::element(&made, &look, b"/bundle/digest/", Primary::Control));
    }
    let mut fixture = fixtures::made(Role::Text);
    for (role, case, _) in PROPERTIES {
        if let Property::Primary(Primary::Meta) = case {
            continue;
        }
        fixture.attribute.role = *role;
        let mut made = fixture.clone();
        property(&mut made, *case);
        page.append(mapping::element(&made, &look, b"/bundle/digest/", Primary::Control));
    }
    assert_eq!(dom::print(&page), include_str!("golden/mapping.html").trim_end());
}
