//! The forge component and fake protocol layer joined by in-memory streams.
pub mod fixtures;
mod referee;
mod world;
pub use referee::Accounting;
pub use world::World;
