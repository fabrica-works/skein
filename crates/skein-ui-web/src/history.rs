//! Paths and entry scroll, independent of the application's codec (ui-web.md, 9).
#[cfg(target_arch = "wasm32")]
pub mod web;

#[derive(Default)]
pub struct History {
    restore: Option<(f64, f64)>,
}
impl History {
    #[must_use]
    pub fn gone_to(&mut self, path: &[u8], scroll: (f64, f64)) -> Box<[u8]> {
        self.restore = Some(scroll);
        path.into()
    }
    pub fn take_scroll(&mut self) -> Option<(f64, f64)> {
        self.restore.take()
    }
}
