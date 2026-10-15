//! One node per call with bounded open parents (skein's ui.md, section 5).
//! `open` enters a parent, `close` records its subtree size, and `finish` hands
//! over a complete tree. Attributes change the innermost open node. Capacity
//! errors assert before allocating bytes: the caller's counts must cover all
//! page nodes, fallback children and fragment nodes.

use crate::Binding;
use crate::Limits;
use crate::tree::length;
use crate::{
    Drawing, DrawingKind, EntryKind, Key, Node, Picture, Region, Role, Run, Shortcut, SlotRef, Sort, Source, Spans,
    State, Style, TextRun, Tree, Value,
};

use alloc::boxed::Box;
use core::mem::replace;
use skein_lib::{List, Stack, bytes::copy_of};

/// A fragment position's identity during one build.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SlotId(pub u32);

/// An unfilled fragment request, in preorder within one splice pass.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Slot {
    pub id: SlotId,
    pub reference: SlotRef,
    /// Zero for a page's slot, one for a fragment's slot, two left loading.
    pub depth: u8,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PlacedSlot {
    slot: Slot,
    index: u32,
}

/// A tree under construction and its bounded parent stack.
#[derive(Debug)]
pub struct Builder {
    pub(crate) tree: Tree,
    pub(crate) limits: Limits,
    open: Stack<u32>,
    text_bytes: u32,
    bindings: u32,
    focused: bool,
    slots: List<PlacedSlot>,
    next_slot: u32,
    spare: List<Node>,
}

impl Builder {
    /// Reserve storage for a tree whose counts the caller has computed.
    #[must_use]
    pub fn new(limits: &Limits) -> Builder {
        Builder {
            tree: Tree::new(limits.nodes),
            limits: *limits,
            open: Stack::with_capacity(limits.depth),
            text_bytes: 0,
            bindings: 0,
            focused: false,
            slots: List::with_capacity(limits.slots),
            next_slot: 0,
            spare: List::with_capacity(limits.nodes),
        }
    }

    /// Empty a spare tree while retaining its node allocation.
    #[must_use]
    pub fn reuse(mut tree: Tree, limits: &Limits) -> Builder {
        assert!(tree.nodes.capacity() == limits.nodes, "spare tree has the configured capacity");
        tree.nodes.clear();
        Builder {
            tree,
            limits: *limits,
            open: Stack::with_capacity(limits.depth),
            text_bytes: 0,
            bindings: 0,
            focused: false,
            slots: List::with_capacity(limits.slots),
            next_slot: 0,
            spare: List::with_capacity(limits.nodes),
        }
    }

    /// Begin a node; calls between this and `close` build its children.
    pub fn open(&mut self, role: Role) {
        let runs = if role == Role::Text { self.limits.text_per_node } else { 0 };
        self.open_with_runs(role, runs);
    }

    fn open_with_runs(&mut self, role: Role, runs: u32) {
        assert!(self.tree.nodes.room() > 0, "tree fits its node capacity");
        assert!(self.open.len() < self.limits.depth, "tree fits its depth capacity");
        let index = self.tree.len();
        let node = Node::new(role, &self.limits, runs);
        self.tree.nodes.push(node).expect("tree fits its node capacity");
        self.open.push(index).expect("tree fits its depth capacity");
    }

    /// End the innermost node and record its full subtree's size.
    pub fn close(&mut self) {
        let index = self.open.pop().expect("a node is open");
        let size = self.tree.len().checked_sub(index).expect("node precedes its children");
        self.tree.nodes.get_mut(index).expect("open node exists").size = size;
    }

    fn index(&self) -> u32 {
        *self.open.top().expect("an attribute has an open node")
    }
    fn node(&self) -> &Node {
        self.tree.get(self.index()).expect("open node exists")
    }
    fn node_mut(&mut self) -> &mut Node {
        let index = self.index();
        self.tree.nodes.get_mut(index).expect("open node exists")
    }

    fn charge(&mut self, old: u32, new: u32) {
        let node_bytes = self
            .node()
            .text_bytes()
            .checked_sub(old)
            .expect("old bytes are held")
            .checked_add(new)
            .expect("node bytes fit u32");
        assert!(node_bytes <= self.limits.text_per_node, "node fits its text capacity");
        let total =
            self.text_bytes.checked_sub(old).expect("old bytes are held").checked_add(new).expect("tree bytes fit u32");
        assert!(total <= self.limits.text_bytes, "tree fits its text capacity");
        self.text_bytes = total;
    }

    /// Build a text leaf with one run.
    pub fn text(&mut self, run: Run, bytes: &[u8]) {
        self.open_with_runs(Role::Text, 1);
        self.run(run, bytes);
        self.close();
    }

    /// Append a nonempty run to an open text node; empty spans take no storage.
    pub fn run(&mut self, run: Run, bytes: &[u8]) {
        assert!(self.node().role == Role::Text, "runs belong to text nodes");
        if bytes.is_empty() {
            return;
        }
        self.charge(0, length(bytes));
        let text = copy_of(bytes);
        self.node_mut().text.push(TextRun { run, text }).expect("nonempty runs fit the text capacity");
    }

    /// Give the open node a name where its text does not name it.
    pub fn name(&mut self, bytes: &[u8]) {
        let old = match &self.node().name {
            Some(name) => length(name),
            None => 0,
        };
        self.charge(old, length(bytes));
        let name = copy_of(bytes);
        self.node_mut().name = Some(name);
    }

    /// Describe the open node beyond its name, within its text bound.
    pub fn description(&mut self, bytes: &[u8]) {
        let old = match &self.node().description {
            Some(description) => length(description),
            None => 0,
        };
        self.charge(old, length(bytes));
        let description = copy_of(bytes);
        self.node_mut().description = Some(description);
    }
    /// Offer the keyboard or control for a text field.
    pub fn entry(&mut self, kind: EntryKind) {
        assert!(self.node().role == Role::TextField, "entry kinds belong to text fields");
        self.node_mut().entry = Some(kind);
    }
    /// Request focus after the shell applies the turn's patches.
    pub fn focus(&mut self, count: u64) {
        assert!(!self.focused, "at most one focus request per tree");
        self.node_mut().focus = Some(count);
        self.focused = true;
    }

    /// Count the whole collection when only part of it is shown.
    pub fn count(&mut self, count: u32) {
        assert!([Role::List, Role::Table, Role::Tree].contains(&self.node().role), "counts belong to collections");
        self.node_mut().count = Some(count);
    }
    /// Place an item in its whole collection, starting at one.
    pub fn position(&mut self, position: u32) {
        assert!(
            [Role::Item, Role::Row, Role::TreeItem].contains(&self.node().role),
            "positions belong to items or rows"
        );
        assert!(position > 0, "positions start at one");
        self.node_mut().position = Some(position);
    }
    /// Span rows and columns from a cell.
    pub fn spans(&mut self, spans: Spans) {
        assert!(
            [Role::Cell, Role::ColumnHeader, Role::RowHeader].contains(&self.node().role),
            "spans belong to cells or headers"
        );
        assert!(spans.rows > 0 && spans.columns > 0, "spans start at one");
        self.node_mut().spans = Some(spans);
    }

    /// Give a column header its order.
    pub fn sort(&mut self, sort: Sort) {
        assert!(self.node().role == Role::ColumnHeader, "sort belongs to column headers");
        self.node_mut().sort = Some(sort);
    }

    /// Add a semantic state once.
    pub fn state(&mut self, state: State) {
        if state == State::Required {
            assert!(
                [Role::TextField, Role::TextArea, Role::Select, Role::Check, Role::ChoiceGroup]
                    .contains(&self.node().role),
                "required belongs to fields"
            );
        }
        if !self.node().states.as_slice().contains(&state) {
            self.node_mut().states.push(state).expect("nine closed states fit");
        }
    }

    /// Add a style token once, within the configured count.
    pub fn style(&mut self, style: Style) {
        if !self.node().styles.as_slice().contains(&style) {
            self.node_mut().styles.push(style).expect("styles fit their capacity");
        }
    }

    pub fn region(&mut self, region: Region) {
        assert!(
            [
                Role::Banner,
                Role::Navigation,
                Role::Main,
                Role::Complementary,
                Role::ContentInfo,
                Role::Form,
                Role::Search,
                Role::Region,
                Role::Dialog,
                Role::Tabs,
                Role::Log
            ]
            .contains(&self.node().role),
            "region names belong to landmarks, regions, dialogs, tabs or logs"
        );
        self.node_mut().region = Some(region);
    }
    pub fn shortcut(&mut self, shortcut: Shortcut) {
        assert!(
            shortcut.key.is_ascii_lowercase() || shortcut.key.is_ascii_digit() || shortcut == Shortcut::ESCAPE,
            "shortcut keys are letters or digits, or Escape"
        );
        assert!(self.node().binding.is_some() || self.node().link.is_some(), "shortcut nodes are bound");
        self.node_mut().shortcut = Some(shortcut);
    }
    pub fn picture(&mut self, picture: Picture) {
        assert!(self.node().role == Role::Image, "pictures belong to images");
        let old = self.image_bytes();
        self.charge(old, 0);
        self.node_mut().source = Some(Source::Picture(picture));
    }

    /// Set a checked path or https image address, charged as text bytes.
    pub fn image_address(&mut self, address: &[u8]) {
        assert!(self.node().role == Role::Image, "image addresses belong to images");
        assert!(crate::markdown::safe_image_address(address), "image address is a checked path or https URL");
        let old = self.image_bytes();
        self.charge(old, length(address));
        let address = copy_of(address);
        self.node_mut().source = Some(Source::Address(address));
    }
    fn image_bytes(&self) -> u32 {
        match &self.node().source {
            Some(source) => match source {
                Source::Picture(_) => 0,
                Source::Address(address) => length(address),
            },
            None => 0,
        }
    }

    /// Identify a list item by its object.
    pub fn key(&mut self, key: Key) {
        assert!(
            self.node().role == Role::Item || self.node().role == Role::TreeItem,
            "keys belong to list or tree items"
        );
        self.node_mut().key = Some(key);
    }

    /// Set a field's value together with the owner's own-write count.
    pub fn value(&mut self, bytes: &[u8], written: u64) {
        assert!(self.node().role == Role::TextField || self.node().role == Role::TextArea, "values belong to fields");
        let old = match &self.node().value {
            Some(value) => length(&value.text),
            None => 0,
        };
        self.charge(old, length(bytes));
        let text = copy_of(bytes);
        self.node_mut().value = Some(Value { text, written });
    }

    /// Name a link's address in its owner's vocabulary.
    pub fn link(&mut self, to: Binding) {
        assert!(self.node().role == Role::Link, "addresses belong to links");
        assert!(self.node().external.is_none(), "a link has one address");
        if self.node().link.is_none() && self.node().binding.is_none() {
            self.reserve_binding();
        }
        self.node_mut().link = Some(to);
    }

    /// Give a Markdown link its checked http or https target.
    pub fn external(&mut self, to: &[u8]) {
        assert!(self.node().role == Role::Link, "external addresses belong to links");
        assert!(self.node().link.is_none(), "a link has one address");
        assert!(crate::markdown::safe_url(to), "external target is checked http or https");
        let old = match &self.node().external {
            Some(bytes) => length(bytes),
            None => 0,
        };
        self.charge(old, length(to));
        let target = copy_of(to);
        self.node_mut().external = Some(target);
    }

    /// Bind an actable node to its owner's token.
    pub fn bind(&mut self, binding: Binding) {
        assert!(self.node().role != Role::Drawing, "a drawing is shown and never bound");
        assert!(acted_on(self.node().role), "a binding belongs to a role a person acts on");
        if self.node().binding.is_none() && self.node().link.is_none() {
            self.reserve_binding();
        }
        self.node_mut().binding = Some(binding);
    }

    fn reserve_binding(&mut self) {
        assert!(self.bindings < self.limits.bindings, "bindings fit their capacity");
        self.bindings = self.bindings.checked_add(1).expect("binding count fits u32");
    }

    /// Open a drawing; its following child nodes are the fallback in words.
    /// Both the owner's declared byte bound and the tree's bound are checked.
    pub fn drawing(&mut self, kind: DrawingKind, name: &[u8], data: &[u8], owner_bound: u32) {
        let bytes = length(data);
        assert!(bytes <= owner_bound && bytes <= self.limits.drawing_bytes, "drawing fits its owner's byte bound");
        self.open(Role::Drawing);
        self.name(name);
        let data = copy_of(data);
        self.node_mut().drawing = Some(Drawing { kind, data });
    }

    /// Leave an owner reference with a named busy fallback. The caller supplies
    /// the phrasebook's loading words, so the builder chooses no language.
    pub fn slot(&mut self, reference: SlotRef, loading_name: &[u8]) {
        assert!(self.next_slot < self.limits.slots, "slots fit their capacity");
        let index = self.tree.len();
        let id = SlotId(self.next_slot);
        self.next_slot = self.next_slot.checked_add(1).expect("slot identities fit u32");
        self.open(Role::Slot);
        self.node_mut().slot = Some(reference);
        self.open(Role::Status);
        self.name(loading_name);
        self.state(State::Busy);
        self.close();
        self.close();
        let slot = Slot { id, reference, depth: 0 };
        self.slots.push(PlacedSlot { slot, index }).expect("slots fit their capacity");
    }

    /// Snapshot the unfilled slots for pass zero or one; pass two remains
    /// visible as loading. Identities survive earlier insertions in a pass.
    #[must_use]
    pub fn slots(&self, pass: u8) -> Box<[Slot]> {
        assert!(pass <= 2, "slots nest at most two deep");
        let mut found = List::with_capacity(self.limits.slots);
        for index in 0..self.tree.len() {
            for placed in &self.slots {
                if placed.index == index && placed.slot.depth == pass {
                    found.push(placed.slot).expect("snapshot fits slot capacity");
                }
            }
        }
        found.into_boxed()
    }

    /// Replace a slot's loading fallback with a fragment's ordinary nodes.
    /// A fragment may leave slots for the next pass; a third fill asserts.
    pub fn splice(&mut self, id: SlotId, mut fragment: Builder) {
        assert!(self.open.is_empty() && fragment.open.is_empty(), "splice only complete trees");
        assert!(self.limits == fragment.limits, "fragments use the whole tree limits");
        assert!(fragment.next_slot == fragment.slots.len(), "fragments are built before their slots are filled");
        assert!(!(self.focused && fragment.focused), "at most one focus request per tree");
        self.focused = self.focused || fragment.focused;
        let mut target = None;
        for placed in &self.slots {
            if placed.slot.id == id {
                target = Some(*placed);
                break;
            }
        }
        let target = target.expect("slot is still unfilled");
        let index = target.index;
        let depth = target.slot.depth;
        assert!(depth < 2, "slots nest at most two deep");
        let removed = self.tree.get(index).expect("slot exists").size.checked_sub(1).expect("slot includes itself");
        let end = index.checked_add(removed).expect("slot end fits u32");
        let end = end.checked_add(1).expect("slot end fits u32");
        let count = self
            .tree
            .len()
            .checked_sub(removed)
            .expect("fallback is in tree")
            .checked_add(fragment.tree.len())
            .expect("tree count fits u32");
        assert!(count <= self.limits.nodes, "spliced tree fits its node capacity");
        let slots = self.next_slot.checked_add(fragment.next_slot).expect("slot count fits u32");
        assert!(slots <= self.limits.slots, "spliced slots fit their capacity");
        let bindings = self.bindings.checked_add(fragment.bindings).expect("binding count fits u32");
        assert!(bindings <= self.limits.bindings, "spliced bindings fit their capacity");
        let mut old_bytes = 0_u32;
        for at in index.checked_add(1).expect("slot child index fits u32")..end {
            old_bytes = old_bytes
                .checked_add(self.tree.get(at).expect("fallback node exists").text_bytes())
                .expect("text count fits u32");
        }
        let bytes = self
            .text_bytes
            .checked_sub(old_bytes)
            .expect("fallback bytes are held")
            .checked_add(fragment.text_bytes)
            .expect("text count fits u32");
        assert!(bytes <= self.limits.text_bytes, "spliced tree fits its text capacity");
        let added = fragment.tree.len();
        for at in 0..self.tree.len() {
            let old = self.tree.nodes.get_mut(at).expect("preorder node exists");
            let mut node = replace(old, Node::vacant());
            if at < index {
                let subtree_end = at.checked_add(node.size).expect("subtree end fits u32");
                if subtree_end > index {
                    node.size = resized(node.size, removed, added);
                }
            }
            if at == index {
                node.size = added.checked_add(1).expect("slot size fits u32");
                self.spare.push(node).expect("spliced tree fits its node capacity");
                for child_at in 0..fragment.tree.len() {
                    let child = fragment.tree.nodes.get_mut(child_at).expect("fragment node exists");
                    let child = replace(child, Node::vacant());
                    self.spare.push(child).expect("spliced tree fits its node capacity");
                }
            } else if at < index || at >= end {
                self.spare.push(node).expect("spliced tree fits its node capacity");
            }
        }
        self.tree.nodes.clear();
        let built = replace(&mut self.spare, List::with_capacity(0));
        let empty = replace(&mut self.tree.nodes, built);
        self.spare = empty;
        let old_slots = replace(&mut self.slots, List::with_capacity(self.limits.slots));
        for mut placed in old_slots.into_boxed() {
            if placed.slot.id != id {
                if placed.index >= end {
                    placed.index = resized(placed.index, removed, added);
                }
                self.slots.push(placed).expect("spliced slots fit their capacity");
            }
        }
        for placed in fragment.slots.into_boxed() {
            let slot_id = SlotId(self.next_slot);
            self.next_slot = self.next_slot.checked_add(1).expect("slot identities fit u32");
            let reference = placed.slot.reference;
            let nested = depth.checked_add(1).expect("slot depth fits u8");
            let slot = Slot { id: slot_id, reference, depth: nested };
            let child_index = index
                .checked_add(1)
                .expect("slot child fits u32")
                .checked_add(placed.index)
                .expect("slot index fits u32");
            self.slots.push(PlacedSlot { slot, index: child_index }).expect("spliced slots fit their capacity");
        }
        self.text_bytes = bytes;
        self.bindings = bindings;
        self.check_depth();
    }

    /// Show the owner's explanation when it cannot show the requested part.
    pub fn unshowable(&mut self, id: SlotId, name: &[u8]) {
        let mut fragment = Builder::new(&self.limits);
        fragment.open(Role::Status);
        fragment.name(name);
        fragment.close();
        self.splice(id, fragment);
    }

    fn check_depth(&self) {
        let mut ends = Stack::with_capacity(self.limits.depth);
        for index in 0..self.tree.len() {
            for _level in 0..ends.len() {
                let end = *ends.top().expect("ancestor remains");
                if index < end {
                    break;
                }
                let popped = ends.pop().expect("closed ancestor remains");
                assert!(popped == end, "pop removes the top ancestor");
            }
            let size = self.tree.get(index).expect("node exists").size;
            let end = index.checked_add(size).expect("subtree end fits u32");
            ends.push(end).expect("spliced tree fits its depth capacity");
        }
    }

    /// Hand over a tree only after every open node has been closed.
    #[must_use]
    pub fn finish(self) -> Tree {
        assert!(self.open.is_empty(), "all tree nodes are closed");
        self.tree
    }
}

fn resized(value: u32, removed: u32, added: u32) -> u32 {
    value
        .checked_sub(removed)
        .expect("old subtree includes removed children")
        .checked_add(added)
        .expect("spliced index fits u32")
}

// skein's ui.md, section 6: every bound node is one a person acts on.
fn acted_on(role: Role) -> bool {
    match role {
        Role::Button
        | Role::Link
        | Role::TextField
        | Role::TextArea
        | Role::Choice
        | Role::Check
        | Role::Tab
        | Role::Option
        | Role::MenuItem => true,
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
        | Role::ColumnHeader
        | Role::RowHeader
        | Role::Select
        | Role::Term
        | Role::Description
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
        | Role::Disclosure
        | Role::Image
        | Role::Figure
        | Role::Slot
        | Role::Drawing => false,
    }
}
