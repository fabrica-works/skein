//! A bounded outline of the kit's tree (lab-browser.md, 8.4).
use crate::json::{self, string};
use skein_browser::boundary::{Entry, Node, States};
use skein_fake_person::role_name;
use skein_ui::Role;
use skein_ui_web::mapping::person;
use std::fmt::Write;
pub const LIMIT: usize = 64 << 10;
pub fn states(states: States) -> impl Iterator<Item = &'static str> {
    [
        (states.focused, "focused"),
        (states.disabled, "disabled"),
        (states.checked, "checked"),
        (states.expanded, "expanded"),
        (states.selected, "selected"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
}
fn quoted(bytes: &[u8]) -> String {
    String::from_utf8(json::encode(&string(String::from_utf8_lossy(bytes)))).expect("JSON UTF-8")
}
fn dropped(entry: &Entry) -> bool {
    [b"RootWebArea".as_slice(), b"generic", b"none"].contains(&entry.role.as_ref())
}
pub fn outline(entries: &[Entry], root: Option<Node>) -> (String, u32) {
    let start = match root {
        Some(root) => match entries.iter().position(|entry| entry.node == Some(root)) {
            Some(start) => start,
            None => return (String::new(), 0),
        },
        None => 0,
    };
    let Some(first) = entries.get(start) else { return (String::new(), 0) };
    let mut ancestors: Vec<&Entry> = Vec::new();
    let mut out = String::new();
    let mut left = 0_u32;
    let mut cut = false;
    for (offset, entry) in entries[start..].iter().enumerate() {
        if root.is_some() && offset != 0 && entry.depth <= first.depth {
            break;
        }
        while ancestors.last().is_some_and(|parent| parent.depth >= entry.depth) {
            ancestors.pop();
        }
        if dropped(entry) {
            continue;
        }
        let text = entry.role.as_ref() == b"StaticText" || entry.role.as_ref() == b"InlineTextBox";
        if text
            && !entry.name.is_empty()
            && ancestors
                .last()
                .is_some_and(|parent| parent.name.windows(entry.name.len()).any(|words| words == entry.name.as_ref()))
        {
            continue;
        }
        let role = person(&entry.role, entry.multiline);
        let mut line = "  ".repeat(ancestors.len());
        if let Some(role) = role {
            line.push_str(role_name(role));
            if let Role::Heading { .. } = role {
                write!(line, " {}", entry.level).expect("string write");
            }
            if role == Role::List {
                let mut parents = Vec::new();
                let mut count = 0;
                for child in entries[start + offset + 1..].iter().take_while(|child| child.depth > entry.depth) {
                    while parents.last().is_some_and(|depth| *depth >= child.depth) {
                        parents.pop();
                    }
                    if dropped(child) {
                        continue;
                    }
                    if parents.is_empty() && child.role.as_ref() == b"listitem" {
                        count += 1;
                    }
                    parents.push(child.depth);
                }
                write!(line, " ({count})").expect("string write");
            }
        } else {
            write!(line, "<{}>", String::from_utf8_lossy(&entry.role)).expect("string write");
        }
        if !entry.name.is_empty() {
            line.push(' ');
            line.push_str(&quoted(&entry.name));
        }
        if !entry.value.is_empty() {
            line.push_str(" = ");
            line.push_str(&quoted(&entry.value));
        }
        for state in states(entry.states) {
            write!(line, " [{state}]").expect("string write");
        }
        line.push('\n');
        if cut || out.len() + line.len() > LIMIT {
            cut = true;
            left += 1;
        } else {
            out.push_str(&line);
        }
        ancestors.push(entry);
    }
    (out, left)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn entry(node: u64, depth: u32, role: &str, name: &str) -> Entry {
        Entry {
            node: Some(node),
            depth,
            role: role.as_bytes().into(),
            name: name.as_bytes().into(),
            level: 0,
            value: Box::new([]),
            states: States { focused: false, disabled: false, checked: false, expanded: false, selected: false },
            multiline: false,
        }
    }
    #[test]
    fn nesting_levels_counts_values_states_text_and_lifted_containers() {
        let mut entries = vec![
            entry(1, 0, "RootWebArea", ""),
            entry(2, 1, "generic", ""),
            entry(3, 2, "main", ""),
            entry(4, 3, "heading", "Hello world"),
            entry(5, 4, "StaticText", "world"),
            entry(6, 4, "StaticText", "Other"),
            entry(7, 3, "list", ""),
            entry(8, 4, "listitem", ""),
            entry(9, 4, "listitem", ""),
            entry(10, 3, "textbox", "Title"),
            entry(11, 3, "textbox", "Message"),
            entry(12, 3, "article", "News"),
        ];
        entries[3].level = 2;
        entries[9].value = b"words".as_slice().into();
        entries[9].states = States { focused: true, disabled: true, checked: true, expanded: true, selected: true };
        entries[10].multiline = true;
        let expected = "main\n  heading 2 \"Hello world\"\n    text \"Other\"\n  list (2)\n    item\n    item\n  text field \"Title\" = \"words\" [focused] [disabled] [checked] [expanded] [selected]\n  text area \"Message\"\n  <article> \"News\"\n";
        assert_eq!(outline(&entries, None), (expected.into(), 0));
        assert_eq!(outline(&entries, Some(7)), ("list (2)\n  item\n  item\n".into(), 0));
        assert_eq!(outline(&entries, Some(99)), (String::new(), 0));
    }
    #[test]
    fn cut_is_at_a_whole_line_and_counts_only_kept_entries() {
        // Each 1024-byte line has a name within the kit's 4096-byte bound.
        let mut entries: Vec<_> = (0..64).map(|node| entry(node, 0, "StaticText", &"x".repeat(1016))).collect();
        assert_eq!(outline(&entries, None), (format!("text \"{}\"\n", "x".repeat(1016)).repeat(64), 0));
        entries.last_mut().unwrap().name = "x".repeat(1015).into_bytes().into();
        assert_eq!(outline(&entries, None).0.len(), LIMIT - 1);
        assert_eq!(outline(&entries, None).1, 0);
        entries.last_mut().unwrap().name = "x".repeat(1017).into_bytes().into();
        entries.push(entry(100, 0, "button", "Later"));
        assert_eq!(outline(&entries, None).0.len(), LIMIT - 1024);
        assert_eq!(outline(&entries, None).1, 2);
    }
    #[test]
    fn nearest_kept_name_and_direct_items_after_containers_are_lifted() {
        let entries = vec![
            entry(1, 0, "list", ""),
            entry(2, 1, "generic", ""),
            entry(3, 2, "listitem", "Label"),
            entry(4, 3, "none", ""),
            entry(5, 4, "StaticText", "Label"),
            entry(6, 5, "InlineTextBox", "Label"),
            entry(7, 3, "list", ""),
            entry(8, 4, "listitem", "Nested"),
        ];
        assert_eq!(
            outline(&entries, None),
            ("list (1)\n  item \"Label\"\n    list (1)\n      item \"Nested\"\n".into(), 0)
        );
    }
}
