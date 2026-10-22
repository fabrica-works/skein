//! Stable identities and bounded edits (skein's ui.md, section 7).
//! Roots match; below matched parents, keys match objects and unkeyed nodes
//! match by position among the same role. Four bounded index tables keep the
//! relationships. Structural patches remove unmatched roots, insert unmatched
//! roots after positioning matched siblings by reverse moves. Insertions then
//! use the already-positioned siblings as anchors, in reverse order.
//! A diff emits at most four patches per node (ui.md, 7).
//! Bindings stay in the view: the shell sees only their presence and node ids.

use crate::Limits;
use crate::{
    Drawing, EntryKind, Node, NodeId, Region, Role, Shortcut, Sort, Source, Spans, State, Style, TextRun, Tree, Value,
};
use alloc::boxed::Box;
use skein_lib::{List, Queue, Stack};

/// The attributes a shell maps, without the owner's action or address tokens.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Attribute {
    pub role: Role,
    pub name: Option<Box<[u8]>>,
    pub states: List<State>,
    pub styles: List<Style>,
    pub region: Option<Region>,
    pub shortcut: Option<Shortcut>,
    pub source: Option<Source>,
    pub description: Option<Box<[u8]>>,
    pub placeholder: Option<Box<[u8]>>,
    pub entry: Option<EntryKind>,
    pub focus: Option<u64>,
    pub count: Option<u32>,
    pub position: Option<u32>,
    pub spans: Option<Spans>,
    pub sort: Option<Sort>,
    /// The shell asks the view root for this node's address when this is set.
    pub linked: bool,
    pub external: Option<Box<[u8]>>,
    pub bound: bool,
    pub drawing: Option<Drawing>,
}

impl Attribute {
    fn of(node: &Node) -> Attribute {
        Attribute {
            role: node.role,
            name: node.name.clone(),
            states: node.states.clone(),
            styles: node.styles.clone(),
            region: node.region,
            shortcut: node.shortcut,
            source: node.source.clone(),
            description: node.description.clone(),
            placeholder: node.placeholder.clone(),
            entry: node.entry,
            focus: node.focus,
            count: node.count,
            position: node.position,
            spans: node.spans,
            sort: node.sort,
            linked: node.link.is_some(),
            external: node.external.clone(),
            bound: node.binding.is_some() || node.link.is_some(),
            drawing: node.drawing.clone(),
        }
    }
}

/// An inserted node in preorder; its owner's private tokens are absent.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Made {
    pub id: NodeId,
    pub size: u32,
    pub attribute: Attribute,
    pub text: Box<[TextRun]>,
    pub value: Option<Value>,
}

impl Made {
    /// The platform-visible projection of a semantic node.
    #[must_use]
    pub fn of(node: &Node) -> Made {
        Made {
            id: node.id,
            size: node.size,
            attribute: Attribute::of(node),
            text: node.text.to_boxed(),
            value: node.value.clone(),
        }
    }
}

/// An edit to a shell tree; identity zero names the shell's outer container.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Patch {
    /// Insert a whole preorder subtree under a parent, before a sibling or at its end.
    Insert { parent: NodeId, before: Option<NodeId>, nodes: Box<[Made]> },
    /// Remove a node and its descendants.
    Remove { node: NodeId },
    /// Position a matched sibling and its subtree before another sibling or at the end.
    Move { node: NodeId, parent: NodeId, before: Option<NodeId> },
    /// Replace all runs on a text node.
    Text { node: NodeId, text: Box<[TextRun]> },
    /// Replace semantic attributes, including a drawing's whole data.
    Attribute { node: NodeId, attribute: Box<Attribute> },
    /// Apply a owner write to a field, or remove its value.
    Value { node: NodeId, value: Option<Value> },
}

#[derive(Debug)]
pub(crate) struct Open {
    index: u32,
    end: u32,
}

fn parents(tree: &Tree, depth: u32) -> List<Option<u32>> {
    if let Some(root) = tree.get(0) {
        assert!(root.size == tree.len(), "a displayed tree has one root");
    }
    let mut result = List::with_capacity(tree.len());
    let mut open: Stack<Open> = Stack::with_capacity(depth);
    for index in 0..tree.len() {
        for _level in 0..open.len() {
            if index < open.top().expect("ancestor remains").end {
                break;
            }
            let _closed = open.pop().expect("an open ancestor remains");
        }
        let mut parent = None;
        if let Some(ancestor) = open.top() {
            parent = Some(ancestor.index);
        }
        result.push(parent).expect("one parent per node");
        let size = tree.get(index).expect("preorder node exists").size;
        let end = index.checked_add(size).expect("tree extent fits u32");
        assert!(size > 0 && end <= tree.len(), "subtree is contained in the tree");
        if let Some(ancestor) = open.top() {
            assert!(end <= ancestor.end, "subtree is contained in its parent");
        }
        open.push(Open { index, end }).expect("tree fits its depth capacity");
    }
    result
}

fn old_match(
    old: &Tree,
    parent: u32,
    node: &Node,
    parents: &List<Option<u32>>,
    used: &List<Option<u32>>,
) -> Option<u32> {
    for index in parent.checked_add(1)?..parent.checked_add(old.get(parent)?.size)? {
        if parents.get(index)? != &Some(parent) {
            continue;
        }
        let candidate = old.get(index)?;
        let same = match node.key {
            Some(key) => candidate.key == Some(key),
            None => candidate.key.is_none() && candidate.role == node.role,
        };
        if used.get(index)?.is_none() && same {
            return Some(index);
        }
    }
    None
}

fn next_sibling(tree: &Tree, parents: &List<Option<u32>>, index: u32) -> Option<u32> {
    let next = index.checked_add(tree.get(index)?.size)?;
    if parents.get(next)? == parents.get(index)? { Some(next) } else { None }
}

fn later_id(tree: &Tree, parents: &List<Option<u32>>, index: u32) -> Option<NodeId> {
    Some(tree.get(next_sibling(tree, parents, index)?)?.id)
}

fn subtree(tree: &Tree, start: u32) -> Box<[Made]> {
    let size = tree.get(start).expect("subtree root exists").size;
    let mut made = List::with_capacity(size);
    for index in start..start.checked_add(size).expect("subtree extent fits u32") {
        let node = Made::of(tree.get(index).expect("subtree node exists"));
        made.push(node).expect("subtree fits measured size");
    }
    made.into_boxed()
}

#[derive(Debug)]
struct Matches {
    old_parents: List<Option<u32>>,
    new_parents: List<Option<u32>>,
    old_to_new: List<Option<u32>>,
    new_to_old: List<Option<u32>>,
}

fn match_nodes(old: &Tree, new: &mut Tree, limits: &Limits, next_id: &mut u32) -> Matches {
    let old_parents = parents(old, limits.depth);
    let new_parents = parents(new, limits.depth);
    let mut old_to_new = List::with_capacity(old.len());
    for _index in 0..old.len() {
        old_to_new.push(None).expect("one match per old node");
    }
    let mut new_to_old = List::with_capacity(new.len());
    for _index in 0..new.len() {
        new_to_old.push(None).expect("one match per new node");
    }
    for index in 0..new.len() {
        let matched = if index == 0 {
            if old.is_empty() { None } else { Some(0) }
        } else {
            let parent = new_parents.get(index).copied().flatten().expect("nonroot has a parent");
            match new_to_old.get(parent).copied().flatten() {
                Some(parent) => {
                    old_match(old, parent, new.get(index).expect("new node exists"), &old_parents, &old_to_new)
                }
                None => None,
            }
        };
        let id = match matched {
            Some(old_index) => {
                *old_to_new.get_mut(old_index).expect("old match slot exists") = Some(index);
                *new_to_old.get_mut(index).expect("new match slot exists") = Some(old_index);
                old.get(old_index).expect("matched old node exists").id
            }
            None => {
                *next_id = next_id.checked_add(1).expect("node identities never wrap");
                NodeId(*next_id)
            }
        };
        new.nodes.get_mut(index).expect("new node exists").id = id;
    }
    Matches { old_parents, new_parents, old_to_new, new_to_old }
}

fn removes(old: &Tree, matches: &Matches, out: &mut Queue<Patch>) {
    for index in 0..old.len() {
        if matches.old_to_new.get(index).copied().flatten().is_some() {
            continue;
        }
        let parent = matches.old_parents.get(index).copied().flatten();
        let remove = match parent {
            Some(parent) => matches.old_to_new.get(parent).copied().flatten().is_some(),
            None => true,
        };
        if remove {
            out.push(Patch::Remove { node: old.get(index).expect("old node exists").id });
        }
    }
}

fn inserts(new: &Tree, matches: &Matches, out: &mut Queue<Patch>) {
    for index in (0..new.len()).rev() {
        if matches.new_to_old.get(index).copied().flatten().is_some() {
            continue;
        }
        let parent = matches.new_parents.get(index).copied().flatten();
        let parent_id = match parent {
            Some(parent) => {
                if matches.new_to_old.get(parent).copied().flatten().is_none() {
                    continue;
                }
                new.get(parent).expect("parent exists").id
            }
            None => NodeId(0),
        };
        let nodes = subtree(new, index);
        let before = later_id(new, &matches.new_parents, index);
        out.push(Patch::Insert { parent: parent_id, before, nodes });
    }
}

fn attributes(old: &Node, new: &Node, out: &mut Queue<Patch>) {
    let was_bound = old.binding.is_some() || old.link.is_some();
    let bound = new.binding.is_some() || new.link.is_some();
    if old.role != new.role
        || old.name != new.name
        || old.states != new.states
        || old.styles != new.styles
        || old.region != new.region
        || old.shortcut != new.shortcut
        || old.source != new.source
        || old.description != new.description
        || old.placeholder != new.placeholder
        || old.entry != new.entry
        || old.focus != new.focus
        || old.count != new.count
        || old.position != new.position
        || old.spans != new.spans
        || old.sort != new.sort
        || old.link != new.link
        || old.external != new.external
        || was_bound != bound
        || old.drawing != new.drawing
    {
        let attribute = Box::new(Attribute::of(new));
        out.push(Patch::Attribute { node: new.id, attribute });
    }
    if old.text != new.text {
        let text = new.text.to_boxed();
        out.push(Patch::Text { node: new.id, text });
    }
    let changed = match &new.value {
        Some(value) => match &old.value {
            Some(old) => old.written != value.written,
            None => true,
        },
        None => old.value.is_some(),
    };
    if changed {
        out.push(Patch::Value { node: new.id, value: new.value.clone() });
    }
}

fn moves(new: &Tree, matches: &Matches, out: &mut Queue<Patch>) {
    let mut children = List::with_capacity(new.len());
    for parent in 0..new.len() {
        if matches.new_to_old.get(parent).copied().flatten().is_none() {
            continue;
        }
        children.clear();
        let mut last_old = None;
        let mut reordered = false;
        for child in parent.checked_add(1).expect("child index fits u32")
            ..parent.checked_add(new.get(parent).expect("parent exists").size).expect("parent extent fits u32")
        {
            if matches.new_parents.get(child).copied().flatten() != Some(parent) {
                continue;
            }
            if let Some(old) = matches.new_to_old.get(child).copied().flatten() {
                if let Some(last) = last_old
                    && old < last
                {
                    reordered = true;
                }
                last_old = Some(old);
                children.push(child).expect("matched children fit tree capacity");
            }
        }
        if !reordered {
            continue;
        }
        let parent = new.get(parent).expect("parent exists").id;
        for index in (0..children.len()).rev() {
            let child = *children.get(index).expect("matched child exists");
            let node = new.get(child).expect("matched node exists").id;
            let mut before = None;
            for sibling in child.checked_add(1).expect("sibling index fits u32")..new.len() {
                if matches.new_parents.get(sibling).copied().flatten()
                    == matches.new_parents.get(child).copied().flatten()
                    && matches.new_to_old.get(sibling).copied().flatten().is_some()
                {
                    before = Some(new.get(sibling).expect("later matched sibling exists").id);
                    break;
                }
            }
            out.push(Patch::Move { node, parent, before });
        }
    }
}

/// Assign stable identities and emit edits. The caller reserves `limits.patches`
/// output slots; each tree is complete and fits the same configured limits.
pub fn diff(old: &Tree, new: &mut Tree, limits: &Limits, next_id: &mut u32, out: &mut Queue<Patch>) {
    assert!(old.len() <= limits.nodes && new.len() <= limits.nodes, "both trees fit node capacity");
    assert!(out.room() >= limits.patches, "the caller reserves the patch capacity");
    for node in old.nodes() {
        assert!(node.id.0 > 0 && node.id.0 <= *next_id, "old identities came from this diff sequence");
    }
    let start = out.len();
    let matches = match_nodes(old, new, limits, next_id);
    removes(old, &matches, out);
    moves(new, &matches, out);
    inserts(new, &matches, out);
    for index in 0..new.len() {
        if let Some(old_index) = matches.new_to_old.get(index).copied().flatten() {
            attributes(old.get(old_index).expect("matched old node"), new.get(index).expect("matched new node"), out);
        }
    }
    let emitted = out.len().checked_sub(start).expect("patch count only grows");
    assert!(emitted <= limits.patches, "diff fits the configured patch capacity");
}
