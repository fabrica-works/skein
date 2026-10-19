//! Bounds shared by both sides.
use skein_json::{tokenizer, writer};
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Limits {
    pub json: tokenizer::Limits,
    pub writer: writer::Limits,
    pub string_bytes: u32,
    pub body_bytes: u32,
    pub inner: u32,
    pub target_bytes: u32,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Problem {
    TokenizerString,
    Depth,
    Zero,
}
pub fn check(limits: &Limits) -> Result<(), Problem> {
    if limits.string_bytes == 0
        || limits.body_bytes == 0
        || limits.inner == 0
        || limits.target_bytes == 0
        || limits.json.chunk == 0
        || limits.json.number == 0
        || limits.json.length == 0
        || limits.writer.length == 0
    {
        return Err(Problem::Zero);
    }
    if limits.json.string < limits.string_bytes || limits.json.string < limits.body_bytes {
        return Err(Problem::TokenizerString);
    }
    if limits.json.depth < 8 || limits.writer.depth < 8 {
        return Err(Problem::Depth);
    }
    Ok(())
}
