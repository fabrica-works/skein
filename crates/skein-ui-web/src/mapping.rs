//! The default semantic tree mapping (ui-web.md, 6).

use skein_ui::{EntryKind, Made, Modifier, Role, Run, Shortcut, Source, State, TextRun};

use crate::{Look, dom::Element};

/// The platform's primary shortcut modifier, read once at startup.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Primary {
    Control,
    Meta,
}

#[must_use]
pub fn element(made: &Made, look: &Look<'_>, prefix: &[u8], primary: Primary) -> Element {
    let mut e = core_element(made, look, prefix, primary);
    if let Some(description) = &made.attribute.description {
        let id = format!("skein-ui-description-{}", made.id.0);
        let core = crate::page::semantic_mut(&mut e);
        core.set("aria-describedby", &id);
        let mut words = Element::new("span");
        words.set("id", &id);
        words.text(description);
        if [Role::Item, Role::Cell].contains(&made.attribute.role) {
            core.append(words);
        } else if e.tag.is_empty() {
            e.append(words);
        } else {
            let mut pair = Element::new("");
            pair.append(e);
            pair.append(words);
            e = pair;
        }
    }
    e
}

fn core_element(made: &Made, look: &Look<'_>, prefix: &[u8], primary: Primary) -> Element {
    let a = &made.attribute;
    let mut e = base(a.role, made.id.0);
    name(&mut e, made);
    properties(&mut e, made);
    if a.role == Role::Disclosure && a.name.is_none() {
        e.append(Element::new("summary"));
    }
    for state in [
        State::Disabled,
        State::Expanded,
        State::Busy,
        State::Current,
        State::Invalid,
        State::Pressed,
        State::Selected,
        State::SubmitsOnEnter,
    ] {
        if a.states.as_slice().contains(&state) {
            state_attribute(&mut e, a.role, state);
        }
    }
    let classes: Vec<_> = a
        .styles
        .iter()
        .filter_map(|style| look.names.iter().find_map(|names| names.style(*style)))
        .map(words)
        .collect();
    if !classes.is_empty() {
        e.set("class", &classes.join(" "));
    }
    if let Some(region) = a.region
        && let Some(name) = look.names.iter().find_map(|names| names.region(region))
    {
        e.set("data-region", &words(name));
    }
    shortcut(&mut e, a.shortcut, primary);
    if a.role == Role::Link {
        if let Some(url) = &a.external {
            e.set("href", &words(url));
            e.set("rel", "noopener noreferrer");
            e.set("target", "_blank");
        } else {
            e.set("href", "");
        }
    }
    image(&mut e, made, look, prefix);
    value(&mut e, made);
    for run in &made.text {
        text_run(&mut e, run);
    }
    if a.role == Role::Code {
        let mut code = Element::new("code");
        code.children = std::mem::take(&mut e.children);
        e.append(code);
    }
    if a.role == Role::Progress {
        let text: String = made.text.iter().map(|run| words(&run.text)).collect();
        if !text.is_empty() {
            e.set("aria-valuetext", &text);
        }
    }
    if a.role == Role::Choice || a.role == Role::Check {
        let id = format!("skein-ui-label-{}", made.id.0);
        e.remove("aria-label");
        e.set("id", &id);
        let mut label = Element::new("label");
        label.set("for", &id);
        if let Some(name) = &a.name {
            label.text(name);
        }
        let mut pair = Element::new("");
        pair.append(e);
        pair.append(label);
        return pair;
    }
    if a.role == Role::Menu {
        let id = format!("skein-ui-menu-{}", made.id.0);
        e.set("id", &id);
        e.set("hidden", "");
        let mut button = Element::new("button");
        button.set("type", "button");
        button.set("aria-haspopup", "menu");
        button.set("aria-controls", &id);
        button.set("aria-expanded", "false");
        if let Some(name) = &a.name {
            button.text(name);
        }
        let mut pair = Element::new("");
        pair.append(button);
        pair.append(e);
        return pair;
    }
    e
}

fn properties(e: &mut Element, made: &Made) {
    let a = &made.attribute;
    if let Some(kind) = a.entry {
        e.set(
            "type",
            match kind {
                EntryKind::Text => "text",
                EntryKind::Number => "number",
                EntryKind::Email => "email",
                EntryKind::Telephone => "tel",
                EntryKind::Url => "url",
                EntryKind::Date => "date",
                EntryKind::Time => "time",
                EntryKind::Secret => "password",
            },
        );
    }
    if a.focus.is_some()
        && ![
            Role::Button,
            Role::Link,
            Role::TextField,
            Role::TextArea,
            Role::Choice,
            Role::Check,
            Role::Dialog,
            Role::Tab,
            Role::Disclosure,
        ]
        .contains(&a.role)
    {
        e.set("tabindex", "-1");
    }
}

fn base(role: Role, id: u32) -> Element {
    let (tag, aria) = tag(role);
    let mut e = Element::new(tag);
    if role != Role::Page {
        e.set("data-node", &id.to_string());
    }
    if let Some(aria) = aria {
        e.set("role", aria);
    }
    match role {
        Role::Heading { level } => {
            if level > 6 || level == 0 {
                e.set("aria-level", &level.to_string());
            }
        }
        Role::Button => e.set("type", "button"),
        Role::TextField => e.set("type", "text"),
        Role::Choice => e.set("type", "radio"),
        Role::Check => e.set("type", "checkbox"),
        Role::Tab => e.set("href", ""),
        Role::Image => e.set("alt", ""),
        Role::Page
        | Role::Banner
        | Role::Navigation
        | Role::Main
        | Role::Complementary
        | Role::ContentInfo
        | Role::Form
        | Role::Search
        | Role::Region
        | Role::Paragraph
        | Role::Text
        | Role::List
        | Role::Item
        | Role::Table
        | Role::Row
        | Role::Cell
        | Role::Term
        | Role::Description
        | Role::Link
        | Role::TextArea
        | Role::ChoiceGroup
        | Role::Dialog
        | Role::Status
        | Role::Alert
        | Role::Log
        | Role::Tree
        | Role::TreeItem
        | Role::Tabs
        | Role::Progress
        | Role::Time
        | Role::Code
        | Role::Quote
        | Role::Separator
        | Role::Menu
        | Role::MenuItem
        | Role::Disclosure
        | Role::Figure
        | Role::Slot
        | Role::Drawing => {}
    }
    e
}

fn state_attribute(e: &mut Element, role: Role, state: State) {
    match state {
        State::Disabled => e.set(
            if [Role::Button, Role::TextField, Role::TextArea, Role::Choice, Role::Check].contains(&role) {
                "disabled"
            } else {
                "aria-disabled"
            },
            if [Role::Button, Role::TextField, Role::TextArea, Role::Choice, Role::Check].contains(&role) {
                ""
            } else {
                "true"
            },
        ),
        State::Expanded => e.set("aria-expanded", "true"),
        State::Busy => e.set("aria-busy", "true"),
        State::Current => e.set("aria-current", if role == Role::Link { "page" } else { "true" }),
        State::Invalid => e.set("aria-invalid", "true"),
        State::Pressed => e.set(
            if role == Role::Check { "checked" } else { "aria-pressed" },
            if role == Role::Check { "" } else { "true" },
        ),
        State::Selected => e.set(
            if role == Role::Choice { "checked" } else { "aria-selected" },
            if role == Role::Choice { "" } else { "true" },
        ),
        State::SubmitsOnEnter => e.set("enterkeyhint", "send"),
    }
}

fn text_run(e: &mut Element, run: &TextRun) {
    let tag = match run.run {
        Run::Plain => None,
        Run::Emphasis => Some("em"),
        Run::Strong => Some("strong"),
        Run::Code => Some("code"),
        Run::Link { .. } => Some("a"),
    };
    if let Some(tag) = tag {
        let mut child = Element::new(tag);
        if tag == "a" {
            child.set("href", "");
        }
        child.text(&run.text);
        e.append(child);
    } else {
        e.text(&run.text);
    }
}

fn words(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Accessible roles queried by the browser face, in Chromium's vocabulary.
#[must_use]
pub const fn accessible(role: Role) -> &'static [&'static [u8]] {
    match role {
        Role::Page | Role::Slot => &[b"generic"],
        Role::Banner => &[b"banner"],
        Role::Navigation => &[b"navigation"],
        Role::Main => &[b"main"],
        Role::Complementary => &[b"complementary"],
        Role::ContentInfo => &[b"contentinfo"],
        Role::Form => &[b"form"],
        Role::Search => &[b"search"],
        Role::Region => &[b"region"],
        Role::Heading { .. } => &[b"heading"],
        Role::Paragraph => &[b"paragraph"],
        Role::Text => &[b"StaticText"],
        Role::List => &[b"list"],
        Role::Item => &[b"listitem"],
        Role::Table => &[b"table"],
        Role::Row => &[b"row"],
        Role::Cell => &[b"cell"],
        Role::Term => &[b"term"],
        Role::Description => &[b"definition"],
        Role::Button => &[b"button"],
        Role::Link => &[b"link"],
        Role::TextField | Role::TextArea => &[b"textbox"],
        Role::Choice => &[b"radio"],
        Role::ChoiceGroup => &[b"radiogroup"],
        Role::Check => &[b"checkbox"],
        Role::Dialog => &[b"dialog"],
        Role::Status => &[b"status"],
        Role::Alert => &[b"alert"],
        Role::Log => &[b"log"],
        Role::Tree => &[b"tree"],
        Role::TreeItem => &[b"treeitem"],
        Role::Tabs => &[b"tablist"],
        Role::Tab => &[b"tab"],
        Role::Progress => &[b"progressbar"],
        Role::Time => &[b"time"],
        Role::Code => &[b"code"],
        Role::Quote => &[b"blockquote"],
        Role::Separator => &[b"separator"],
        Role::Menu => &[b"menu"],
        Role::MenuItem => &[b"menuitem"],
        Role::Disclosure => &[b"group"],
        Role::Image => &[b"image", b"StaticText"],
        Role::Figure | Role::Drawing => &[b"figure"],
    }
}

fn name(e: &mut Element, made: &Made) {
    let a = &made.attribute;
    if let Some(name) = &a.name {
        match a.role {
            Role::Button | Role::Tab | Role::MenuItem => e.text(name),
            Role::Disclosure => {
                let mut summary = Element::new("summary");
                summary.text(name);
                e.append(summary);
            }
            Role::Image => e.set("alt", &words(name)),
            Role::Page
            | Role::Banner
            | Role::Navigation
            | Role::Main
            | Role::Complementary
            | Role::ContentInfo
            | Role::Form
            | Role::Search
            | Role::Region
            | Role::Heading { .. }
            | Role::Paragraph
            | Role::Text
            | Role::List
            | Role::Item
            | Role::Table
            | Role::Row
            | Role::Cell
            | Role::Term
            | Role::Description
            | Role::Link
            | Role::TextField
            | Role::TextArea
            | Role::Choice
            | Role::ChoiceGroup
            | Role::Check
            | Role::Dialog
            | Role::Status
            | Role::Alert
            | Role::Log
            | Role::Tree
            | Role::TreeItem
            | Role::Tabs
            | Role::Progress
            | Role::Time
            | Role::Code
            | Role::Quote
            | Role::Separator
            | Role::Menu
            | Role::Figure
            | Role::Slot
            | Role::Drawing => e.set("aria-label", &words(name)),
        }
    }
}

fn image(e: &mut Element, made: &Made, look: &Look<'_>, prefix: &[u8]) {
    let a = &made.attribute;
    if a.role == Role::Image {
        let source = match &a.source {
            Some(Source::Address(address)) => Some(words(address)),
            Some(Source::Picture(picture)) => picture_source(*picture, look, prefix),
            None => None,
        };
        if let Some(source) = source {
            e.set("src", &source);
        } else {
            e.tag = "span".into();
            e.remove("alt");
            if let Some(name) = &a.name {
                e.text(name);
            }
        }
    }
}

fn picture_source(picture: skein_ui::Picture, look: &Look<'_>, prefix: &[u8]) -> Option<String> {
    let owner = look.names.iter().find(|names| names.owner == picture.owner)?;
    let (_, files) = look.pictures.iter().find(|(owner, _)| *owner == picture.owner)?;
    let file = files.get(usize::from(picture.number))?;
    Some(format!("{}pictures/{}/{}", words(prefix), words(owner.prefix), words(file)))
}

fn value(e: &mut Element, made: &Made) {
    let a = &made.attribute;
    if let Some(value) = &made.value {
        match a.role {
            Role::TextArea => e.text(&value.text),
            Role::Page
            | Role::Banner
            | Role::Navigation
            | Role::Main
            | Role::Complementary
            | Role::ContentInfo
            | Role::Form
            | Role::Search
            | Role::Region
            | Role::Heading { .. }
            | Role::Paragraph
            | Role::Text
            | Role::List
            | Role::Item
            | Role::Table
            | Role::Row
            | Role::Cell
            | Role::Term
            | Role::Description
            | Role::Button
            | Role::Link
            | Role::TextField
            | Role::Choice
            | Role::ChoiceGroup
            | Role::Check
            | Role::Dialog
            | Role::Status
            | Role::Alert
            | Role::Log
            | Role::Tree
            | Role::TreeItem
            | Role::Tabs
            | Role::Tab
            | Role::Progress
            | Role::Time
            | Role::Code
            | Role::Quote
            | Role::Separator
            | Role::Menu
            | Role::MenuItem
            | Role::Disclosure
            | Role::Image
            | Role::Figure
            | Role::Slot
            | Role::Drawing => e.set("value", &words(&value.text)),
        }
    }
}

fn tag(role: Role) -> (&'static str, Option<&'static str>) {
    match role {
        Role::Page | Role::Slot => ("div", None),
        Role::Banner => ("header", None),
        Role::Navigation => ("nav", None),
        Role::Main => ("main", None),
        Role::Complementary => ("aside", None),
        Role::ContentInfo => ("footer", None),
        Role::Form => ("form", None),
        Role::Search => ("search", None),
        Role::Region => ("section", None),
        Role::Heading { level } => match level {
            1 => ("h1", None),
            2 => ("h2", None),
            3 => ("h3", None),
            4 => ("h4", None),
            5 => ("h5", None),
            6 => ("h6", None),
            _ => ("div", Some("heading")),
        },
        Role::Paragraph => ("p", None),
        Role::Text => ("span", None),
        Role::List => ("ul", None),
        Role::Item => ("li", None),
        Role::Table => ("table", None),
        Role::Row => ("tr", None),
        Role::Cell => ("td", None),
        Role::Term => ("div", Some("term")),
        Role::Description => ("div", Some("definition")),
        Role::Button => ("button", None),
        Role::Link => ("a", None),
        Role::TextField | Role::Choice | Role::Check => ("input", None),
        Role::TextArea => ("textarea", None),
        Role::ChoiceGroup => ("div", Some("radiogroup")),
        Role::Dialog => ("dialog", None),
        Role::Status => ("div", Some("status")),
        Role::Alert => ("div", Some("alert")),
        Role::Log => ("div", Some("log")),
        Role::Tree => ("div", Some("tree")),
        Role::TreeItem => ("div", Some("treeitem")),
        Role::Tabs => ("div", Some("tablist")),
        Role::Tab => ("a", Some("tab")),
        Role::Progress => ("div", Some("progressbar")),
        Role::Time => ("time", None),
        Role::Code => ("pre", None),
        Role::Quote => ("blockquote", None),
        Role::Separator => ("hr", None),
        Role::Menu => ("div", Some("menu")),
        Role::MenuItem => ("div", Some("menuitem")),
        Role::Disclosure => ("details", None),
        Role::Image => ("img", None),
        Role::Figure | Role::Drawing => ("figure", None),
    }
}

fn shortcut(e: &mut Element, shortcut: Option<Shortcut>, primary: Primary) {
    if let Some(shortcut) = shortcut {
        let modifier = match shortcut.modifier {
            Modifier::Plain => "",
            Modifier::Primary => match primary {
                Primary::Control => "Control+",
                Primary::Meta => "Meta+",
            },
            Modifier::Alt => "Alt+",
        };
        let key = if shortcut.key == 27 {
            "Escape".into()
        } else {
            char::from(shortcut.key.to_ascii_uppercase()).to_string()
        };
        e.set("aria-keyshortcuts", &format!("{modifier}{key}"));
    }
}
