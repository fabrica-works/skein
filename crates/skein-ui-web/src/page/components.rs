//! Component selection keeps the mapped native element as its stable host.
use super::{Op, Page, semantic_mut};
use crate::{
    Look,
    dom::Element,
    mapping::{self, Primary},
};
use skein_ui::{Made, NodeId};

pub(super) fn index(made: &Made, look: &Look<'_>) -> Option<usize> {
    look.components.iter().position(|(owner, role, style)| {
        style.owner == *owner && made.attribute.role == *role && made.attribute.styles.as_slice().contains(style)
    })
}
pub(super) fn element(made: &Made, look: &Look<'_>, prefix: &[u8], primary: Primary) -> Element {
    let mut element = mapping::element(made, look, prefix, primary);
    if let Some(component) = index(made, look) {
        semantic_mut(&mut element).set("data-component", &component.to_string());
    }
    element
}
impl Page<'_> {
    pub(super) fn component_op(&self, node: NodeId, ops: &mut Vec<Op>) {
        let made = &self.held(node).made;
        if let Some(component) = index(made, &self.look) {
            ops.push(Op::Component { node, component, made: made.clone() });
        }
    }
}
