//! Patches applied in place, with the host deciding each document operation.

use std::collections::BTreeMap;

use skein_lib::List;
use skein_ui::{
    Act, Limits, Made, Modifier, NodeId, Patch, Role, State,
    scene::{Point, Scene},
};

#[cfg(target_arch = "wasm32")]
pub mod web;

use crate::{
    Look,
    dom::{self, Child, Element},
    mapping::{self, Primary},
    svg,
};

/// The platform half replays these operations in order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Op {
    Insert { node: NodeId, parent: NodeId, before: Option<NodeId>, element: Element },
    Remove { node: NodeId },
    Move { node: NodeId, parent: NodeId, before: Option<NodeId> },
    Replace { node: NodeId, element: Element },
    Attributes { node: NodeId, element: Element },
    Text { node: NodeId, element: Element },
    Value { node: NodeId, value: Box<[u8]> },
    Choice { node: NodeId, checked: bool },
    Href { node: NodeId, path: Box<[u8]> },
    Focus { node: NodeId },
    ScrollEnd { node: NodeId },
}

struct Held {
    made: Made,
    parent: NodeId,
    children: Vec<NodeId>,
    element: Element,
    size: Option<(u16, u16)>,
    drawn: Option<Element>,
    href: Option<Box<[u8]>>,
    follow_log: bool,
}

/// The semantic nodes and their document model, sharing the view's limits.
pub struct Page<'a> {
    look: Look<'a>,
    prefix: &'a [u8],
    primary: Primary,
    nodes: BTreeMap<NodeId, Held>,
    roots: Vec<NodeId>,
    links: Vec<NodeId>,
    focused: Option<NodeId>,
    scene: Scene,
    limit: u32,
    act_bytes: u32,
    menu: Option<NodeId>,
    composition_end: Option<(NodeId, Box<[u8]>)>,
}

impl<'a> Page<'a> {
    #[must_use]
    pub fn new(look: Look<'a>, limits: &Limits, prefix: &'a [u8], primary: Primary) -> Self {
        Self {
            look,
            prefix,
            primary,
            nodes: BTreeMap::new(),
            roots: Vec::new(),
            links: Vec::new(),
            focused: None,
            scene: Scene::new(limits),
            limit: limits.nodes,
            act_bytes: limits.drawing_bytes,
            menu: None,
            composition_end: None,
        }
    }

    pub fn apply(&mut self, patches: &[Patch], ops: &mut Vec<Op>) {
        let mut focus = None;
        for patch in patches {
            match patch {
                Patch::Insert { nodes, .. } => {
                    for made in nodes {
                        if made.attribute.focus.is_some() {
                            focus = Some(made.id);
                        }
                    }
                }
                Patch::Attribute { node, attribute } => {
                    if attribute.focus.is_some() && attribute.focus != self.held(*node).made.attribute.focus {
                        focus = Some(*node);
                    }
                }
                Patch::Remove { .. } | Patch::Move { .. } | Patch::Text { .. } | Patch::Value { .. } => {}
            }
            match patch {
                Patch::Insert { parent, before, nodes } => self.insert(*parent, *before, nodes, ops),
                Patch::Remove { node } => self.remove(*node, ops),
                Patch::Move { node, parent, before } => {
                    let old_parent = self.held(*node).parent;
                    self.detach(*node, old_parent);
                    self.attach(*node, *parent, *before);
                    self.nodes.get_mut(node).expect("held node").parent = *parent;
                    ops.push(Op::Move { node: *node, parent: *parent, before: *before });
                }
                Patch::Text { node, text } => {
                    self.held(*node);
                    self.nodes.get_mut(node).expect("held node").made.text.clone_from(text);
                    self.remap(*node);
                    ops.push(Op::Text { node: *node, element: self.own_element(*node) });
                }
                Patch::Attribute { node, attribute } => {
                    let old_tag = semantic(&self.held(*node).element).tag.clone();
                    self.nodes.get_mut(node).expect("held node").made.attribute = attribute.clone();
                    self.remap(*node);
                    self.draw(*node);
                    let element = self.own_element(*node);
                    if semantic(&element).tag == old_tag {
                        ops.push(Op::Attributes { node: *node, element });
                    } else {
                        ops.push(Op::Replace { node: *node, element });
                    }
                }
                Patch::Value { node, value } => {
                    self.held(*node);
                    self.nodes.get_mut(node).expect("held node").made.value.clone_from(value);
                    self.remap(*node);
                    let value = value.as_ref().map_or_else(|| Vec::new().into_boxed_slice(), |v| v.text.clone());
                    ops.push(Op::Value { node: *node, value });
                }
            }
        }
        self.links.clear();
        let mut order = Vec::new();
        for root in &self.roots {
            self.preorder(*root, &mut order);
        }
        for node in order {
            let a = &self.held(node).made.attribute;
            if a.linked && a.external.is_none() {
                self.links.push(node);
            }
        }
        if !patches.is_empty() {
            for (node, held) in &self.nodes {
                if held.made.attribute.role == Role::Log && held.follow_log {
                    ops.push(Op::ScrollEnd { node: *node });
                }
            }
        }
        if let Some(node) = focus.filter(|node| self.nodes.contains_key(node)) {
            self.focused = Some(node);
            ops.push(Op::Focus { node });
        }
    }

    /// The mount's focus listener reports the nearest semantic node.
    pub fn focused(&mut self, node: NodeId) {
        self.held(node);
        self.focused = Some(node);
    }

    pub fn resized(&mut self, node: NodeId, width: u16, height: u16, ops: &mut Vec<Op>) {
        self.held(node);
        let held = self.nodes.get_mut(&node).expect("held node");
        if held.made.attribute.drawing.is_none() {
            return;
        }
        if held.size == Some((width, height)) {
            return;
        }
        held.size = Some((width, height));
        self.draw(node);
        ops.push(Op::Attributes { node, element: self.own_element(node) });
    }

    #[must_use]
    pub fn html(&self) -> String {
        let mut outer = Element::new("");
        for root in &self.roots {
            outer.append(self.render(*root));
        }
        dom::print(&outer)
    }

    #[must_use]
    pub fn links(&self) -> &[NodeId] {
        &self.links
    }

    pub fn set_href(&mut self, node: NodeId, path: &[u8], ops: &mut Vec<Op>) {
        self.held(node);
        self.nodes.get_mut(&node).expect("held node").href = Some(path.into());
        self.remap(node);
        ops.push(Op::Href { node, path: path.into() });
    }

    fn held(&self, node: NodeId) -> &Held {
        self.nodes.get(&node).expect("patch names a held node")
    }

    fn siblings(&mut self, parent: NodeId) -> &mut Vec<NodeId> {
        if parent == NodeId(0) {
            &mut self.roots
        } else {
            self.held(parent);
            &mut self.nodes.get_mut(&parent).expect("held parent").children
        }
    }

    fn attach(&mut self, node: NodeId, parent: NodeId, before: Option<NodeId>) {
        let siblings = self.siblings(parent);
        let index = before.map_or(siblings.len(), |before| {
            siblings.iter().position(|sibling| *sibling == before).expect("patch names a held sibling")
        });
        siblings.insert(index, node);
    }

    fn detach(&mut self, node: NodeId, parent: NodeId) {
        let siblings = self.siblings(parent);
        let index = siblings.iter().position(|sibling| *sibling == node).expect("held child has its parent");
        siblings.remove(index);
    }

    fn insert(&mut self, parent: NodeId, before: Option<NodeId>, nodes: &[Made], ops: &mut Vec<Op>) {
        assert!(!nodes.is_empty(), "insert holds a subtree");
        let mut open = Vec::new();
        for (index, made) in nodes.iter().enumerate() {
            assert!(
                self.nodes.len() < usize::try_from(self.limit).expect("node limit fits usize"),
                "page fits the view's node limit"
            );
            while open.last().is_some_and(|(_, end)| index >= *end) {
                open.pop();
            }
            let node_parent = open.last().map_or(parent, |(node, _)| *node);
            let element = mapping::element(made, &self.look, self.prefix, self.primary);
            let held = Held {
                made: made.clone(),
                parent: node_parent,
                children: Vec::new(),
                element,
                size: None,
                drawn: None,
                href: None,
                follow_log: true,
            };
            assert!(self.nodes.insert(made.id, held).is_none(), "inserted node has a fresh identity");
            self.attach(made.id, node_parent, if index == 0 { before } else { None });
            let end = index + usize::try_from(made.size).expect("subtree size fits usize");
            assert!(made.size > 0 && end <= nodes.len(), "insert subtree is contained");
            open.push((made.id, end));
        }
        let node = nodes[0].id;
        ops.push(Op::Insert { node, parent, before, element: self.render(node) });
    }

    fn preorder(&self, node: NodeId, out: &mut Vec<NodeId>) {
        out.push(node);
        for child in &self.held(node).children {
            self.preorder(*child, out);
        }
    }

    fn remove(&mut self, node: NodeId, ops: &mut Vec<Op>) {
        let parent = self.held(node).parent;
        let mut removed = Vec::new();
        self.preorder(node, &mut removed);
        let took_focus = self.focused.is_some_and(|focus| removed.contains(&focus));
        let mut ancestor = parent;
        let mut focus = None;
        if took_focus {
            while ancestor != NodeId(0) {
                let held = self.held(ancestor);
                if focusable(held.made.attribute.role) {
                    focus = Some(ancestor);
                    break;
                }
                ancestor = held.parent;
            }
        }
        self.detach(node, parent);
        if self.menu.is_some_and(|menu| removed.contains(&menu)) {
            self.menu = None;
        }
        for node in removed {
            self.nodes.remove(&node);
        }
        ops.push(Op::Remove { node });
        if took_focus {
            if focus.is_none() {
                let mut order = Vec::new();
                for root in &self.roots {
                    self.preorder(*root, &mut order);
                }
                focus = order.into_iter().find(|node| self.held(*node).made.attribute.role == Role::Main);
            }
            self.focused = focus;
            if let Some(node) = focus {
                ops.push(Op::Focus { node });
            }
        }
    }

    fn remap(&mut self, node: NodeId) {
        let held = self.nodes.get_mut(&node).expect("held node");
        held.element = mapping::element(&held.made, &self.look, self.prefix, self.primary);
        if held.made.attribute.role == Role::Menu {
            menu_element(&mut held.element, self.menu == Some(node));
        }
        if held.made.attribute.linked
            && held.made.attribute.external.is_none()
            && let Some(path) = &held.href
        {
            semantic_mut(&mut held.element).set("href", &String::from_utf8_lossy(path));
        }
    }

    fn draw(&mut self, node: NodeId) {
        let held = self.nodes.get_mut(&node).expect("held node");
        held.drawn = None;
        if let Some(drawing) = &held.made.attribute.drawing
            && let Some((width, height)) = held.size
        {
            self.scene.clear();
            if (self.look.draw)(drawing.kind, &drawing.data, width, height, &mut self.scene) && !self.scene.refused() {
                held.drawn = Some(svg::draw(&self.scene, self.look.names, width, height));
            }
        }
    }

    fn own_element(&self, node: NodeId) -> Element {
        let held = self.held(node);
        let mut element = held.element.clone();
        if let Some(svg) = &held.drawn {
            let core = semantic_mut(&mut element);
            core.set("data-drawn", "");
            core.children.insert(0, Child::Element(svg.clone()));
        }
        element
    }

    fn render(&self, node: NodeId) -> Element {
        let mut element = self.own_element(node);
        let core = semantic_mut(&mut element);
        let destination = if core.tag == "pre" {
            match core.children.first_mut().expect("code has a code element") {
                Child::Element(contents) => contents,
                Child::Text(_) => unreachable!("code is an element"),
            }
        } else {
            core
        };
        let description = if [Role::Item, Role::Cell].contains(&self.held(node).made.attribute.role)
            && self.held(node).made.attribute.description.is_some()
        {
            destination.children.pop()
        } else {
            None
        };
        for child in &self.held(node).children {
            destination.append(self.render(*child));
        }
        if let Some(description) = description {
            destination.children.push(description);
        }
        element
    }
}

fn semantic(element: &Element) -> &Element {
    if element.tag.is_empty() {
        for child in &element.children {
            if let Child::Element(child) = child
                && child.attribute("data-node").is_some()
            {
                return child;
            }
        }
        if let Some(Child::Element(first)) = element.children.first() {
            return first;
        }
        unreachable!("node fragment has its semantic element");
    }
    element
}

pub(crate) fn semantic_mut(element: &mut Element) -> &mut Element {
    if element.tag.is_empty() {
        let index = element
            .children
            .iter()
            .position(|child| match child {
                Child::Element(child) => child.attribute("data-node").is_some(),
                Child::Text(_) => false,
            })
            .unwrap_or(0);
        match element.children.get_mut(index).expect("fragment has its semantic element") {
            Child::Element(child) => return child,
            Child::Text(_) => unreachable!("semantic element is an element"),
        }
    }
    element
}

fn focusable(role: Role) -> bool {
    [
        Role::Button,
        Role::Link,
        Role::TextField,
        Role::TextArea,
        Role::Choice,
        Role::Check,
        Role::Dialog,
        Role::Tab,
        Role::Main,
    ]
    .contains(&role)
}

/// Platform events as data; shortcut interpretation belongs to the page.
#[derive(Clone, Debug)]
pub enum Event {
    Click { node: NodeId, modifier: bool },
    Input { node: NodeId, value: Box<[u8]>, composing: bool },
    CompositionEnd { node: NodeId, value: Box<[u8]> },
    Enter { node: NodeId, shift: bool, composing: bool },
    Change { node: NodeId },
    Cancel { dialog: NodeId },
    Closed { dialog: NodeId },
    Focused { node: NodeId },
    Scrolled { node: NodeId, top: i32, height: i32, viewport: i32 },
    Drawing { node: NodeId, under: Box<[u32]>, x: u16, y: u16 },
    Key { key: Box<[u8]>, code: Box<[u8]>, control: bool, meta: bool, alt: bool, shift: bool, in_field: bool },
}

impl Page<'_> {
    pub fn act(&mut self, event: Event, ops: &mut Vec<Op>) -> Option<(NodeId, Act)> {
        match event {
            Event::Click { node, modifier } => self.click(node, modifier, ops),
            Event::Input { node, value, composing } => {
                if self.composition_end.take().is_some_and(|(ended, text)| ended == node && text == value) {
                    return None;
                }
                let a = &self.held(node).made.attribute;
                if !composing && enabled(a) && a.bound && [Role::TextField, Role::TextArea].contains(&a.role) {
                    Some((node, Act::Input { value }))
                } else {
                    None
                }
            }
            Event::CompositionEnd { node, value } => {
                self.composition_end = Some((node, value.clone()));
                let a = &self.held(node).made.attribute;
                if enabled(a) && a.bound && [Role::TextField, Role::TextArea].contains(&a.role) {
                    Some((node, Act::Input { value }))
                } else {
                    None
                }
            }
            Event::Enter { node, shift, composing } => {
                self.composition_end = None;
                let a = &self.held(node).made.attribute;
                if !composing
                    && enabled(a)
                    && a.bound
                    && a.states.as_slice().contains(&State::SubmitsOnEnter)
                    && (a.role == Role::TextField || (a.role == Role::TextArea && !shift))
                {
                    Some((node, Act::Submit))
                } else {
                    None
                }
            }
            Event::Change { node } => {
                let a = &self.held(node).made.attribute;
                if enabled(a) && a.bound && [Role::Choice, Role::Check].contains(&a.role) {
                    let state = if a.role == Role::Choice { State::Selected } else { State::Pressed };
                    ops.push(Op::Choice { node, checked: a.states.as_slice().contains(&state) });
                    Some((node, Act::Toggle))
                } else {
                    None
                }
            }
            Event::Cancel { dialog } | Event::Closed { dialog } => {
                self.held(dialog);
                ops.push(Op::Attributes { node: dialog, element: self.own_element(dialog) });
                None
            }
            Event::Focused { node } => {
                self.focused(node);
                None
            }
            Event::Scrolled { node, top, height, viewport } => {
                self.held(node);
                self.nodes.get_mut(&node).expect("held node").follow_log =
                    top.saturating_add(viewport) >= height.saturating_sub(1);
                None
            }
            Event::Drawing { node, under, x, y } => {
                let a = &self.held(node).made.attribute;
                if !enabled(a) || !a.bound || a.role != Role::Drawing || self.held(node).size.is_none() {
                    return None;
                }
                self.draw(node);
                self.held(node).drawn.as_ref()?;
                let mut data = List::with_capacity(self.act_bytes);
                if !self.scene.refused() && self.scene.act_at(&under, Point { x, y }, &mut data) {
                    Some((node, Act::Drawing { data: data.into_boxed() }))
                } else {
                    None
                }
            }
            event @ Event::Key { .. } => self.key(event, ops),
        }
    }

    fn click(&mut self, node: NodeId, modifier: bool, ops: &mut Vec<Op>) -> Option<(NodeId, Act)> {
        let a = &self.held(node).made.attribute;
        if !enabled(a) {
            return None;
        }
        if a.role == Role::Menu {
            let open = self.menu != Some(node);
            self.close_menu(ops);
            if open {
                self.menu = Some(node);
                menu_element(&mut self.nodes.get_mut(&node).expect("held menu").element, true);
                ops.push(Op::Attributes { node, element: self.own_element(node) });
                if let Some(item) = self.menu_items(node).first() {
                    ops.push(Op::Focus { node: *item });
                }
            }
            return None;
        }
        if a.role == Role::Link && (modifier || a.external.is_some()) {
            return None;
        }
        if a.bound && [Role::Button, Role::Link, Role::Tab, Role::MenuItem].contains(&a.role) {
            if a.role == Role::MenuItem {
                self.close_menu(ops);
            }
            Some((node, Act::Press))
        } else {
            None
        }
    }

    fn key(&mut self, event: Event, ops: &mut Vec<Op>) -> Option<(NodeId, Act)> {
        let Event::Key { key, code, control, meta, alt, shift, in_field } = event else { unreachable!("key event") };
        if shift {
            return None;
        }
        let modifier = if !control && !meta && !alt {
            Some(Modifier::Plain)
        } else if alt && !control && !meta {
            Some(Modifier::Alt)
        } else if !alt
            && match self.primary {
                Primary::Control => control && !meta,
                Primary::Meta => meta && !control,
            }
        {
            Some(Modifier::Primary)
        } else {
            None
        }?;
        if modifier == Modifier::Plain && self.menu_key(&key, ops) {
            return None;
        }
        if in_field && modifier == Modifier::Plain {
            return None;
        }
        let key = shortcut_key(&key, &code)?;
        let mut order = Vec::new();
        for root in &self.roots {
            self.preorder(*root, &mut order);
        }
        for node in order {
            let a = &self.held(node).made.attribute;
            if enabled(a) && a.bound && a.shortcut == Some(skein_ui::Shortcut { key, modifier }) {
                return Some((node, Act::Press));
            }
        }
        None
    }

    fn menu_items(&self, node: NodeId) -> Vec<NodeId> {
        let mut order = Vec::new();
        self.preorder(node, &mut order);
        order
            .into_iter()
            .filter(|node| {
                let a = &self.held(*node).made.attribute;
                a.role == Role::MenuItem && enabled(a)
            })
            .collect()
    }

    fn menu_key(&mut self, key: &[u8], ops: &mut Vec<Op>) -> bool {
        if self.menu.is_none()
            && key == b"ArrowDown"
            && let Some(node) = self.focused
            && self.held(node).made.attribute.role == Role::Menu
        {
            self.click(node, false, ops);
            return true;
        }
        let Some(menu) = self.menu else {
            return false;
        };
        if key == b"Escape" {
            self.close_menu(ops);
            return true;
        }
        let items = self.menu_items(menu);
        if items.is_empty() {
            return false;
        }
        let current = items.iter().position(|node| Some(*node) == self.focused);
        let index = match key {
            b"ArrowDown" => current.map_or(0, |n| (n + 1) % items.len()),
            b"ArrowUp" => current.map_or(items.len() - 1, |n| (n + items.len() - 1) % items.len()),
            b"Home" => 0,
            b"End" => items.len() - 1,
            _ => return false,
        };
        ops.push(Op::Focus { node: items[index] });
        true
    }

    fn close_menu(&mut self, ops: &mut Vec<Op>) {
        if let Some(node) = self.menu.take() {
            menu_element(&mut self.nodes.get_mut(&node).expect("held menu").element, false);
            ops.push(Op::Attributes { node, element: self.own_element(node) });
            ops.push(Op::Focus { node });
        }
    }
}

fn enabled(attribute: &skein_ui::Attribute) -> bool {
    !attribute.states.as_slice().contains(&State::Disabled)
}
fn shortcut_key(key: &[u8], code: &[u8]) -> Option<u8> {
    if key.len() == 1 && key[0].is_ascii_alphanumeric() {
        return Some(key[0].to_ascii_lowercase());
    }
    let letter = code.strip_prefix(b"Key").or_else(|| code.strip_prefix(b"Digit"))?;
    if letter.len() == 1 && letter[0].is_ascii_alphanumeric() { Some(letter[0].to_ascii_lowercase()) } else { None }
}
fn menu_element(element: &mut Element, open: bool) {
    if let Some(Child::Element(button)) = element.children.first_mut() {
        button.set("aria-expanded", if open { "true" } else { "false" });
    }
    let menu = semantic_mut(element);
    if open {
        menu.remove("hidden");
    } else {
        menu.set("hidden", "");
    }
}
