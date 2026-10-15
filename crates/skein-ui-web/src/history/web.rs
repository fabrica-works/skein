//! Browser history owns paths and entry scroll (ui-web.md, 9).
use crate::inbox::Inbox;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

pub struct History {
    model: Rc<RefCell<super::History>>,
    _pop: Closure<dyn FnMut(web_sys::PopStateEvent)>,
    _scroll: Closure<dyn FnMut(web_sys::Event)>,
}
fn save(window: &web_sys::Window) {
    let state = js_sys::Array::new();
    state.push(&JsValue::from_f64(window.scroll_x().expect("scroll x")));
    state.push(&JsValue::from_f64(window.scroll_y().expect("scroll y")));
    window.history().expect("history").replace_state_with_url(&state, "", None).expect("same origin entry");
}
impl History {
    #[must_use]
    pub fn new<R: 'static>(inbox: Rc<Inbox<R>>, wrap: fn(Box<[u8]>) -> R) -> Self {
        let window = web_sys::window().expect("window");
        window
            .history()
            .expect("history")
            .set_scroll_restoration(web_sys::ScrollRestoration::Manual)
            .expect("manual scroll");
        save(&window);
        let model = Rc::new(RefCell::new(super::History::default()));
        let target = Rc::clone(&model);
        let pop = Closure::new(move |event: web_sys::PopStateEvent| {
            let window = web_sys::window().expect("window");
            let state = js_sys::Array::from(&event.state());
            let scroll = (state.get(0).as_f64().unwrap_or(0.0), state.get(1).as_f64().unwrap_or(0.0));
            let location = window.location();
            let path = location.pathname().expect("path") + &location.search().expect("query");
            let path = target.borrow_mut().gone_to(path.as_bytes(), scroll);
            inbox.raise(wrap(path));
        });
        window.add_event_listener_with_callback("popstate", pop.as_ref().unchecked_ref()).expect("popstate listener");
        let scroll = Closure::new(move |_: web_sys::Event| save(&web_sys::window().expect("window")));
        window.add_event_listener_with_callback("scroll", scroll.as_ref().unchecked_ref()).expect("scroll listener");
        Self { model, _pop: pop, _scroll: scroll }
    }
    pub fn show(&self, path: &[u8], replace: bool) {
        let window = web_sys::window().expect("window");
        save(&window);
        let history = window.history().expect("history");
        let path = String::from_utf8_lossy(path);
        let state = js_sys::Array::of2(&JsValue::from_f64(0.0), &JsValue::from_f64(0.0));
        if replace {
            history.replace_state_with_url(&state, "", Some(&path)).expect("same origin path");
        } else {
            history.push_state_with_url(&state, "", Some(&path)).expect("same origin path");
        }
    }
    pub fn restore(&self) {
        if let Some((x, y)) = self.model.borrow_mut().take_scroll() {
            web_sys::window().expect("window").scroll_to_with_x_and_y(x, y);
        }
    }
}
