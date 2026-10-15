//! Browser parts, usable independently (ui-web.md, 5.1).
#![forbid(unsafe_code)]

pub mod inbox;

#[cfg(target_arch = "wasm32")]
pub use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(test)]
mod tests;
