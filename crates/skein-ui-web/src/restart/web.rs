//! The panic hook keeps dates in session storage before a reload.
use super::{Action, Restart};
use skein_lib::Wall;

impl Restart {
    /// Install before starting application code; false leaves the failure words visible.
    #[must_use]
    pub fn install(self, mount: &str) -> bool {
        let now = crate::platform::clocks().1;
        if self.load().is_none_or(|(_, dates)| self.failed(now, &dates)) {
            self.say_failed(mount);
            return false;
        }
        let mount = mount.to_owned();
        std::panic::set_hook(Box::new(move |_| {
            if let Some(window) = web_sys::window() {
                window.set_onbeforeunload(None);
            }
            let Some((storage, mut dates)) = self.load() else {
                self.say_failed(&mount);
                return;
            };
            let action = self.panicked(crate::platform::clocks().1, &mut dates);
            let dates = dates.iter().map(|date| date.as_nanos().to_string()).collect::<Vec<_>>().join(",");
            if storage.set_item(self.key(), &dates).is_err() || action == Action::Failed {
                self.say_failed(&mount);
            } else if let Some(window) = web_sys::window()
                && window.location().reload().is_err()
            {
                self.say_failed(&mount);
            }
        }));
        true
    }
    fn load(&self) -> Option<(web_sys::Storage, Vec<Wall>)> {
        let storage = web_sys::window()?.session_storage().ok()??;
        let held = storage.get_item(self.key()).ok()?.unwrap_or_default();
        let dates = held
            .split(',')
            .filter(|part| !part.is_empty())
            .map(|part| part.parse().ok().map(Wall::from_nanos))
            .collect::<Option<Vec<_>>>()?;
        Some((storage, dates))
    }
    fn say_failed(&self, mount: &str) {
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(mount))
        {
            element.set_text_content(Some(self.words()));
        }
    }
}
