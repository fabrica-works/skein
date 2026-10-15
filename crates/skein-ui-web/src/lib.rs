//! Browser parts, usable independently (ui-web.md, 5.1).
#![forbid(unsafe_code)]

pub mod dom;
pub mod inbox;
mod look;
pub mod mapping;
pub mod page;
pub mod svg;
pub use look::Look;

#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(test)]
mod tests;

pub mod fetch;
pub mod hidden;
pub mod platform;
pub mod store;
pub mod transport;

pub mod history;
pub mod restart;
pub mod timer;
pub mod title;
