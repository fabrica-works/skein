//! The lab's bounded reads of a page's drawing (browser.md, 5.1).
#![expect(clippy::disallowed_types, reason = "the tree traversal uses a stack bounded by the reply's node count")]
use super::{
    Browser, OpKind, Operation, PendingKind, ax_property_bool, ax_states, ax_text, contains, field_array, field_text,
    field_u64, opt_bool, viewport_offset,
};
use crate::boundary::{Entry, Event, Face, Look, Looked, Property, Rect, Refusal};
use crate::{Limits, params::Params, wire::decode::Value};
use alloc::{boxed::Box, vec::Vec};
use skein_lib::{Env, List, Token};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Css {
    Disabled,
    Enabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    Viewport,
    Media,
    Tree,
    SelectRoot,
    Select,
    Selected,
    Enable,
    StyleRoot,
    Push,
    Values,
    Children,
    Texts,
    TextFront,
    Faces,
    Box,
    Metrics,
    Exists,
    ShotMetrics,
    Shot,
}

#[derive(Debug)]
enum TextNode {
    Frontend(u64),
    Backend(u64),
}

#[derive(Debug)]
pub(super) struct State {
    front: u64,
    values: List<Property>,
    faces: List<Face>,
    text_nodes: Vec<TextNode>,
    rect: Option<Rect>,
}
impl State {
    fn new() -> State {
        State {
            front: 0,
            values: List::with_capacity(0),
            faces: List::with_capacity(0),
            text_nodes: Vec::new(),
            rect: None,
        }
    }
}

pub(super) fn waits_for_document(kind: &OpKind) -> bool {
    match kind {
        OpKind::Find { .. }
        | OpKind::Attribute { .. }
        | OpKind::Look { look: Look::Tree | Look::Select { .. } | Look::Style { .. }, .. } => true,
        OpKind::Go(_)
        | OpKind::Press { .. }
        | OpKind::Type { .. }
        | OpKind::Key { .. }
        | OpKind::Address
        | OpKind::Snapshot
        | OpKind::Screenshot
        | OpKind::Look { look: Look::Viewport { .. } | Look::Shot { .. }, .. } => false,
    }
}

impl Browser {
    pub(super) fn request_look(&mut self, env: &Env<Limits>, page: Token, token: Token, look: Look) {
        let limit = usize::try_from(env.limits.text).expect("text fits usize");
        let refusal = match &look {
            Look::Select { selector } if selector.len() > limit => Some(Refusal::Limit),
            Look::Style { properties, .. } => {
                if properties.len() > env.limits.properties {
                    Some(Refusal::Limit)
                } else {
                    let mut why = None;
                    for property in properties {
                        if property.len() > limit {
                            why = Some(Refusal::Limit);
                            break;
                        }
                    }
                    why
                }
            }
            Look::Viewport { width: 0, .. }
            | Look::Viewport { height: 0, .. }
            | Look::Shot { clip: Some(Rect { width: 0, .. }), .. }
            | Look::Shot { clip: Some(Rect { height: 0, .. }), .. } => Some(Refusal::Protocol),
            Look::Shot { scale, .. } if *scale != 1 && *scale != 2 => Some(Refusal::Protocol),
            Look::Tree | Look::Select { .. } | Look::Viewport { .. } | Look::Shot { .. } => None,
        };
        if let Some(why) = refusal {
            self.events.push_back(Event::Refused { op: token, why });
            return;
        }
        if self.admit_op(env, page, token, OpKind::Look { look, state: State::new() }) {
            self.start_look(env, token);
        }
    }

    pub(super) fn start_look(&mut self, env: &Env<Limits>, token: Token) {
        let Some(Operation { page, kind: OpKind::Look { look, .. }, .. }) = self.op(token) else { return };
        let page = *page;
        if waits_for_document(&self.op(token).expect("look exists").kind)
            && let Some(page) = self.page(page)
            && page.navigating
        {
            if let Some(op) = self.op_mut(token) {
                op.due = Some(env.now.saturating_add(env.limits.poll));
            }
            return;
        }
        match look {
            Look::Viewport { width, height, mobile, .. } => {
                let params = Params::Viewport { width: u64::from(*width), height: u64::from(*height), mobile: *mobile };
                self.look_issue(env, token, b"Emulation.setDeviceMetricsOverride", &params, Step::Viewport);
            }
            Look::Tree => self.look_issue(env, token, b"Accessibility.getFullAXTree", &Params::Empty, Step::Tree),
            Look::Select { .. } => self.look_issue(env, token, b"DOM.getDocument", &Params::Root, Step::SelectRoot),
            Look::Style { .. } => {
                if let Some(state) = self.look_state_mut(token) {
                    *state = State::new();
                }
                if let Some(super::Page { css: Css::Enabled, .. }) = self.page(page) {
                    self.look_issue(env, token, b"DOM.getDocument", &Params::Root, Step::StyleRoot);
                } else {
                    for pending in &self.pending {
                        if let PendingKind::Step(other, super::Step::Look(Step::Enable)) = pending.kind
                            && self.op_page(other) == Some(page)
                        {
                            if let Some(op) = self.op_mut(token) {
                                op.due = Some(env.now.saturating_add(env.limits.poll));
                            }
                            return;
                        }
                    }
                    self.look_issue(env, token, b"CSS.enable", &Params::Empty, Step::Enable);
                }
            }
            Look::Shot { clip, scale } => match clip {
                Some(clip) => {
                    let params = Params::Shot { clip: *clip, scale: *scale };
                    self.look_issue(env, token, b"Page.captureScreenshot", &params, Step::Shot);
                }
                None => self.look_issue(env, token, b"Page.getLayoutMetrics", &Params::Empty, Step::ShotMetrics),
            },
        }
    }

    fn look_issue(&mut self, env: &Env<Limits>, token: Token, method: &[u8], params: &Params<'_>, step: Step) {
        self.issue_step(env, token, method, params, super::Step::Look(step));
    }
    fn look_state_mut(&mut self, token: Token) -> Option<&mut State> {
        if let Some(Operation { kind: OpKind::Look { state, .. }, .. }) = self.op_mut(token) {
            Some(state)
        } else {
            None
        }
    }
    fn look_front(&self, token: Token) -> Option<u64> {
        if let Some(Operation { kind: OpKind::Look { state, .. }, .. }) = self.op(token) {
            Some(state.front)
        } else {
            None
        }
    }
    fn looked(&mut self, token: Token, looked: Looked) {
        if self.remove_op(token).is_some() {
            self.events.push_back(Event::Looked { op: token, looked });
        }
    }
    fn style_done(&mut self, token: Token, text: u32) {
        if let Some(Operation { kind: OpKind::Look { mut state, .. }, .. }) = self.remove_op(token) {
            // Merge with whole family names before cutting the boundary's text.
            for index in 0..state.faces.len() {
                let face = state.faces.get_mut(index).expect("face index within length");
                if face.family.len() > usize::try_from(text).expect("text fits usize") {
                    face.family = Box::from(
                        face.family
                            .get(..usize::try_from(text).expect("text fits usize"))
                            .expect("family exceeds text"),
                    );
                }
            }
            self.events.push_back(Event::Looked {
                op: token,
                looked: Looked::Style { values: state.values, faces: state.faces, rect: state.rect },
            });
        }
    }
    fn next_face(&mut self, env: &Env<Limits>, token: Token) -> Result<(), Refusal> {
        let state = self.look_state_mut(token).ok_or(Refusal::Protocol)?;
        match state.text_nodes.pop() {
            Some(TextNode::Frontend(node)) => {
                self.look_issue(env, token, b"CSS.getPlatformFontsForNode", &Params::Node(node), Step::Faces);
            }
            Some(TextNode::Backend(node)) => self.look_issue(
                env,
                token,
                b"DOM.pushNodesByBackendIdsToFrontend",
                &Params::Push(node),
                Step::TextFront,
            ),
            None => {
                let front = state.front;
                self.look_issue(env, token, b"DOM.getBoxModel", &Params::Node(front), Step::Box);
            }
        }
        Ok(())
    }

    pub(super) fn look_error(&mut self, env: &Env<Limits>, token: Token, step: Step, message: &[u8]) {
        match step {
            Step::Box => {
                if contains(message, b"Could not compute box model") {
                    if let Some(front) = self.look_front(token) {
                        self.look_issue(env, token, b"DOM.describeNode", &Params::Node(front), Step::Exists);
                    }
                } else {
                    let why = if node_missing(message) { Refusal::Gone } else { Refusal::Protocol };
                    self.finish_refused(token, why);
                }
            }
            Step::Push => self.finish_refused(token, Refusal::Gone),
            Step::Values
            | Step::Children
            | Step::Texts
            | Step::TextFront
            | Step::Faces
            | Step::Metrics
            | Step::Exists => {
                let why = if node_missing(message) { Refusal::Gone } else { Refusal::Protocol };
                self.finish_refused(token, why);
            }
            Step::Viewport
            | Step::Media
            | Step::Tree
            | Step::SelectRoot
            | Step::Select
            | Step::Selected
            | Step::Enable
            | Step::StyleRoot
            | Step::ShotMetrics
            | Step::Shot => self.finish_refused(token, Refusal::Protocol),
        }
    }

    pub(super) fn look_reply(&mut self, env: &Env<Limits>, token: Token, step: Step, result: Value<'_>) {
        let answer = self.read_look_reply(env, token, step, result);
        if let Err(why) = answer {
            self.finish_refused(token, why);
        }
    }

    #[expect(clippy::too_many_lines, reason = "each look step names its reply fields and next command together")]
    fn read_look_reply(
        &mut self,
        env: &Env<Limits>,
        token: Token,
        step: Step,
        result: Value<'_>,
    ) -> Result<(), Refusal> {
        // Replies canceled by a navigation or a close cannot advance a read.
        let Some(Operation { kind: OpKind::Look { .. }, .. }) = self.op(token) else { return Ok(()) };
        match step {
            Step::Viewport => {
                if let Some(Operation {
                    kind: OpKind::Look { look: Look::Viewport { scheme, motion, .. }, .. }, ..
                }) = self.op(token)
                {
                    let params = Params::Media { scheme: *scheme, motion: *motion };
                    self.look_issue(env, token, b"Emulation.setEmulatedMedia", &params, Step::Media);
                }
            }
            Step::Media => self.looked(token, Looked::Viewport),
            Step::Tree => self.looked(token, Looked::Tree(tree(result, &env.limits)?)),
            Step::SelectRoot => {
                let root = nested_u64(result, b"root", b"nodeId").ok_or(Refusal::Protocol)?;
                if let Some(Operation { kind: OpKind::Look { look: Look::Select { selector }, .. }, .. }) =
                    self.op(token)
                {
                    let selector = selector.clone();
                    self.look_issue(
                        env,
                        token,
                        b"DOM.querySelector",
                        &Params::Selector { node: root, selector: &selector },
                        Step::Select,
                    );
                }
            }
            Step::Select => {
                let node = field_u64(result, b"nodeId").ok_or(Refusal::Protocol)?;
                if node == 0 {
                    self.looked(token, Looked::Selected(None));
                } else {
                    self.look_issue(env, token, b"DOM.describeNode", &Params::Node(node), Step::Selected);
                }
            }
            Step::Selected => {
                let node = nested_u64(result, b"node", b"backendNodeId").ok_or(Refusal::Protocol)?;
                self.looked(token, Looked::Selected(Some(node)));
            }
            Step::Enable => {
                if let Some(page) = self.op_page(token)
                    && let Some(page) = self.page_mut(page)
                {
                    page.css = Css::Enabled;
                }
                self.look_issue(env, token, b"DOM.getDocument", &Params::Root, Step::StyleRoot);
            }
            Step::StyleRoot => {
                nested_u64(result, b"root", b"nodeId").ok_or(Refusal::Protocol)?;
                if let Some(Operation { kind: OpKind::Look { look: Look::Style { node, .. }, .. }, .. }) =
                    self.op(token)
                {
                    let node = *node;
                    self.look_issue(
                        env,
                        token,
                        b"DOM.pushNodesByBackendIdsToFrontend",
                        &Params::Push(node),
                        Step::Push,
                    );
                }
            }
            Step::Push => {
                let front = first_frontend(result).ok_or(Refusal::Protocol)?;
                if front == 0 {
                    return Err(Refusal::Gone);
                }
                if let Some(state) = self.look_state_mut(token) {
                    state.front = front;
                }
                self.look_issue(env, token, b"CSS.getComputedStyleForNode", &Params::Node(front), Step::Values);
            }
            Step::Values => {
                let computed = field_array(result, b"computedStyle").ok_or(Refusal::Protocol)?;
                if let Some(Operation { kind: OpKind::Look { look: Look::Style { properties, .. }, state }, .. }) =
                    self.op_mut(token)
                {
                    let mut values = List::with_capacity(properties.len());
                    for name in &*properties {
                        let mut value = Box::from(b"".as_slice());
                        for property in computed.clone() {
                            if field_text(property, b"name").as_deref() == Some(name.as_ref()) {
                                value = field_text(property, b"value").ok_or(Refusal::Protocol)?;
                                break;
                            }
                        }
                        if value.len() > usize::try_from(env.limits.text).expect("text fits usize") {
                            return Err(Refusal::Limit);
                        }
                        values.push(Property { name: name.clone(), value }).expect("one value per requested property");
                    }
                    state.values = values;
                }
                let front = self.look_front(token).ok_or(Refusal::Protocol)?;
                self.look_issue(env, token, b"DOM.requestChildNodes", &Params::Children(front), Step::Children);
            }
            Step::Children => {
                if let Some(Operation { kind: OpKind::Look { look: Look::Style { node, .. }, .. }, .. }) =
                    self.op(token)
                {
                    let params = Params::Describe { backend: *node, depth: -1 };
                    // Requesting children assigns frontend ids. Describing then reads
                    // the whole subtree even when Chromium sends no new child event.
                    self.look_issue(env, token, b"DOM.describeNode", &params, Step::Texts);
                }
            }
            Step::Texts => {
                let nodes = text_nodes(result.get(b"node").ok_or(Refusal::Protocol)?)?;
                if let Some(state) = self.look_state_mut(token) {
                    state.text_nodes = nodes;
                    state.faces = List::with_capacity(env.limits.entries);
                }
                self.next_face(env, token)?;
            }
            Step::TextFront => {
                let front = first_frontend(result).ok_or(Refusal::Protocol)?;
                if front == 0 {
                    return Err(Refusal::Gone);
                }
                self.look_issue(env, token, b"CSS.getPlatformFontsForNode", &Params::Node(front), Step::Faces);
            }
            Step::Faces => {
                let fonts = field_array(result, b"fonts").ok_or(Refusal::Protocol)?;
                let state = self.look_state_mut(token).ok_or(Refusal::Protocol)?;
                for font in fonts {
                    let family = field_text(font, b"familyName").ok_or(Refusal::Protocol)?;
                    let custom = opt_bool(font.get(b"isCustomFont")).ok_or(Refusal::Protocol)?;
                    let glyphs = u32::try_from(field_u64(font, b"glyphCount").ok_or(Refusal::Protocol)?)
                        .or(Err(Refusal::Protocol))?;
                    let mut merged = false;
                    for index in 0..state.faces.len() {
                        let face = state.faces.get_mut(index).expect("face index within length");
                        if face.family == family && face.custom == custom {
                            face.glyphs = face.glyphs.checked_add(glyphs).ok_or(Refusal::Protocol)?;
                            merged = true;
                            break;
                        }
                    }
                    if !merged {
                        if state.faces.room() == 0 {
                            return Err(Refusal::Limit);
                        }
                        state.faces.push(Face { family, custom, glyphs }).expect("face room checked");
                    }
                }
                self.next_face(env, token)?;
            }
            Step::Box => {
                let rect = border(result).ok_or(Refusal::Protocol)?;
                if let Some(state) = self.look_state_mut(token) {
                    state.rect = Some(rect);
                }
                self.look_issue(env, token, b"Page.getLayoutMetrics", &Params::Empty, Step::Metrics);
            }
            Step::Metrics => {
                let (x, y) = viewport_offset(result).ok_or(Refusal::Protocol)?;
                if let Some(state) = self.look_state_mut(token)
                    && let Some(rect) = &mut state.rect
                {
                    rect.x = rect.x.checked_add(x).ok_or(Refusal::Protocol)?;
                    rect.y = rect.y.checked_add(y).ok_or(Refusal::Protocol)?;
                }
                let front = self.look_front(token).ok_or(Refusal::Protocol)?;
                self.look_issue(env, token, b"DOM.describeNode", &Params::Node(front), Step::Exists);
            }
            Step::Exists => {
                nested_u64(result, b"node", b"backendNodeId").ok_or(Refusal::Gone)?;
                self.style_done(token, env.limits.text);
            }
            Step::ShotMetrics => {
                let viewport = result.get(b"cssVisualViewport").ok_or(Refusal::Protocol)?;
                let x = pixel(viewport, b"pageX").ok_or(Refusal::Protocol)?;
                let y = pixel(viewport, b"pageY").ok_or(Refusal::Protocol)?;
                let width = pixel_size(viewport, b"clientWidth").ok_or(Refusal::Protocol)?;
                let height = pixel_size(viewport, b"clientHeight").ok_or(Refusal::Protocol)?;
                if width == 0 || height == 0 {
                    return Err(Refusal::Protocol);
                }
                if let Some(Operation { kind: OpKind::Look { look: Look::Shot { scale, .. }, .. }, .. }) =
                    self.op(token)
                {
                    let params = Params::Shot { clip: Rect { x, y, width, height }, scale: *scale };
                    self.look_issue(env, token, b"Page.captureScreenshot", &params, Step::Shot);
                }
            }
            Step::Shot => {
                let data = field_text(result, b"data").ok_or(Refusal::Protocol)?;
                let png =
                    skein_lib::base64::decode(skein_lib::base64::Alphabet::Standard, &data, env.limits.screenshot)
                        .or(Err(Refusal::Limit))?;
                self.looked(token, Looked::Shot(png));
            }
        }
        Ok(())
    }
}

fn text_nodes(root: Value<'_>) -> Result<Vec<TextNode>, Refusal> {
    let mut stack = Vec::new();
    let mut texts = Vec::new();
    stack.push(root);
    // The nested DOM reply owns each node once. Its message bound also
    // bounds this stack and the retained ids; there is no separate walk cap.
    while let Some(node) = stack.pop() {
        if field_u64(node, b"nodeType") == Some(3)
            && !field_text(node, b"nodeValue").ok_or(Refusal::Protocol)?.is_empty()
        {
            let id = field_u64(node, b"nodeId").ok_or(Refusal::Protocol)?;
            // Chromium can leave internal control text without a frontend id.
            // Its backend id can be pushed before asking for its fonts.
            let text = if id == 0 {
                TextNode::Backend(field_u64(node, b"backendNodeId").ok_or(Refusal::Protocol)?)
            } else {
                TextNode::Frontend(id)
            };
            texts.push(text);
        }
        for key in [b"children".as_slice(), b"shadowRoots".as_slice(), b"pseudoElements".as_slice()] {
            if let Some(children) = field_array(node, key) {
                for child in children {
                    stack.push(child);
                }
            }
        }
    }
    Ok(texts)
}

fn border(result: Value<'_>) -> Option<Rect> {
    let mut points = result.get(b"model")?.get(b"border")?.array()?;
    let mut left = i64::MAX;
    let mut top = i64::MAX;
    let mut right = i64::MIN;
    let mut bottom = i64::MIN;
    for _ in 0..4_u8 {
        let x = points.next()?.whole_pixels()?;
        let y = points.next()?.whole_pixels()?;
        left = left.min(x);
        top = top.min(y);
        right = right.max(x);
        bottom = bottom.max(y);
    }
    Some(Rect {
        x: left,
        y: top,
        width: right.checked_sub(left)?.try_into().ok()?,
        height: bottom.checked_sub(top)?.try_into().ok()?,
    })
}

fn tree(result: Value<'_>, limits: &Limits) -> Result<List<Entry>, Refusal> {
    let nodes = field_array(result, b"nodes").ok_or(Refusal::Protocol)?;
    let count = nodes.clone().count();
    let mut entries = List::with_capacity(limits.entries);
    let Some(root) = nodes.clone().next() else { return Ok(entries) };
    let mut stack = Vec::with_capacity(count);
    stack.push((root, 0_u32));
    // Each node is visited at most once in a valid tree; the count also
    // bounds malformed cycles and duplicate children without recursion.
    for _ in 0..count {
        let Some((node, depth)) = stack.pop() else { return Ok(entries) };
        let ignored = opt_bool(node.get(b"ignored")).unwrap_or(false);
        let child_depth = if ignored { depth } else { depth.checked_add(1).ok_or(Refusal::Protocol)? };
        if !ignored {
            if entries.room() == 0 {
                return Err(Refusal::Limit);
            }
            let mut level = 0;
            let mut multiline = false;
            if let Some(properties) = field_array(node, b"properties") {
                for property in properties {
                    if field_text(property, b"name").as_deref() == Some(b"level") {
                        level = level_value(property).unwrap_or(0);
                    }
                    if field_text(property, b"name").as_deref() == Some(b"multiline") {
                        multiline = ax_property_bool(property);
                    }
                }
            }
            entries
                .push(Entry {
                    node: field_u64(node, b"backendDOMNodeId"),
                    depth,
                    role: ax_text(node, b"role", limits.text),
                    name: ax_text(node, b"name", limits.text),
                    level,
                    value: ax_text(node, b"value", limits.text),
                    states: ax_states(node),
                    multiline,
                })
                .expect("entry room checked");
        }
        if let Some(children) = field_array(node, b"childIds") {
            let start = stack.len();
            for id in children {
                let id = id.text().ok_or(Refusal::Protocol)?;
                let mut found = None;
                for candidate in nodes.clone() {
                    if field_text(candidate, b"nodeId").as_deref() == Some(id.as_ref()) {
                        found = Some(candidate);
                        break;
                    }
                }
                if stack.len() >= count {
                    return Err(Refusal::Protocol);
                }
                stack.push((found.ok_or(Refusal::Protocol)?, child_depth));
            }
            stack.get_mut(start..).expect("children appended after start").reverse();
        }
    }
    if stack.is_empty() { Ok(entries) } else { Err(Refusal::Protocol) }
}

fn nested_u64(value: Value<'_>, outer: &[u8], inner: &[u8]) -> Option<u64> {
    field_u64(value.get(outer)?, inner)
}
fn first_frontend(value: Value<'_>) -> Option<u64> {
    field_array(value, b"nodeIds")?.next()?.u64()
}
fn pixel(value: Value<'_>, key: &[u8]) -> Option<i64> {
    value.get(key)?.whole_pixels()
}
fn pixel_size(value: Value<'_>, key: &[u8]) -> Option<u64> {
    pixel(value, key)?.try_into().ok()
}
fn level_value(property: Value<'_>) -> Option<u32> {
    nested_u64(property, b"value", b"value")?.try_into().ok()
}

fn node_missing(message: &[u8]) -> bool {
    contains(message, b"No node found")
        || contains(message, b"Could not find node")
        || contains(message, b"Cannot find node")
        || contains(message, b"No node with given id")
}
