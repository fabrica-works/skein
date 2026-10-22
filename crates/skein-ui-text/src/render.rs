//! Role-based text, independent of styles (ui-text.md, 5).
use skein_ui::{EntryKind, Limits, Node, NodeId, Region, Role, Run, State, Tree};

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
        Some(Print {
            text: List::with_capacity(0),
            numbers: List::with_capacity(limits.nodes),
            name_bytes: limits.text_per_node,
        })
    }
}
/// Maximum output lines and bytes, including replacement bytes and indentation.
#[must_use]
pub fn worst_case(limits: &Limits) -> Option<(u32, u64)> {
    let text = u64::from(limits.text_bytes).checked_add(u64::from(limits.text_per_node))?;
    room(limits.nodes, limits.depth, text)
}
fn room(nodes: u32, depth: u32, text: u64) -> Option<(u32, u64)> {
    let lines = u64::from(nodes).checked_mul(2)?.checked_add(text)?.checked_add(1)?;
    // Each input byte can be a newline, with every ancestor named; code adds four.
    // Three replacement bytes and two code delimiters cover each nonempty run.
    let width = u64::from(depth).checked_mul(2)?.checked_add(6)?;
    let bytes = lines
        .checked_mul(width)?
        .checked_add(text.checked_mul(5)?)?
        // Base words, longest entry kind, required, and two u32 positions.
        .checked_add(u64::from(nodes).checked_mul(400 + 19 + 11 + 27)?)?
        .checked_add(16)?;
    Some((u32::try_from(lines).ok()?, bytes))
}
fn tree_room(tree: &Tree, name: &[u8]) -> Option<u32> {
    let mut text = u64::try_from(name.len()).ok()?;
    let mut ends = Stack::with_capacity(tree.len());
    let mut depth = 0;
    for at in 0..tree.len() {
        while let Some(end) = ends.top() {
            if *end > at {
                break;
            }
            let _end = ends.pop();
        }
        let n = tree.get(at).expect("Preorder node exists");
        ends.push(at.saturating_add(n.size)).expect("Ancestors fit tree nodes");
        depth = depth.max(ends.len());
        if let Some(value) = &n.value {
            text = text.checked_add(u64::try_from(value.text.len()).ok()?)?;
        }
        for bytes in [n.name.as_deref(), n.description.as_deref(), n.external.as_deref()].into_iter().flatten() {
            text = text.checked_add(u64::try_from(bytes.len()).ok()?)?;
        }
        for run in &n.text {
            text = text.checked_add(u64::try_from(run.text.len()).ok()?)?;
        }
    }
    u32::try_from(room(tree.len(), depth, text)?.1).ok()
}
#[derive(Debug)]
struct Parent {
    end: u32,
    indent: u32,
}
#[derive(Debug)]
struct PendingItem {
    at: u32,
    line: u32,
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
    print.numbers.clear();
    let Some(bytes) = tree_room(tree, name) else {
        print.text = List::with_capacity(name_bytes.saturating_mul(3).saturating_add(26));
        safe(&mut print.text, name, 0, false);
        raw(&mut print.text, b": page too long to print\n");
        return;
    };
    print.text = List::with_capacity(bytes);
    let (title, link) = header(tree, name, status, &mut print.text);
    for at in 0..tree.len() {
        if title == Some(at) || link == Some(at) {
            let n = tree.get(at).expect("header node exists");
            descriptions(tree, at, at.saturating_add(n.size), 2, &mut print.text);
        }
    }
    let mut parents: Stack<Parent> = Stack::with_capacity(tree.len());
    let mut pending: List<PendingItem> = List::with_capacity(tree.len());
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
            finish_items(tree, &mut pending, &mut print.text);
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
        let offset = print.text.len();
        let within_item = !pending.is_empty();
        let description_indent = match pending.last() {
            Some(item) => item.indent,
            None => indent.saturating_add(2),
        };
        match node(tree, index, indent, print) {
            Some(indent) => {
                if n.role == Role::Item || n.role == Role::TreeItem {
                    pending
                        .push(PendingItem { at: index, line: offset, indent })
                        .expect("pending items fit tree nodes");
                    finish_items(tree, &mut pending, &mut print.text);
                } else {
                    finish_items(tree, &mut pending, &mut print.text);
                    let described_indent = if within_item { description_indent } else { indent };
                    descriptions(tree, index, index.saturating_add(1), described_indent, &mut print.text);
                    finish_items(tree, &mut pending, &mut print.text);
                }
                parents.push(Parent { end, indent }).expect("parents fit tree nodes");
            }
            None => {
                position(tree, index, offset, &mut print.text);
                finish_items(tree, &mut pending, &mut print.text);
                descriptions(tree, index, end, description_indent, &mut print.text);
                skip = end;
            }
        }
    }
    while parents.pop().is_some() {
        if print.text.last() != Some(&b'\n') {
            raw(&mut print.text, b"\n");
        }
        finish_items(tree, &mut pending, &mut print.text);
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
        | Role::Select
        | Role::Tabs
        | Role::Log
        | Role::Drawing => {
            if let Some(name) = &n.name {
                start(&mut print.text, indent);
                safe(&mut print.text, name, indent, false);
                colon(&mut print.text, name);
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
        | Role::Option
        | Role::Check => {
            control(tree, at, indent, print);
            None
        }
        Role::Row => {
            start(&mut print.text, indent);
            let mut first = true;
            for child in at.saturating_add(1)..at.saturating_add(n.size) {
                let cell = tree.get(child).expect("row child exists");
                if [Role::Cell, Role::ColumnHeader, Role::RowHeader].contains(&cell.role) {
                    if !first {
                        raw(&mut print.text, b" | ");
                    }
                    content(tree, child, &mut print.text, indent, false);
                    sort(cell, &mut print.text);
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
        | Role::ColumnHeader
        | Role::RowHeader
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
        number(tree, n, print);
    }
    if n.role == Role::Choice || n.role == Role::Option {
        raw(&mut print.text, if has(n, State::Selected) { b"(*) " } else { b"( ) " });
    }
    if n.role == Role::Check {
        raw(&mut print.text, if has(n, State::Pressed) || has(n, State::Selected) { b"[x] " } else { b"[ ] " });
    }
    content(tree, at, &mut print.text, indent, false);
    if n.role == Role::TextField || n.role == Role::TextArea {
        raw(&mut print.text, b": ");
        match &n.value {
            Some(v) if !v.text.is_empty() => {
                if n.entry == Some(EntryKind::Secret) {
                    raw(&mut print.text, b"(hidden)");
                } else {
                    safe(&mut print.text, &v.text, indent, false);
                }
            }
            Some(_) | None => raw(&mut print.text, b"(empty)"),
        }
        entry(n, &mut print.text);
    }
    finish(n, &mut print.text);
}
fn entry(n: &Node, out: &mut List<u8>) {
    let word = match n.entry {
        None | Some(EntryKind::Text | EntryKind::Secret) => b"".as_slice(),
        Some(EntryKind::Number) => b" (number)",
        Some(EntryKind::Email) => b" (email)",
        Some(EntryKind::Telephone) => b" (telephone)",
        Some(EntryKind::Url) => b" (url)",
        Some(EntryKind::Date) => b" (date, YYYY-MM-DD)",
        Some(EntryKind::Time) => b" (time, HH:MM)",
    };
    raw(out, word);
}
fn descriptions(tree: &Tree, begin: u32, end: u32, indent: u32, out: &mut List<u8>) {
    for at in begin..end {
        let n = tree.get(at).expect("described node exists");
        if let Some(text) = &n.description {
            start(out, indent);
            safe(out, text, indent, false);
            raw(out, b"\n");
        }
    }
}
fn finish_items(tree: &Tree, pending: &mut List<PendingItem>, out: &mut List<u8>) {
    if out.last() != Some(&b'\n') {
        return;
    }
    for item in pending.iter() {
        let n = tree.get(item.at).expect("pending item exists");
        if n.size > 1 {
            let end = first_line_end(out, item.line);
            let old_end = out.len();
            states(n, out);
            insert_tail(out, end, old_end);
        }
        position(tree, item.at, item.line, out);
    }
    for item in pending.iter() {
        descriptions(tree, item.at, item.at.saturating_add(1), item.indent, out);
    }
    pending.clear();
}
fn first_line_end(out: &List<u8>, begin: u32) -> u32 {
    for at in begin..out.len() {
        if out.get(at) == Some(&b'\n') {
            return at;
        }
    }
    out.len()
}
fn position(tree: &Tree, at: u32, line: u32, out: &mut List<u8>) {
    let n = tree.get(at).expect("positioned node exists");
    if ![Role::Item, Role::Row, Role::TreeItem].contains(&n.role) {
        return;
    }
    let Some(position) = n.position else { return };
    let mut count = None;
    for ancestor in 0..at {
        let parent = tree.get(ancestor).expect("ancestor exists");
        if ancestor.saturating_add(parent.size) > at && [Role::List, Role::Table, Role::Tree].contains(&parent.role) {
            count = parent.count;
        }
    }
    let end = first_line_end(out, line);
    let old_end = out.len();
    raw(out, b" (");
    raw(out, Decimal::of(u64::from(position)).as_bytes());
    if let Some(count) = count {
        raw(out, b" of ");
        raw(out, Decimal::of(u64::from(count)).as_bytes());
    }
    raw(out, b")");
    insert_tail(out, end, old_end);
}
// Move the newly appended annotation before the existing suffix, in place.
fn insert_tail(out: &mut List<u8>, at: u32, old_end: u32) {
    let end = out.len();
    reverse(out, at, old_end);
    reverse(out, old_end, end);
    reverse(out, at, end);
}
fn reverse(out: &mut List<u8>, begin: u32, end: u32) {
    for offset in 0..end.saturating_sub(begin) / 2 {
        let left = begin.saturating_add(offset);
        let right = end.saturating_sub(offset).saturating_sub(1);
        let byte = *out.get(left).expect("annotation byte exists");
        *out.get_mut(left).expect("annotation byte exists") = *out.get(right).expect("annotation byte exists");
        *out.get_mut(right).expect("annotation byte exists") = byte;
    }
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
    sort(n, out);
    finish(n, out);
}
fn sort(n: &Node, out: &mut List<u8>) {
    if let Some(sort) = n.sort {
        match sort {
            skein_ui::Sort::Ascending => raw(out, b" (ascending)"),
            skein_ui::Sort::Descending => raw(out, b" (descending)"),
        }
    }
}
fn finish(n: &Node, out: &mut List<u8>) {
    states(n, out);
    raw(out, b"\n");
}
/// A group's name line ends in a colon, unless its name ends in its own mark.
fn colon(out: &mut List<u8>, name: &[u8]) {
    let marked = match name.last() {
        Some(last) => b"?!:".contains(last),
        None => false,
    };
    if !marked {
        raw(out, b":");
    }
}
fn start(out: &mut List<u8>, indent: u32) {
    if out.last() == Some(&b'\n') {
        spaces(out, indent);
    }
}
fn has(n: &Node, state: State) -> bool {
    n.states.as_slice().contains(&state)
}
fn number(tree: &Tree, n: &Node, print: &mut Print) {
    raw(&mut print.text, b"[");
    if (n.binding.is_some() || n.link.is_some()) && !tree.disabled(n.id) {
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
        State::Required,
        State::Completed,
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
                if n.role != Role::Choice && n.role != Role::Option && n.role != Role::Check {
                    raw(out, if state == State::Pressed { b" (pressed)" } else { b" (selected)" });
                }
            }
            State::Required => raw(out, b" (required)"),
            State::Completed => raw(out, b" (completed)"),
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
