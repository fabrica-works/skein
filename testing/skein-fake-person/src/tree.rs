//! Immediate accessibility queries, never drawing data or style tokens.
//! The preorder subtree sizes scope queries (skein's ui.md, section 17.1).
use crate::{Face, Find, Found, Target};
use alloc::vec::Vec;
use skein_ui::Act;
use skein_ui::{Node, NodeId, Role, State, Tree};

/// An immediate face over the last tree shown to the person.
#[derive(Debug)]
pub struct TreeFace<'a> {
    tree: &'a Tree,
    start: usize,
    end: usize,
}
impl<'a> TreeFace<'a> {
    /// Borrow the displayed tree; builders' unassigned ids are not visible ids.
    #[must_use]
    pub fn new(tree: &'a Tree) -> Self {
        Self { tree, start: 0, end: tree.nodes().len() }
    }
    /// Query one visible subtree using the same role, text and ancestor finds.
    /// Repeated accessible names outside this subtree cannot shadow its targets.
    #[must_use]
    pub fn subtree(tree: &'a Tree, id: NodeId) -> Option<Self> {
        if id == NodeId(0) {
            return None;
        }
        let start = tree.nodes().iter().position(|node| node.id == id)?;
        let end = start.checked_add(usize::try_from(tree.nodes()[start].size).ok()?)?;
        if end > tree.nodes().len() {
            return None;
        }
        Some(Self { tree, start, end })
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    match (core::str::from_utf8(hay), core::str::from_utf8(needle)) {
        (Ok(hay), Ok(needle)) => hay.to_lowercase().contains(&needle.to_lowercase()),
        (Ok(_) | Err(_), Ok(_) | Err(_)) => hay.windows(needle.len()).any(|part| part.eq_ignore_ascii_case(needle)),
    }
}
fn text(node: &Node) -> Vec<u8> {
    node.text.as_slice().iter().flat_map(|run| run.text.iter().copied()).collect()
}
fn exactly_named(nodes: &[Node], index: usize, role: skein_ui::Role, name: &[u8]) -> bool {
    let node = &nodes[index];
    if node.role != role {
        return false;
    }
    if let Some(name_text) = &node.name {
        return name_text.as_ref() == name;
    }
    let end = index.checked_add(usize::try_from(node.size).expect("subtree fits usize")).expect("subtree end fits");
    let fallback: Vec<u8> = nodes[index..end].iter().flat_map(text).collect();
    fallback == name
}
fn matches(nodes: &[Node], index: usize, target: &Target) -> bool {
    let node = &nodes[index];
    match target {
        Target::Role { role, name } => {
            if node.role != *role {
                return false;
            }
            if let Some(name_text) = &node.name {
                return contains(name_text, name);
            }
            let end =
                index.checked_add(usize::try_from(node.size).expect("subtree fits usize")).expect("subtree end fits");
            let fallback: Vec<u8> = nodes[index..end].iter().flat_map(text).collect();
            contains(&fallback, name)
        }
        Target::ExactRole { role, name } => exactly_named(nodes, index, *role, name),
        Target::Text { text: words } => !node.text.is_empty() && contains(&text(node), words),
    }
}

impl Face for TreeFace<'_> {
    fn find(&mut self, find: &Find) -> Found {
        let nodes = self.tree.nodes();
        let mut start = self.start;
        let mut end = self.end;
        for region in &find.within {
            let Some((index, node)) =
                nodes.iter().enumerate().take(end).skip(start).find(|(index, _)| matches(nodes, *index, region))
            else {
                return Found::Absent;
            };
            start = index.checked_add(1).expect("subtree index fits");
            end = index.checked_add(usize::try_from(node.size).expect("subtree size fits")).expect("subtree end fits");
        }
        (start..end)
            .find(|index| matches(nodes, *index, &find.target))
            .map_or(Found::Absent, |index| Found::Node(nodes[index].id))
    }
    fn allows(&self, id: NodeId, act: &Act) -> bool {
        if !self.tree.nodes()[self.start..self.end].iter().any(|node| node.id == id) {
            return false;
        }
        let Some(node) = self.tree.find(id) else {
            return false;
        };
        if self.tree.bound(id).is_none() || node.states.as_slice().contains(&State::Disabled) || passive(node.role) {
            return false;
        }
        match act {
            Act::Press | Act::Toggle => true,
            Act::Drawing { .. } => node.role == Role::Drawing,
            Act::Input { .. } => node.role == Role::TextField || node.role == Role::TextArea,
            Act::Submit => node.states.as_slice().contains(&State::SubmitsOnEnter),
        }
    }
}

pub(crate) fn role_name(role: Role) -> &'static str {
    match role {
        Role::Page => "page",
        Role::Banner => "banner",
        Role::Navigation => "navigation",
        Role::Main => "main",
        Role::Complementary => "complementary",
        Role::ContentInfo => "content info",
        Role::Form => "form",
        Role::Search => "search",
        Role::Menu => "menu",
        Role::MenuItem => "menu item",
        Role::Disclosure => "disclosure",
        Role::Image => "image",

        Role::Region => "region",
        Role::Heading { .. } => "heading",
        Role::Paragraph => "paragraph",
        Role::Text => "text",
        Role::List => "list",
        Role::Item => "item",
        Role::Table => "table",
        Role::Row => "row",
        Role::Cell => "cell",
        Role::Term => "term",
        Role::Description => "description",
        Role::Button => "button",
        Role::Link => "link",
        Role::TextField => "text field",
        Role::TextArea => "text area",
        Role::Choice => "choice",
        Role::ChoiceGroup => "choice group",
        Role::Check => "check",
        Role::Dialog => "dialog",
        Role::Status => "status",
        Role::Alert => "alert",
        Role::Log => "log",
        Role::Tree => "tree",
        Role::TreeItem => "tree item",
        Role::Tabs => "tabs",
        Role::Tab => "tab",
        Role::Progress => "progress",
        Role::Time => "time",
        Role::Code => "code",
        Role::Quote => "quote",
        Role::Separator => "separator",
        Role::Figure => "figure",
        Role::Slot => "slot",
        Role::Drawing => "drawing",
    }
}

pub(crate) fn passive(role: Role) -> bool {
    [
        Role::Banner,
        Role::Navigation,
        Role::Main,
        Role::Complementary,
        Role::ContentInfo,
        Role::Form,
        Role::Search,
        Role::Menu,
        Role::Disclosure,
        Role::Image,
    ]
    .contains(&role)
}
