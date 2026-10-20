//! Shared transcript vocabulary, masking and leak refusal for forge conformance tools.
pub mod facts;
pub mod json;
pub mod mask;
pub mod write;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
