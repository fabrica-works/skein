//! Plain data supplied by the application (ui-web.md, 3.2).

use skein_ui::{Owner, look::Names, scene::Draw};

#[derive(Clone, Copy)]
pub struct Look<'a> {
    pub names: &'a [Names<'a>],
    /// Each owner's picture files, indexed by the picture's number.
    pub pictures: &'a [(Owner, &'a [&'a [u8]])],
    pub draw: Draw,
    pub elements: &'a [(Owner, skein_ui::Role, skein_ui::Style, &'static str)],
}
