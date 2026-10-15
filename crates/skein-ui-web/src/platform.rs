//! Platform facts are read by the shell, outside the pure page part.
use crate::mapping::Primary;
#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::{clocks, facts};

pub struct Facts {
    pub now: skein_lib::Time,
    pub wall: skein_lib::Wall,
    pub seed: u64,
    pub offset: i32,
    pub languages: Box<[u8]>,
    pub primary: Primary,
}
#[must_use]
pub fn primary(platform: Option<&str>) -> Primary {
    if platform
        .is_some_and(|platform| ["Mac", "iPhone", "iPad", "iPod"].iter().any(|prefix| platform.starts_with(prefix)))
    {
        Primary::Meta
    } else {
        Primary::Control
    }
}
