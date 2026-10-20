//! GitHub's owner-provided values and plans (forge-setup.md, 5).
pub(crate) mod app;
pub mod checks;
pub mod convert;
pub mod given;
#[cfg_attr(not(test), expect(dead_code, reason = "the reconcile and paste plans reuse the hook child"))]
pub(crate) mod hook;
mod key;
pub mod limits;
pub mod manifest;
pub use checks::Lacking;
