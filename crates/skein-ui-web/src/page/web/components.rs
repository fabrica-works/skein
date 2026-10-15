//! The application owns a component's contents and listeners until removal.
use super::Mounted;
use skein_ui::{Act, Made, NodeId};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

pub trait Component {
    fn insert(&mut self, host: &HtmlElement, made: &Made, report: Rc<dyn Fn(Act)>);
    fn patch(&mut self, made: &Made);
    fn remove(&mut self);
}
pub type Constructor = fn() -> Box<dyn Component>;
pub(super) struct Instance {
    index: usize,
    component: Box<dyn Component>,
}
impl Mounted {
    pub(super) fn component(&self, node: NodeId, index: usize, made: &Made) {
        let changed = self.components.borrow().get(&node).is_some_and(|old| old.index != index);
        if changed {
            self.remove_component(node);
        }
        if let Some(instance) = self.components.borrow_mut().get_mut(&node) {
            instance.component.patch(made);
            return;
        }
        let host = self.group(node).element.unchecked_into::<HtmlElement>();
        let mut component = self.constructors[index]();
        let report = Rc::clone(&self.report);
        component.insert(&host, made, Rc::new(move |act| report(node, act)));
        self.components.borrow_mut().insert(node, Instance { index, component });
    }
    pub(super) fn remove_component(&self, node: NodeId) {
        if let Some(mut instance) = self.components.borrow_mut().remove(&node) {
            instance.component.remove();
        }
    }
    pub(super) fn remove_components(&self) {
        for (_, mut instance) in std::mem::take(&mut *self.components.borrow_mut()) {
            instance.component.remove();
        }
    }
}
