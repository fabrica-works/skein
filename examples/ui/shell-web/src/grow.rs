//! The example's one component keeps the native text area's semantics.
use skein_ui::{Act, Made};
use skein_ui_web::page::web::Component;
use std::rc::Rc;
use wasm_bindgen::{JsCast, closure::Closure, prelude::wasm_bindgen};
use web_sys::{HtmlElement, HtmlTextAreaElement};

#[wasm_bindgen(module = "/grow.js")]
extern "C" {
    fn grow(element: &HtmlElement);
}

type Listener = (&'static str, Closure<dyn FnMut(web_sys::Event)>);
#[derive(Default)]
struct Grow {
    area: Option<HtmlTextAreaElement>,
    listeners: Vec<Listener>,
    written: Option<u64>,
}
#[must_use]
pub fn new() -> Box<dyn Component> {
    Box::new(Grow::default())
}
impl Component for Grow {
    fn insert(&mut self, host: &HtmlElement, made: &Made, report: Rc<dyn Fn(Act)>) {
        let area = host.clone().unchecked_into::<HtmlTextAreaElement>();
        for kind in ["input", "compositionend"] {
            let (target, report) = (area.clone(), Rc::clone(&report));
            let listener = Closure::new(move |event: web_sys::Event| {
                if event.dyn_ref::<web_sys::InputEvent>().is_some_and(web_sys::InputEvent::is_composing) {
                    return;
                }
                grow(target.unchecked_ref());
                report(Act::Input { value: target.value().into_bytes().into_boxed_slice() });
            });
            host.add_event_listener_with_callback(kind, listener.as_ref().unchecked_ref()).expect("input listener");
            self.listeners.push((kind, listener));
        }
        self.area = Some(area);
        self.patch(made);
    }
    fn patch(&mut self, made: &Made) {
        let area = self.area.as_ref().expect("component inserted");
        if let Some(value) = &made.value
            && self.written != Some(value.written)
        {
            area.set_value(&String::from_utf8_lossy(&value.text));
            self.written = Some(value.written);
        }
        grow(area.unchecked_ref());
    }
    fn remove(&mut self) {
        if let Some(area) = self.area.take() {
            for (kind, listener) in &self.listeners {
                area.remove_event_listener_with_callback(kind, listener.as_ref().unchecked_ref())
                    .expect("input listener");
            }
        }
        self.listeners.clear();
    }
}
