//! Server body events in, typed call out; answers and errors to JSON.
use super::{Answer, Call, Decoder, Name, read, write};
use crate::Limits;
use alloc::boxed::Box;
use skein_fake_forge_domain::api;
use skein_json::{tokenizer, writer};
#[derive(Debug)]
pub struct BodyDecoder {
    name: Name,
    decoder: Decoder,
}
#[derive(Debug, PartialEq, Eq)]
pub enum BodyStep {
    Demand(tokenizer::Request),
    Decoded(Call),
    Malformed,
}
#[must_use]
pub fn decoder(name: Name, limits: &Limits) -> BodyDecoder {
    BodyDecoder { name, decoder: Decoder::new(limits) }
}
#[must_use]
pub const fn body_first(_decoder: &BodyDecoder) -> tokenizer::Request {
    tokenizer::Request::Next
}
pub fn body(decoder: &mut BodyDecoder, limits: &Limits, event: &tokenizer::Event) -> BodyStep {
    match read::collect(&mut decoder.decoder, event) {
        Ok(Some(document)) => match read::call(&document, decoder.name, limits) {
            Ok(call) => BodyStep::Decoded(call),
            Err(()) => BodyStep::Malformed,
        },
        Ok(None) => BodyStep::Demand(tokenizer::Request::Next),
        Err(()) => BodyStep::Malformed,
    }
}
pub fn encode(answer: &Answer, limits: &Limits) -> Result<Box<[u8]>, writer::Refusal> {
    let mut measure = writer::Encoder::measure(&limits.documents.writer);
    write::answer(&mut measure, answer)?;
    let mut json = write::finish(measure, limits)?;
    write::answer(&mut json, answer)?;
    Ok(json.finish())
}
pub fn error(error: api::Error, limits: &Limits) -> Result<Box<[u8]>, writer::Refusal> {
    write::encode_error(error, limits)
}
