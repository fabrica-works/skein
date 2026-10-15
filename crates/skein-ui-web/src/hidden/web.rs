//! Visibility and one browser timer drive the pure hidden-tab part.
use crate::{fetch, platform, transport::StreamDuty};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{JsCast, closure::Closure};

pub struct Hidden<R> {
    shared: Rc<Shared<R>>,
    document: web_sys::Document,
    listener: Closure<dyn FnMut(web_sys::Event)>,
}
struct Shared<R> {
    hidden: RefCell<super::Hidden>,
    fetch: fetch::web::Fetch<R>,
    timer: Cell<Option<i32>>,
    timeout: RefCell<Option<Closure<dyn FnMut()>>>,
}
impl<R: 'static> Hidden<R> {
    #[must_use]
    pub fn new(fetch: fetch::web::Fetch<R>) -> Self {
        let document = web_sys::window().expect("browser window").document().expect("browser document");
        let shared = Rc::new(Shared {
            hidden: RefCell::new(super::Hidden::new(skein_lib::Duration::from_secs(60))),
            fetch,
            timer: Cell::new(None),
            timeout: RefCell::new(None),
        });
        let weak = Rc::downgrade(&shared);
        shared.timeout.replace(Some(Closure::new(move || {
            if let Some(shared) = weak.upgrade() {
                shared.timer.set(None);
                let duty = shared.hidden.borrow_mut().fire(platform::clocks().0);
                if let Some(duty) = duty {
                    shared.perform(duty);
                }
            }
        })));
        let weak = Rc::downgrade(&shared);
        let target = document.clone();
        let listener = Closure::new(move |_: web_sys::Event| {
            if let Some(shared) = weak.upgrade() {
                shared.visibility(target.hidden());
            }
        });
        document
            .add_event_listener_with_callback("visibilitychange", listener.as_ref().unchecked_ref())
            .expect("visibility listener");
        shared.visibility(document.hidden());
        Self { shared, document, listener }
    }
    pub fn carry(&self, duty: StreamDuty) {
        let duty = self.shared.hidden.borrow_mut().carry(duty);
        if let Some(duty) = duty {
            self.shared.perform(duty);
        }
    }
}
impl<R: 'static> Shared<R> {
    fn perform(&self, duty: StreamDuty) {
        match duty {
            StreamDuty::Open { path, headers, bound } => self.fetch.open_stream(&path, &headers, bound),
            StreamDuty::Close => self.fetch.close_stream(),
        }
    }
    fn visibility(&self, hidden: bool) {
        let (now, _) = platform::clocks();
        let duty = self.hidden.borrow_mut().visibility(hidden, now);
        if let Some(duty) = duty {
            self.perform(duty);
        }
        let window = web_sys::window().expect("browser window");
        if let Some(timer) = self.timer.take() {
            window.clear_timeout_with_handle(timer);
        }
        if let Some(deadline) = self.hidden.borrow().deadline() {
            let millis = deadline.as_nanos().saturating_sub(now.as_nanos()).div_ceil(1_000_000);
            let timeout = self.timeout.borrow();
            let callback = timeout.as_ref().expect("timeout installed");
            self.timer.set(Some(
                window
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        callback.as_ref().unchecked_ref(),
                        i32::try_from(millis).unwrap_or(i32::MAX),
                    )
                    .expect("hidden deadline"),
            ));
        }
    }
}
impl<R> Drop for Hidden<R> {
    fn drop(&mut self) {
        let _removed = self
            .document
            .remove_event_listener_with_callback("visibilitychange", self.listener.as_ref().unchecked_ref());
        if let Some(timer) = self.shared.timer.take() {
            web_sys::window().expect("browser window").clear_timeout_with_handle(timer);
        }
    }
}
