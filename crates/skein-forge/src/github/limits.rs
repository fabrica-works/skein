//! Bounds shared by GitHub's two sides (forge-github.md, section 2).
use skein_json::{tokenizer, writer};
/// Bounds on tokens, documents, retained fields and targets.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    pub json: tokenizer::Limits,
    pub writer: writer::Limits,
    pub string_bytes: u32,
    pub body_bytes: u32,
    pub inner: u32,
    pub target_bytes: u32,
}
/// A limit that cannot hold GitHub's documents.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Problem {
    Zero,
    String,
    Depth,
}
/// The depth needed by a logged delivery's wrapped payload.
pub const DEPTH: u32 = 10;
/// Checks that both sides can honour the limits.
pub fn check(limits: &Limits) -> Result<(), Problem> {
    if limits.string_bytes == 0
        || limits.body_bytes == 0
        || limits.inner == 0
        || limits.target_bytes == 0
        || limits.json.chunk == 0
        || limits.json.number == 0
        || limits.json.length == 0
        || limits.writer.length == 0
        || limits.json.string == 0
    {
        return Err(Problem::Zero);
    }
    if limits.json.string < limits.string_bytes || limits.json.string < limits.body_bytes {
        return Err(Problem::String);
    }
    if limits.json.depth < DEPTH || limits.writer.depth < DEPTH {
        return Err(Problem::Depth);
    }
    Ok(())
}
