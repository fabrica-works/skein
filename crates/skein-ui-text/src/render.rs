//! Role-based text, independent of styles (ui-text.md, 5).
use skein_ui::{Limits, Node, NodeId, Region, Role, Run, State, Tree};

use skein_lib::{Decimal, List, Stack};
/// One page's lines and its numbered node identities, beginning with `[1]`.
#[derive(Debug)]
#[expect(clippy::partial_pub_fields, reason = "ruling 106 keeps the configured name bound private")]
pub struct Print {
    pub text: List<u8>,
    pub numbers: List<NodeId>,
    name_bytes: u32,
}
impl Print {
    #[must_use]
    pub fn new(limits: &Limits) -> Option<Print> {
        let (_, bytes) = worst_case(limits)?;
        Some(Print {
            text: List::with_capacity(u32::try_from(bytes).ok()?),
            numbers: List::with_capacity(limits.nodes),
            name_bytes: limits.text_per_node,
        })
    }
}
/// Maximum output lines and bytes, including replacement bytes and indentation.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<(u32, u64)> {
    let text = u64::from(limits.text_bytes).checked_add(u64::from(limits.text_per_node))?;
    let lines = u64::from(limits.nodes).checked_add(text)?.checked_add(1)?;
    // Each input byte can be a newline, with every ancestor named; code adds four.
    // Three replacement bytes and two code delimiters cover each nonempty run.
    let width = u64::from(limits.depth).checked_mul(2)?.checked_add(6)?;
    let bytes = lines
        .checked_mul(width)?
        .checked_add(text.checked_mul(5)?)?
        .checked_add(u64::from(limits.nodes).checked_mul(400)?)?
        .checked_add(16)?;
    Some((u32::try_from(lines).ok()?, bytes))
}
#[derive(Debug)]
struct Parent {
    end: u32,
    indent: u32,
}
fn header(tree: &Tree, name: &[u8], status: Option<Region>, out: &mut List<u8>) -> (Option<u32>, Option<u32>) {
    let mut title = None;
    let mut main_end = 0;
    let mut link = None;
    for index in 0..tree.len() {
        let n = tree.get(index).expect("preorder node exists");
        if n.role == Role::Main {
            main_end = index.saturating_add(n.size);
        }
        if title.is_none() && index < main_end && n.role == (Role::Heading { level: 1 }) {
            title = Some(index);
        }
        if status.is_some() && n.region == status {
            link = Some(index);
        }
    }
    raw(out, b"== ");
    match title {
        Some(at) => content(tree, at, out, 0, false),
        None => safe(out, name, 0, false),
    }
    raw(out, b" == ");
    if let Some(at) = link {
        let root = tree.get(at).expect("link region exists");
        let begin = at.saturating_add(1);
        let mut next = begin;
        let mut first = true;
        for child in begin..at.saturating_add(root.size) {
            if child < next {
                continue;
            }
            if !first {
                raw(out, b"; ");
            }
            content(tree, child, out, 0, false);
            next = child.saturating_add(tree.get(child).expect("link child exists").size);
            first = false;
        }
    }
    raw(out, b"\n");
    (title, link)
}
/// Print the complete tree, assigning numbers in preorder.
pub fn render(tree: &Tree, name: &[u8], status: Option<Region>, print: &mut Print) {
    let name_bytes = u32::try_from(name.len()).expect("shell name fits its configured node text bound");
    assert!(name_bytes <= print.name_bytes, "shell name fits its configured node text bound");
    print.text.clear();
    print.numbers.clear();
    let (title, link) = header(tree, name, status, &mut print.text);
    let mut parents: Stack<Parent> = Stack::with_capacity(tree.len());
    let mut skip = 0;
    for index in 0..tree.len() {
        if index < skip {
            continue;
        }
        for _ in 0..parents.len() {
            if parents.top().expect("ancestor exists").end > index {
                break;
            }
            let _ended = parents.pop().expect("ancestor exists");
            if print.text.last() != Some(&b'\n') {
                raw(&mut print.text, b"\n");
            }
        }
        let indent = match parents.top() {
            Some(p) => p.indent,
            None => 0,
        };
        let n = tree.get(index).expect("preorder node exists");
        let end = index.saturating_add(n.size);
        if title == Some(index) || link == Some(index) {
            skip = end;
            continue;
        }
        match node(tree, index, indent, print) {
            Some(indent) => parents.push(Parent { end, indent }).expect("parents fit tree nodes"),
            None => skip = end,
        }
    }
    if print.text.last() != Some(&b'\n') {
        raw(&mut print.text, b"\n");
    }
}
fn node(tree: &Tree, at: u32, indent: u32, print: &mut Print) -> Option<u32> {
    let n = tree.get(at).expect("preorder node exists");
    match n.role {
        Role::Page | Role::Table | Role::Slot => Some(indent),
        Role::Banner
        | Role::Navigation
        | Role::Main
        | Role::Complementary
        | Role::ContentInfo
        | Role::Form
        | Role::Search
        | Role::Region
        | Role::Figure
        | Role::Dialog
        | Role::Disclosure
        | Role::List
        | Role::Tree
        | Role::Menu
        | Role::ChoiceGroup
        | Role::Tabs
        | Role::Log
        | Role::Drawing => {
            if let Some(name) = &n.name {
                start(&mut print.text, indent);
                safe(&mut print.text, name, indent, false);
                raw(&mut print.text, b":");
                finish(n, &mut print.text);
                Some(indent.saturating_add(2))
            } else {
                Some(indent)
            }
        }
        Role::Description => Some(indent.saturating_add(2)),
        Role::Item | Role::TreeItem => {
            start(&mut print.text, indent);
            raw(&mut print.text, b"- ");
            if n.size == 1 {
                content(tree, at, &mut print.text, indent, false);
                finish(n, &mut print.text);
            }
            Some(indent.saturating_add(2))
        }
        Role::Button
        | Role::Tab
        | Role::MenuItem
        | Role::Link
        | Role::TextField
        | Role::TextArea
        | Role::Choice
        | Role::Check => {
            control(tree, at, indent, print);
            None
        }
        Role::Row => {
            start(&mut print.text, indent);
            let mut first = true;
            for child in at.saturating_add(1)..at.saturating_add(n.size) {
                if tree.get(child).expect("row child exists").role == Role::Cell {
                    if !first {
                        raw(&mut print.text, b" | ");
                    }
                    content(tree, child, &mut print.text, indent, false);
                    first = false;
                }
            }
            finish(n, &mut print.text);
            None
        }
        Role::Heading { level } => {
            start(&mut print.text, indent);
            for _ in 0..level {
                raw(&mut print.text, b"#");
            }
            raw(&mut print.text, b" ");
            content(tree, at, &mut print.text, indent, false);
            finish(n, &mut print.text);
            None
        }
        Role::Paragraph
        | Role::Text
        | Role::Time
        | Role::Cell
        | Role::Term
        | Role::Status
        | Role::Alert
        | Role::Progress
        | Role::Code
        | Role::Quote
        | Role::Separator
        | Role::Image => {
            line(tree, at, indent, &mut print.text);
            None
        }
    }
}
fn control(tree: &Tree, at: u32, indent: u32, print: &mut Print) {
    let n = tree.get(at).expect("control exists");
    start(&mut print.text, indent);
    if n.external.is_none() {
        number(n, print);
    }
    if n.role == Role::Choice {
        raw(&mut print.text, if has(n, State::Selected) { b"(*) " } else { b"( ) " });
    }
    if n.role == Role::Check {
        raw(&mut print.text, if has(n, State::Pressed) || has(n, State::Selected) { b"[x] " } else { b"[ ] " });
    }
    content(tree, at, &mut print.text, indent, false);
    if n.role == Role::TextField || n.role == Role::TextArea {
        raw(&mut print.text, b": ");
        match &n.value {
            Some(v) if !v.text.is_empty() => safe(&mut print.text, &v.text, indent, false),
            Some(_) | None => raw(&mut print.text, b"(empty)"),
        }
    }
    finish(n, &mut print.text);
}
fn line(tree: &Tree, at: u32, mut indent: u32, out: &mut List<u8>) {
    let n = tree.get(at).expect("line node exists");
    if n.role == Role::Code {
        indent = indent.saturating_add(4);
    }
    start(out, indent);
    if n.role == Role::Status {
        raw(out, b"* ");
    }
    if n.role == Role::Alert {
        raw(out, b"! ");
    }
    let quote = n.role == Role::Quote;
    if quote {
        raw(out, b"> ");
    }
    if n.role == Role::Progress {
        if let Some(name) = &n.name {
            safe(out, name, indent, false);
        }
        raw(out, b": ");
    }
    if n.role == Role::Separator {
        raw(out, b"--");
    } else {
        content(tree, at, out, indent, quote);
    }
    if n.role == Role::Term {
        raw(out, b":");
    }
    finish(n, out);
}
fn finish(n: &Node, out: &mut List<u8>) {
    states(n, out);
    raw(out, b"\n");
}
fn start(out: &mut List<u8>, indent: u32) {
    if out.last() == Some(&b'\n') {
        spaces(out, indent);
    }
}
fn has(n: &Node, state: State) -> bool {
    n.states.as_slice().contains(&state)
}
fn number(n: &Node, print: &mut Print) {
    raw(&mut print.text, b"[");
    if (n.binding.is_some() || n.link.is_some()) && !has(n, State::Disabled) {
        print.numbers.push(n.id).expect("numbers fit nodes");
        raw(&mut print.text, Decimal::of(u64::from(print.numbers.len())).as_bytes());
    } else {
        raw(&mut print.text, b"-");
    }
    raw(&mut print.text, b"] ");
}
fn states(n: &Node, out: &mut List<u8>) {
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
        if !has(n, state) {
            continue;
        }
        match state {
            State::Disabled => raw(out, b" (disabled)"),
            State::Expanded => raw(out, b" (expanded)"),
            State::Busy => raw(out, b" (busy)"),
            State::Current => raw(out, b" (current)"),
            State::Invalid => raw(out, b" (invalid)"),
            State::Pressed | State::Selected => {
                if n.role != Role::Choice && n.role != Role::Check {
                    raw(out, if state == State::Pressed { b" (pressed)" } else { b" (selected)" });
                }
            }
            State::SubmitsOnEnter => {
                if n.role == Role::TextField || n.role == Role::TextArea {
                    raw(out, b" (enter sends)");
                }
            }
        }
    }
}
fn content(tree: &Tree, at: u32, out: &mut List<u8>, indent: u32, quote: bool) {
    let root = tree.get(at).expect("content root exists");
    let end = at.saturating_add(root.size);
    let mut found = false;
    let mut external = None;
    for index in at..end {
        if let Some((until, url)) = external
            && index >= until
        {
            raw(out, b" <");
            safe(out, url, indent, quote);
            raw(out, b">");
            external = None;
        }
        let node = tree.get(index).expect("content descendant exists");
        for run in &node.text {
            found = true;
            match run.run {
                Run::Code => {
                    raw(out, b"`");
                    safe(out, &run.text, indent, quote);
                    raw(out, b"`");
                }
                Run::Plain | Run::Emphasis | Run::Strong | Run::Link { .. } => safe(out, &run.text, indent, quote),
            }
        }
        if let Some(url) = &node.external {
            external = Some((index.saturating_add(node.size), url.as_ref()));
        }
    }
    if !found
        && root.role != Role::Progress
        && let Some(name) = &root.name
    {
        safe(out, name, indent, quote);
    }
    if let Some((_, url)) = external {
        raw(out, b" <");
        safe(out, url, indent, quote);
        raw(out, b">");
    }
}
fn raw(out: &mut List<u8>, bytes: &[u8]) {
    for byte in bytes {
        out.push(*byte).expect("text fits its derived print bound");
    }
}
fn spaces(out: &mut List<u8>, indent: u32) {
    for _ in 0..indent {
        raw(out, b" ");
    }
}
fn safe(out: &mut List<u8>, bytes: &[u8], indent: u32, quote: bool) {
    let mut next = 0;
    for at in 0..bytes.len() {
        if at < next {
            continue;
        }
        let lead = *bytes.get(at).expect("input byte exists");
        let length = match lead {
            0..=127 => 1,
            194..=223 => 2,
            224..=239 => 3,
            240..=244 => 4,
            _ => 0,
        };
        let end = at.saturating_add(length);
        let mut valid = length != 0 && end <= bytes.len();
        let second = bytes.get(at.saturating_add(1)).copied().unwrap_or(0);
        let second_range = match lead {
            224 => 160..=191,
            237 => 128..=159,
            240 => 144..=191,
            244 => 128..=143,
            _ => 128..=191,
        };
        if length > 1 && !second_range.contains(&second) {
            valid = false;
        }
        for index in at.saturating_add(1)..end {
            match bytes.get(index) {
                Some(byte) if (128..=191).contains(byte) => {}
                Some(_) | None => valid = false,
            }
        }
        if !valid {
            raw(out, b"\xef\xbf\xbd");
            continue;
        }
        next = end;
        if lead == 194 && second <= 159 {
            raw(out, b"\xef\xbf\xbd");
            continue;
        }
        match lead {
            10 => {
                raw(out, b"\n");
                spaces(out, indent);
                if quote {
                    raw(out, b"> ");
                }
            }
            9 => raw(out, b" "),
            0..=31 | 127 => raw(out, b"\xef\xbf\xbd"),
            _ => raw(out, bytes.get(at..end).expect("validated character exists")),
        }
    }
}
