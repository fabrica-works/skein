//! The fake person over the browser kit (ui-web.md, 14.2).
#![forbid(unsafe_code)]
mod face;
pub use face::BrowserFace;
#[cfg(test)]
mod tests;
