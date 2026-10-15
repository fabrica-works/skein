//! Title, language and the warning for requests a close would lose.
use wasm_bindgen::{JsCast, closure::Closure};
pub struct Title {
    closing: Closure<dyn FnMut(web_sys::BeforeUnloadEvent)>,
}
impl Default for Title {
    fn default() -> Self {
        let closing = Closure::new(move |event: web_sys::BeforeUnloadEvent| {
            event.prevent_default();
            event.set_return_value("");
        });
        Self { closing }
    }
}
impl Title {
    pub fn set(&self, heading: &[u8], name: &[u8], language: &[u8], loses: bool) {
        let window = web_sys::window().expect("window");
        let document = window.document().expect("document");
        document.set_title(&super::text(heading, name));
        document
            .document_element()
            .expect("html")
            .set_attribute("lang", &String::from_utf8_lossy(language))
            .expect("language");
        window.set_onbeforeunload(if loses { Some(self.closing.as_ref().unchecked_ref()) } else { None });
    }
}
