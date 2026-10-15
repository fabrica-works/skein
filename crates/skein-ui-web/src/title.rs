//! The document's title follows the main heading (ui-web.md, 7).
#[cfg(target_arch = "wasm32")]
pub mod web;
#[must_use]
pub fn text(heading: &[u8], name: &[u8]) -> String {
    if heading.is_empty() {
        String::from_utf8_lossy(name).into_owned()
    } else {
        format!("{} · {}", String::from_utf8_lossy(heading), String::from_utf8_lossy(name))
    }
}
