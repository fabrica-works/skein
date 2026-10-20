//! The App's wanted access, shared by reconcile and paste (forge-setup.md, 5.6 and 5.8).
use alloc::boxed::Box;
use skein_forge::github::{document::App, setup};
use skein_lib::List;

/// One wanted permission or subscription the App lacks (forge-setup.md, 5.6, step 2).
/// Names are owned copies of the dialect's bounded manifest constants.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Lacking {
    Permission { name: Box<[u8]>, access: setup::Access },
    Event { name: Box<[u8]> },
}

/// Compares an admitted App document using the dialect's wanted access check
/// (forge-setup.md, 5.6 and 5.8). Permissions precede events, each in manifest
/// order, with at most nine permissions and fourteen events. Stronger grants
/// satisfy weaker ones; extra grants and subscriptions add nothing to the result.
#[must_use]
pub fn lacks(app: &App) -> Box<[Lacking]> {
    let missing = setup::lacks(app);
    let count = setup::PERMISSIONS.len().checked_add(setup::EVENTS.len()).expect("constant wanted count");
    let mut lacking = List::with_capacity(u32::try_from(count).expect("constant wanted count fits"));
    for permission in missing.permissions {
        lacking
            .push(Lacking::Permission { name: permission.name, access: permission.access })
            .expect("one entry per wanted permission");
    }
    for name in missing.events {
        lacking.push(Lacking::Event { name }).expect("one entry per wanted event");
    }
    lacking.into_boxed()
}
