//! One tokenizer event in, one decoder step out.
use super::document::{Document, Row};
use skein_json::tokenizer;
pub(crate) mod walk;
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Malformed {
    Json(tokenizer::Error),
    Shape { field: &'static [u8] },
    Missing { field: &'static [u8] },
    Range { field: &'static [u8] },
    TooLong { field: &'static [u8] },
    Id { field: &'static [u8] },
    Time { field: &'static [u8] },
    Value { field: &'static [u8] },
    Bound { field: &'static [u8] },
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Step {
    Demand(tokenizer::Request),
    Row(Row, tokenizer::Request),
    Done(Document),
    Malformed(Malformed),
}
