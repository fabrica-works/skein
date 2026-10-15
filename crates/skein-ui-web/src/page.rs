//! Patches applied in place, with the host deciding each document operation.

use std::collections::BTreeMap;

use skein_ui::{Limits, Made, NodeId, Patch, Role, scene::Scene};

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
}

struct Held {
    made: Made,
    parent: NodeId,
    children: Vec<NodeId>,
    element: Element,
    size: Option<(u16, u16)>,
    drawn: Option<Element>,
    href: Option<Box<[u8]>>,
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
        }
    }

    pub fn apply(&mut self, patches: &[Patch], ops: &mut Vec<Op>) {
        for patch in patches {
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
    }

    /// The mount's focus listener reports the nearest semantic node.
    pub fn focused(&mut self, node: NodeId) {
        self.held(node);
        self.focused = Some(node);
    }

    pub fn resized(&mut self, node: NodeId, width: u16, height: u16, ops: &mut Vec<Op>) {
        self.held(node);
        let held = self.nodes.get_mut(&node).expect("held node");
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
            let actual_parent = open.last().map_or(parent, |(node, _)| *node);
            let element = mapping::element(made, &self.look, self.prefix, self.primary);
            let held = Held {
                made: made.clone(),
                parent: actual_parent,
                children: Vec::new(),
                element,
                size: None,
                drawn: None,
                href: None,
            };
            assert!(self.nodes.insert(made.id, held).is_none(), "inserted node has a fresh identity");
            self.attach(made.id, actual_parent, if index == 0 { before } else { None });
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
        for child in &self.held(node).children {
            destination.append(self.render(*child));
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
        unreachable!("node fragment has its semantic element");
    }
    element
}

fn semantic_mut(element: &mut Element) -> &mut Element {
    if element.tag.is_empty() {
        for child in &mut element.children {
            if let Child::Element(child) = child
                && child.attribute("data-node").is_some()
            {
                return child;
            }
        }
        unreachable!("node fragment has its semantic element");
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
