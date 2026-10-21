//! The browser half replays operations and copies events into the pure half.

use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use skein_ui::{Act, NodeId};
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{
    Document, Element, HtmlElement, HtmlInputElement, HtmlOptionElement, HtmlSelectElement, HtmlTextAreaElement,
    KeyboardEvent, MouseEvent, Node, ResizeObserver, ResizeObserverEntry,
};

use super::{Event, Op, Page};
use crate::{
    dom::{Child, Element as Model},
    inbox::Inbox,
};

mod components;
pub use components::{Component, Constructor};

type Listener = (String, Closure<dyn FnMut(web_sys::Event)>);

/// Owns the mount's listeners and resize observer until the page ends.
pub struct Dom {
    document: Rc<Mounted>,
    listeners: Vec<Listener>,
    _resize: Closure<dyn FnMut(js_sys::Array, ResizeObserver)>,
}

impl Dom {
    pub fn new<R: 'static>(
        mount: Element,
        page: &Rc<RefCell<Page<'static>>>,
        inbox: &Rc<Inbox<R>>,
        wrap: fn(NodeId, Act) -> R,
        components: &'static [Constructor],
    ) -> Self {
        let document = Rc::new(Mounted {
            document: mount.owner_document().expect("mount has its document"),
            mount,
            nodes: RefCell::new(BTreeMap::new()),
            observer: RefCell::new(None),
            constructors: components,
            components: RefCell::new(BTreeMap::new()),
            report: {
                let inbox = Rc::clone(inbox);
                Rc::new(move |node, act| inbox.raise(wrap(node, act)))
            },
        });
        let (target, model) = (Rc::clone(&document), Rc::clone(page));
        let resize = Closure::new(move |entries: js_sys::Array, _: ResizeObserver| {
            for entry in entries.iter() {
                let entry: ResizeObserverEntry = entry.unchecked_into();
                if let Some(node) = target.id(&entry.target()) {
                    let size = entry.content_rect();
                    let mut ops = Vec::new();
                    model.borrow_mut().resized(node, pixels(size.width()), pixels(size.height()), &mut ops);
                    target.apply(&ops);
                }
            }
        });
        let observer = ResizeObserver::new(resize.as_ref().unchecked_ref()).expect("resize observer is available");
        *document.observer.borrow_mut() = Some(observer);
        let mut listeners = Vec::new();
        for kind in ["click", "input", "compositionend", "keydown", "change", "cancel", "close", "focusin", "scroll"] {
            let (target, model, reports) = (Rc::clone(&document), Rc::clone(page), Rc::clone(inbox));
            let listener = Closure::new(move |event: web_sys::Event| {
                let Some(element) = event.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
                    return;
                };
                if element.closest("[data-component]").expect("component selector").is_some()
                    && event.type_() != "keydown"
                {
                    return;
                }
                let node = target.nearest(&element);
                let events = read(&event, &element, node, &target);
                for data in events {
                    let mut ops = Vec::new();
                    let act = model.borrow_mut().act(data, &mut ops);
                    if act.is_some() || !ops.is_empty() || event.type_() == "cancel" {
                        event.prevent_default();
                    }
                    target.apply(&ops);
                    if let Some((node, act)) = act {
                        if let Act::Submit = &act {
                            let mut before = Vec::new();
                            let input = model.borrow_mut().act(
                                Event::Input {
                                    node,
                                    value: value(&element).into_bytes().into_boxed_slice(),
                                    composing: false,
                                },
                                &mut before,
                            );
                            target.apply(&before);
                            if let Some((node, input)) = input {
                                reports.raise(wrap(node, input));
                            }
                        }
                        reports.raise(wrap(node, act));
                    }
                }
            });
            document
                .mount
                .add_event_listener_with_callback_and_bool(kind, listener.as_ref().unchecked_ref(), true)
                .expect("mount accepts its listener");
            listeners.push((kind.into(), listener));
        }
        Self { document, listeners, _resize: resize }
    }

    /// Release the page's borrow before replay: focus may report synchronously.
    pub fn apply(&self, ops: &[Op]) {
        self.document.apply(ops);
    }
}

impl Drop for Dom {
    fn drop(&mut self) {
        self.document.remove_components();
        if let Some(observer) = self.document.observer.borrow_mut().take() {
            observer.disconnect();
        }
        for (kind, listener) in &self.listeners {
            self.document
                .mount
                .remove_event_listener_with_callback_and_bool(kind, listener.as_ref().unchecked_ref(), true)
                .expect("registered listener can be removed");
        }
    }
}

#[derive(Clone)]
struct Group {
    element: Element,
    roots: Vec<Node>,
}
struct Mounted {
    document: Document,
    mount: Element,
    nodes: RefCell<BTreeMap<NodeId, Group>>,
    observer: RefCell<Option<ResizeObserver>>,
    constructors: &'static [Constructor],
    components: RefCell<BTreeMap<NodeId, components::Instance>>,
    report: Rc<dyn Fn(NodeId, Act)>,
}

impl Mounted {
    fn group(&self, node: NodeId) -> Group {
        self.nodes.borrow().get(&node).expect("operation names a mounted node").clone()
    }
    fn subtree_group(&self, node: NodeId) -> Group {
        let mut group = self.group(node);
        for child in self.nodes.borrow().values() {
            if group.element.contains(Some(&child.element)) {
                for root in &child.roots {
                    if !group.element.contains(Some(root))
                        && !group.roots.iter().any(|held| held.is_same_node(Some(root)))
                    {
                        group.roots.push(root.clone());
                    }
                }
            }
        }
        group
    }
    fn id(&self, element: &Element) -> Option<NodeId> {
        let node = NodeId(element.get_attribute("data-node")?.parse().ok()?);
        self.nodes.borrow().contains_key(&node).then_some(node)
    }
    fn nearest(&self, element: &Element) -> Option<NodeId> {
        if let Some(button) = element.closest("[aria-controls]").expect("menu button selector")
            && let Some(id) = button.get_attribute("aria-controls")
            && let Some(number) = id.strip_prefix("skein-ui-menu-")
        {
            return number.parse().ok().map(NodeId);
        }
        if let Some(node) = element.closest("[data-node]").expect("node selector").and_then(|e| self.id(&e)) {
            return Some(node);
        }
        self.nodes.borrow().iter().find_map(|(node, group)| group.element.is_same_node(Some(element)).then_some(*node))
    }
    fn make(&self, model: &Model, svg: bool) -> Vec<Node> {
        if model.tag.is_empty() {
            return self.children(&model.children, svg);
        }
        let svg = svg || model.tag == "svg";
        let element = if svg {
            self.document.create_element_ns(Some("http://www.w3.org/2000/svg"), &model.tag)
        } else {
            self.document.create_element(&model.tag)
        }
        .expect("mapping supplies an element name");
        attributes(&element, model, false);
        for child in self.children(&model.children, svg) {
            element.append_child(&child).expect("new element accepts its child");
        }
        vec![element.into()]
    }
    fn children(&self, children: &[Child], svg: bool) -> Vec<Node> {
        children
            .iter()
            .flat_map(|child| match child {
                Child::Element(element) => self.make(element, svg),
                Child::Text(text) => vec![self.document.create_text_node(text).into()],
            })
            .collect()
    }
    fn scan(&self, root: &Element) {
        let elements = root.query_selector_all("[data-node]").expect("node selector");
        for index in 0..elements.length() {
            let element: Element = elements.item(index).expect("selected element").unchecked_into();
            let node = NodeId(
                element
                    .get_attribute("data-node")
                    .expect("selected node attribute")
                    .parse()
                    .expect("node id is decimal"),
            );
            self.register(node, &element);
        }
    }
    fn register(&self, node: NodeId, element: &Element) {
        let mut roots: Vec<Node> = vec![element.clone().into()];
        if let Some(id) = element.get_attribute("id") {
            if id.starts_with("skein-ui-label-") {
                if let Some(label) = element.previous_element_sibling()
                    && label.get_attribute("for").as_ref() == Some(&id)
                {
                    roots.insert(0, label.into());
                } else if let Some(label) = element.next_element_sibling()
                    && label.get_attribute("for").as_ref() == Some(&id)
                {
                    roots.push(label.into());
                }
            }
            if id.starts_with("skein-ui-menu-")
                && let Some(button) = element.previous_element_sibling()
                && button.get_attribute("aria-controls").as_ref() == Some(&id)
            {
                roots.insert(0, button.into());
            }
        }
        if let Some(id) = element.get_attribute("aria-describedby")
            && let Some(description) = self.document.get_element_by_id(&id)
            && !element.contains(Some(&description))
        {
            roots.push(description.into());
        }
        self.nodes.borrow_mut().insert(node, Group { element: element.clone(), roots });
        if element.tag_name().eq_ignore_ascii_case("figure")
            && let Some(observer) = self.observer.borrow().as_ref()
        {
            observer.observe(element);
        }
    }
    fn affected_trees(&self, op: &Op, trees: &mut Vec<NodeId>) {
        let mut affected = Vec::new();
        match op {
            Op::Insert { parent, .. } if *parent != NodeId(0) => {
                affected.push(self.group(*parent).element);
            }
            Op::Move { node, parent, .. } => {
                if *parent != NodeId(0) {
                    affected.push(self.group(*parent).element);
                }
                if let Some(parent) = self
                    .group(*node)
                    .element
                    .parent_element()
                    .and_then(|parent| parent.closest("[data-node]").expect("parent selector"))
                {
                    affected.push(parent);
                }
            }
            Op::Remove { node } | Op::Replace { node, .. } | Op::Attributes { node, .. } => {
                let element = self.group(*node).element;
                affected.push(element.clone());
                if let Some(parent) =
                    element.parent_element().and_then(|parent| parent.closest("[data-node]").expect("parent selector"))
                {
                    affected.push(parent);
                }
            }
            _ => {}
        }
        for element in affected {
            if let Some(node) = self.id(&element)
                && !trees.contains(&node)
            {
                trees.push(node);
            }
        }
    }
    fn apply(&self, ops: &[Op]) {
        let mut trees = Vec::new();
        for op in ops {
            self.affected_trees(op, &mut trees);
            match op {
                Op::Insert { node, parent, before, element } => self.insert(*node, *parent, *before, element),
                Op::Remove { node } => self.remove(*node),
                Op::Move { node, parent, before } => self.move_group(*node, *parent, *before),
                Op::Replace { node, element } => self.refresh(*node, element, true),
                Op::Attributes { node, element } | Op::Text { node, element } => self.refresh(*node, element, false),
                Op::Component { node, component, made } => self.component(*node, *component, made),
                Op::Value { node, value: bytes } => {
                    if self.components.borrow().contains_key(node) {
                        continue;
                    }
                    let element = self.group(*node).element;
                    let text = String::from_utf8_lossy(bytes);
                    if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
                        input.set_value(&text);
                    } else if let Some(area) = element.dyn_ref::<HtmlTextAreaElement>() {
                        area.set_value(&text);
                    }
                }
                Op::Choice { node, checked } => {
                    let element = self.group(*node).element;
                    if let Some(option) = element.dyn_ref::<HtmlOptionElement>() {
                        option.set_selected(*checked);
                    } else {
                        element.unchecked_into::<HtmlInputElement>().set_checked(*checked);
                    }
                }
                Op::Href { node, path } => self
                    .group(*node)
                    .element
                    .set_attribute("href", &String::from_utf8_lossy(path))
                    .expect("href is an attribute"),
                Op::Focus { node } => {
                    let group = self.group(*node);
                    let element = if group.element.get_attribute("role").as_deref() == Some("menu") {
                        group.roots[0].clone().unchecked_into::<Element>()
                    } else {
                        group.element
                    };
                    if !element.has_attribute("tabindex")
                        && !["INPUT", "TEXTAREA", "BUTTON", "A", "SELECT"].contains(&element.tag_name().as_str())
                    {
                        element.set_attribute("tabindex", "-1").expect("focus target accepts tabindex");
                    }
                    element.unchecked_into::<HtmlElement>().focus().expect("focus target is mounted");
                }
                Op::ScrollEnd { node } => {
                    let element = self.group(*node).element;
                    element.set_scroll_top(element.scroll_height());
                }
            }
        }
        for node in trees {
            let tree = self.nodes.borrow().get(&node).map(|group| group.element.clone());
            if let Some(tree) = tree
                && tree.get_attribute("role").as_deref() == Some("treeitem")
            {
                self.tree_groups(&tree);
            }
        }
        let dialogs: Vec<_> = self
            .nodes
            .borrow()
            .values()
            .filter_map(|group| group.element.dyn_ref::<web_sys::HtmlDialogElement>().cloned())
            .collect();
        for dialog in dialogs {
            if !dialog.open() && dialog.is_connected() {
                dialog.show_modal().expect("mounted dialog can be modal");
            }
        }
    }
    fn insert(&self, node: NodeId, parent: NodeId, before: Option<NodeId>, model: &Model) {
        let semantic = super::semantic(model);
        if parent == NodeId(0) && semantic.attribute("data-node").is_none() {
            attributes(&self.mount, semantic, true);
            for child in self.children(&semantic.children, false) {
                self.mount.append_child(&child).expect("mount accepts the page");
            }
            if model.tag.is_empty() {
                for part in model.children.iter().filter_map(|child| match child {
                    Child::Element(part) if part.attribute("id").is_some() => Some(part),
                    Child::Element(_) | Child::Text(_) => None,
                }) {
                    let parent = self.mount.parent_node().expect("mount has a parent");
                    for root in self.make(part, false) {
                        parent
                            .insert_before(&root, self.mount.next_sibling().as_ref())
                            .expect("description follows the mount");
                    }
                }
            }
            self.register(node, &self.mount);
            self.scan(&self.mount);
            return;
        }
        let parent = if parent == NodeId(0) { self.mount.clone() } else { self.destination(parent) };
        let anchor = before.map(|node| self.group(node).roots[0].clone());
        for root in self.make(model, false) {
            let description =
                root.dyn_ref::<Element>().is_some_and(|root| root.id().starts_with("skein-ui-description-"));
            if parent.tag_name() == "SELECT" && description {
                parent
                    .parent_node()
                    .expect("select has a parent")
                    .insert_before(&root, parent.next_sibling().as_ref())
                    .expect("option description follows select");
            } else {
                parent.insert_before(&root, anchor.as_ref()).expect("insert anchor belongs to parent");
            }
        }
        self.scan(&parent);
    }
    fn destination(&self, node: NodeId) -> Element {
        let element = self.group(node).element;
        if element.get_attribute("role").as_deref() == Some("treeitem") {
            let active = self.document.active_element().filter(|active| element.contains(Some(active)));
            let selection = active.as_ref().and_then(selection);
            Self::ungroup_tree(&element);
            if let Some(active) = active {
                active.unchecked_ref::<HtmlElement>().focus().expect("ungrouped focus remains mounted");
                if let Some((start, end, direction)) = selection {
                    restore_selection(&active, start, end, &direction);
                }
            }
        }
        if element.tag_name() == "PRE" {
            element.query_selector("code").expect("code selector").expect("pre holds code")
        } else {
            element
        }
    }
    fn ungroup_tree(element: &Element) {
        let children = element.children();
        let groups: Vec<_> = (0..children.length())
            .filter_map(|index| children.item(index))
            .filter(|child| {
                child.get_attribute("role").as_deref() == Some("group") && !child.has_attribute("data-node")
            })
            .collect();
        for group in groups {
            while let Some(child) = group.first_child() {
                element.insert_before(&child, Some(&group)).expect("tree child stays in order");
            }
            element.remove_child(&group).expect("tree owns its group");
        }
    }
    fn tree_groups(&self, element: &Element) {
        let active = self.document.active_element().filter(|active| element.contains(Some(active)));
        let selection = active.as_ref().and_then(selection);
        Self::ungroup_tree(element);
        let children = element.child_nodes();
        let children: Vec<_> = (0..children.length()).filter_map(|index| children.item(index)).collect();
        let mut wrapper: Option<Element> = None;
        for child in children {
            if !child.parent_node().is_some_and(|parent| parent.is_same_node(Some(element))) {
                continue;
            }
            let item = child.dyn_ref::<Element>().filter(|child| {
                child.get_attribute("role").as_deref() == Some("treeitem") && child.has_attribute("data-node")
            });
            if let Some(item) = item {
                let group = wrapper.get_or_insert_with(|| {
                    let group = self.document.create_element("div").expect("group element");
                    group.set_attribute("role", "group").expect("group role");
                    element.insert_before(&group, Some(&child)).expect("group precedes its run");
                    group
                });
                for root in self.group(self.id(item).expect("tree item is registered")).roots {
                    group.append_child(&root).expect("group accepts tree item and companions");
                }
            } else {
                wrapper = None;
            }
        }
        if let Some(active) = active {
            active.unchecked_ref::<HtmlElement>().focus().expect("grouped focus remains mounted");
            if let Some((start, end, direction)) = selection {
                restore_selection(&active, start, end, &direction);
            }
        }
    }
    fn remove(&self, node: NodeId) {
        let group = self.subtree_group(node);
        let removed: Vec<_> = self
            .nodes
            .borrow()
            .iter()
            .filter_map(|(id, child)| {
                group
                    .roots
                    .iter()
                    .any(|root| root.contains(Some(&child.element)))
                    .then_some((*id, child.element.clone()))
            })
            .collect();
        for (id, element) in removed {
            if let Some(observer) = self.observer.borrow().as_ref() {
                observer.unobserve(&element);
            }
            self.remove_component(id);
            self.nodes.borrow_mut().remove(&id);
        }
        for root in group.roots {
            root.parent_node().expect("removed node has parent").remove_child(&root).expect("held child");
        }
    }
    fn move_group(&self, node: NodeId, parent: NodeId, before: Option<NodeId>) {
        let group = self.subtree_group(node);
        let active = self
            .document
            .active_element()
            .filter(|element| group.roots.iter().any(|root| root.contains(Some(element))));
        let selection = active.as_ref().and_then(selection);
        let parent = self.destination(parent);
        let anchor = before.map(|node| self.group(node).roots[0].clone());
        for root in group.roots {
            let description =
                root.dyn_ref::<Element>().is_some_and(|root| root.id().starts_with("skein-ui-description-"));
            if parent.tag_name() == "SELECT" && description {
                parent
                    .parent_node()
                    .expect("select has a parent")
                    .insert_before(&root, parent.next_sibling().as_ref())
                    .expect("option description follows select");
            } else {
                parent.insert_before(&root, anchor.as_ref()).expect("move anchor belongs to parent");
            }
        }
        if let Some(element) = active {
            element.unchecked_ref::<HtmlElement>().focus().expect("moved focus remains mounted");
            if let Some((start, end, direction)) = selection {
                restore_selection(&element, start, end, &direction);
            }
        }
    }
    fn refresh(&self, node: NodeId, model: &Model, replace: bool) {
        let mut group = self.group(node);
        let own = super::semantic(model);
        if own.attribute("data-component").is_none() {
            self.remove_component(node);
        }
        if replace {
            let replacement: Element = self.make(own, false)[0].clone().unchecked_into();
            let children = self.semantic_children(&group.element);
            let destination = if replacement.tag_name() == "PRE" {
                replacement.query_selector("code").expect("code selector").expect("code destination")
            } else {
                replacement.clone()
            };
            for child in children {
                destination.append_child(&child).expect("replacement accepts semantic children");
            }
            group
                .element
                .parent_node()
                .expect("replacement parent")
                .replace_child(&replacement, &group.element)
                .expect("replace held node");
            let old = group.element.clone();
            for root in &mut group.roots {
                if root.is_same_node(Some(&old)) {
                    *root = replacement.clone().into();
                }
            }
            if let Some(observer) = self.observer.borrow().as_ref() {
                observer.unobserve(&old);
                if replacement.tag_name().eq_ignore_ascii_case("figure") {
                    observer.observe(&replacement);
                }
            }
            group.element = replacement;
            self.nodes.borrow_mut().insert(node, group.clone());
        } else {
            attributes(&group.element, own, group.element.is_same_node(Some(&self.mount)));
            if group.element.tag_name() != "TEXTAREA" && own.attribute("data-component").is_none() {
                self.content(&group.element, own);
            }
        }
        self.companions(node, model);
    }
    fn semantic_children(&self, element: &Element) -> Vec<Node> {
        let mut result = Vec::new();
        let children = element.children();
        for index in 0..children.length() {
            let child = children.item(index).expect("element child");
            if let Some(node) = self.id(&child) {
                result.extend(self.group(node).roots);
            } else {
                result.extend(self.semantic_children(&child));
            }
        }
        result
    }
    fn content(&self, element: &Element, model: &Model) {
        let mut kept = Vec::new();
        let nodes = element.child_nodes();
        let children: Vec<_> = (0..nodes.length()).map(|index| nodes.item(index).expect("child node")).collect();
        for child in children {
            let preserve = child.dyn_ref::<Element>().is_some_and(|e| {
                e.has_attribute("data-node") || e.query_selector("[data-node]").expect("node selector").is_some()
            });
            if preserve {
                kept.push(child);
            } else {
                element.remove_child(&child).expect("own child");
            }
        }
        let mut anchor = kept.first().cloned();
        for child in &model.children {
            if let Child::Element(part) = child
                && let Some(wrapper) = kept.iter().find_map(|node| {
                    node.dyn_ref::<Element>()
                        .filter(|e| !e.has_attribute("data-node") && e.tag_name().eq_ignore_ascii_case(&part.tag))
                })
            {
                self.content(wrapper, part);
                continue;
            }
            for node in self.children(
                std::slice::from_ref(child),
                element.namespace_uri().as_deref() == Some("http://www.w3.org/2000/svg"),
            ) {
                let description =
                    node.dyn_ref::<Element>().is_some_and(|e| e.id().starts_with("skein-ui-description-"));
                let before = if description { None } else { anchor.as_ref() };
                element.insert_before(&node, before).expect("own content surrounds semantic children");
                if !description {
                    anchor = node.next_sibling();
                }
            }
        }
    }
    fn option_description(&self, node: NodeId, model: &Model) {
        let group = self.group(node);
        let parent = group.element.closest("select").expect("select selector").expect("option has a select");
        let container = parent.parent_node().expect("select has a parent");
        for companion in group.roots.iter().skip(1) {
            companion
                .parent_node()
                .expect("description is mounted")
                .remove_child(companion)
                .expect("remove description");
        }
        let mut roots = vec![group.element.clone().into()];
        if model.tag.is_empty() {
            for part in model.children.iter().filter_map(|child| match child {
                Child::Element(part) if part.attribute("data-node").is_none() => Some(part),
                Child::Element(_) | Child::Text(_) => None,
            }) {
                for root in self.make(part, false) {
                    container
                        .insert_before(&root, parent.next_sibling().as_ref())
                        .expect("option description follows select");
                    roots.push(root);
                }
            }
        }
        self.nodes.borrow_mut().get_mut(&node).expect("mounted option").roots = roots;
    }
    fn companions(&self, node: NodeId, model: &Model) {
        let group = self.group(node);
        if group.element.tag_name() == "OPTION" {
            self.option_description(node, model);
            return;
        }
        let parent = group.element.parent_node().expect("mapped node is mounted");
        let mut anchor = group.roots.last().and_then(Node::next_sibling);
        let parts: Vec<&Model> = if model.tag.is_empty() {
            model
                .children
                .iter()
                .filter_map(|c| match c {
                    Child::Element(e) => Some(e),
                    Child::Text(_) => None,
                })
                .collect()
        } else {
            vec![model]
        };
        let mut roots = Vec::new();
        for (index, part) in parts.iter().enumerate().rev() {
            let root: Node = if part.attribute("data-node").is_some() || !model.tag.is_empty() {
                group.element.clone().into()
            } else if let Some(existing) = group
                .roots
                .get(index)
                .and_then(|n| n.dyn_ref::<Element>())
                .filter(|e| !e.is_same_node(Some(&group.element)) && e.tag_name().eq_ignore_ascii_case(&part.tag))
            {
                attributes(existing, part, false);
                self.content(existing, part);
                existing.clone().into()
            } else {
                self.make(part, false)[0].clone()
            };
            if !same(root.next_sibling().as_ref(), anchor.as_ref()) {
                parent.insert_before(&root, anchor.as_ref()).expect("companion belongs beside its node");
            }
            anchor = Some(root.clone());
            roots.insert(0, root);
        }
        for old in &group.roots {
            if !roots.iter().any(|root| root.is_same_node(Some(old))) {
                parent.remove_child(old).expect("old companion");
            }
        }
        self.nodes.borrow_mut().get_mut(&node).expect("mounted node").roots = roots;
    }
}

fn attributes(element: &Element, model: &Model, mount: bool) {
    let old = element.get_attribute_names();
    for name in old.iter().filter_map(|n| n.as_string()) {
        let native_open = name == "open" && ["DIALOG", "DETAILS"].contains(&element.tag_name().as_str());
        if model.attribute(&name).is_none() && !(native_open || name == "tabindex" || (mount && name == "id")) {
            element.remove_attribute(&name).expect("mapped attribute can be removed");
        }
    }
    for (name, value) in &model.attributes {
        if element.get_attribute(name).as_ref() != Some(value) {
            element.set_attribute(name, value).expect("mapped attribute can be set");
        }
    }
    if let Some(option) = element.dyn_ref::<HtmlOptionElement>() {
        option.set_selected(model.attribute("selected").is_some());
    }
    if let Some(input) = element.dyn_ref::<HtmlInputElement>()
        && ["checkbox", "radio"].contains(&input.type_().as_str())
    {
        input.set_checked(model.attribute("checked").is_some());
    }
}
fn same(left: Option<&Node>, right: Option<&Node>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.is_same_node(Some(right)),
        (None, None) => true,
        _ => false,
    }
}
fn pixels(value: f64) -> u16 {
    value.floor().clamp(0.0, f64::from(u16::MAX)).to_string().parse().expect("whole CSS pixels fit u16")
}
fn value(element: &Element) -> String {
    if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
        input.value()
    } else if let Some(area) = element.dyn_ref::<HtmlTextAreaElement>() {
        area.value()
    } else {
        String::new()
    }
}
fn selection(element: &Element) -> Option<(u32, u32, String)> {
    if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
        Some((input.selection_start().ok()??, input.selection_end().ok()??, input.selection_direction().ok()??))
    } else if let Some(area) = element.dyn_ref::<HtmlTextAreaElement>() {
        Some((area.selection_start().ok()??, area.selection_end().ok()??, area.selection_direction().ok()??))
    } else {
        None
    }
}
fn restore_selection(element: &Element, start: u32, end: u32, direction: &str) {
    if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
        input.set_selection_range_with_direction(start, end, direction).expect("moved input preserves its selection");
    }
    if let Some(area) = element.dyn_ref::<HtmlTextAreaElement>() {
        area.set_selection_range_with_direction(start, end, direction).expect("moved area preserves its selection");
    }
}
fn read(event: &web_sys::Event, element: &Element, node: Option<NodeId>, document: &Mounted) -> Vec<Event> {
    let mut out = Vec::new();
    if let Some(key) = event.dyn_ref::<KeyboardEvent>() {
        let in_component = element.closest("[data-component]").expect("component selector").is_some();
        let in_field = in_component
            || ["INPUT", "TEXTAREA", "SELECT"].contains(&element.tag_name().as_str())
            || element.dyn_ref::<HtmlElement>().is_some_and(HtmlElement::is_content_editable);
        out.push(Event::Key {
            key: key.key().into_bytes().into_boxed_slice(),
            code: key.code().into_bytes().into_boxed_slice(),
            control: key.ctrl_key(),
            meta: key.meta_key(),
            alt: key.alt_key(),
            shift: key.shift_key(),
            in_field,
        });
        if in_component {
            return out;
        }
        if key.key() == "Enter"
            && let Some(node) = node
        {
            out.push(Event::Enter { node, shift: key.shift_key(), composing: key.is_composing() });
        }
        return out;
    }
    let Some(node) = node else {
        return out;
    };
    match event.type_().as_str() {
        "click" => {
            let mouse = event.unchecked_ref::<MouseEvent>();
            if element.closest("svg").expect("SVG selector").is_some() {
                out.push(drawing_event(mouse, node, document));
            } else {
                out.push(Event::Click {
                    node,
                    modifier: mouse.ctrl_key()
                        || mouse.meta_key()
                        || mouse.alt_key()
                        || mouse.shift_key()
                        || mouse.button() != 0,
                });
            }
        }
        "input" => out.push(Event::Input {
            node,
            value: value(element).into_bytes().into_boxed_slice(),
            composing: event.dyn_ref::<web_sys::InputEvent>().is_some_and(web_sys::InputEvent::is_composing),
        }),
        "compositionend" => {
            out.push(Event::CompositionEnd { node, value: value(element).into_bytes().into_boxed_slice() });
        }
        "change" => {
            if let Some(select) = element.dyn_ref::<HtmlSelectElement>() {
                if let Ok(index) = u32::try_from(select.selected_index())
                    && let Some(option) = select.options().item(index)
                    && let Some(option) = document.id(&option)
                {
                    out.push(Event::Choose { select: node, option });
                }
            } else {
                out.push(Event::Change { node });
            }
        }
        "cancel" => out.push(Event::Cancel { dialog: node }),
        "close" => out.push(Event::Closed { dialog: node }),
        "focusin" => out.push(Event::Focused { node }),
        "scroll" => out.push(Event::Scrolled {
            node,
            top: element.scroll_top(),
            height: element.scroll_height(),
            viewport: element.client_height(),
        }),
        _ => {}
    }
    out
}
fn drawing_event(mouse: &MouseEvent, node: NodeId, document: &Mounted) -> Event {
    let figure = document.group(node).element;
    let marks = figure
        .query_selector_all("svg g, svg line, svg polyline, svg path, svg rect, svg circle, svg text")
        .expect("mark selector");
    let mut under = Vec::new();
    for element in
        document.document.elements_from_point(coordinate(mouse.client_x()), coordinate(mouse.client_y())).iter()
    {
        let element: Element = element.unchecked_into();
        for index in 0..marks.length() {
            if marks.item(index).expect("mark index").is_same_node(Some(&element)) {
                under.push(index);
                break;
            }
        }
    }
    let bounds = figure.get_bounding_client_rect();
    Event::Drawing {
        node,
        under: under.into_boxed_slice(),
        x: pixels(f64::from(mouse.client_x()) - bounds.left()),
        y: pixels(f64::from(mouse.client_y()) - bounds.top()),
    }
}

fn coordinate(value: i32) -> f32 {
    value.to_string().parse().expect("CSS coordinate is a number")
}
