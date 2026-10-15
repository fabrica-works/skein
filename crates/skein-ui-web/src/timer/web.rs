//! The browser's single timeout, replaced when the deadline moves.
use crate::inbox::Inbox;
use skein_lib::Time;
use std::rc::Rc;
use wasm_bindgen::{JsCast, closure::Closure};
pub struct Timer {
    deadline: Option<Time>,
    handle: Option<i32>,
    callback: Closure<dyn FnMut()>,
}
impl Timer {
    #[must_use]
    pub fn new<R: 'static>(inbox: Rc<Inbox<R>>, wrap: fn() -> R) -> Self {
        let callback = Closure::new(move || inbox.raise(wrap()));
        Self { deadline: None, handle: None, callback }
    }
    pub fn set(&mut self, deadline: Option<Time>, now: Time) {
        if self.deadline == deadline {
            return;
        }
        let window = web_sys::window().expect("window");
        if let Some(handle) = self.handle.take() {
            window.clear_timeout_with_handle(handle);
        }
        self.deadline = deadline;
        if let Some(deadline) = deadline {
            let nanos = deadline.as_nanos().saturating_sub(now.as_nanos());
            let millis = nanos.saturating_add(999_999) / 1_000_000;
            let timeout = i32::try_from(millis).unwrap_or(i32::MAX);
            self.handle = Some(
                window
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        self.callback.as_ref().unchecked_ref(),
                        timeout,
                    )
                    .expect("timeout"),
            );
        }
    }
    pub fn fired(&mut self) {
        self.deadline = None;
        self.handle = None;
    }
}
