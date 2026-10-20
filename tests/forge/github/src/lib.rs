//! GitHub dialect conformance; live execution belongs to the fixture owner alone.
mod cases;
mod fake;
mod journal;
pub mod live;
pub mod mask;
mod runner;
mod wire;
pub use runner::{Report, clean, run_fake, run_live};
pub use skein_forge_conformance::Result;
#[cfg(test)]
mod tests;
