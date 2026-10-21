//! Client call spelling and streaming answer decoding, for a fake git client.
use super::{Answer, Call, Decoder as Scratch, Name, read, write};
use crate::Limits;
use alloc::boxed::Box;
use skein_fake_forge_domain::api;
use skein_json::{tokenizer, writer};
use skein_lib::{Time, Writer};
#[derive(Debug, PartialEq, Eq)]
pub struct Encoded {
    pub target: Box<[u8]>,
    pub body: Box<[u8]>,
}
pub fn encode(call: &Call, limits: &Limits) -> Result<Encoded, writer::Refusal> {
    let mut measure = writer::Encoder::measure(&limits.documents.writer);
    write::call(&mut measure, call)?;
    let mut json = write::finish(measure, limits)?;
    write::call(&mut json, call)?;
    let body = json.finish();
    if body.len() > usize::try_from(limits.http.body).expect("u32 fits") {
        return Err(writer::Refusal::TooLong);
    }
    let mut target = Writer::new(11_usize.saturating_add(call.name().segment().len()));
    target.put(b"/.fake/git/").expect("sized target");
    target.put(call.name().segment()).expect("sized target");
    Ok(Encoded { target: target.finish(), body })
}
#[derive(Debug)]
pub struct Decoder {
    name: Name,
    status: u16,
    reset: Time,
    named: bool,
    scratch: Scratch,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Demand(tokenizer::Request),
    Decoded(Result<Answer, api::Error>),
    Malformed,
}
/// `reset` is the caller's clock plus Retry-After for an unnamed 429.
#[must_use]
pub fn decoder(name: Name, status: u16, reset: Time, limits: &Limits) -> Decoder {
    Decoder { name, status, reset, named: false, scratch: Scratch::new(limits) }
}
#[must_use]
pub const fn first(_decoder: &Decoder) -> tokenizer::Request {
    tokenizer::Request::Next
}
/// Whether the body named its error, read after `step` gives `Decoded(Err(_))`; false for a status fallback.
#[must_use]
pub const fn named(decoder: &Decoder) -> bool {
    decoder.named
}
pub fn step(decoder: &mut Decoder, limits: &Limits, event: &tokenizer::Event) -> Step {
    let result = read::collect(&mut decoder.scratch, event);
    if decoder.status != 200 {
        return match result {
            Ok(None) => Step::Demand(tokenizer::Request::Next),
            Ok(Some(document)) => Step::Decoded(Err(match read::error(&document) {
                Ok(error) => {
                    decoder.named = true;
                    error
                }
                Err(()) => status(decoder.status, decoder.reset),
            })),
            Err(()) => Step::Decoded(Err(status(decoder.status, decoder.reset))),
        };
    }
    match result {
        Ok(None) => Step::Demand(tokenizer::Request::Next),
        Ok(Some(document)) => match read::answer(&document, decoder.name, limits) {
            Ok(answer) => Step::Decoded(Ok(answer)),
            Err(()) => Step::Malformed,
        },
        Err(()) => Step::Malformed,
    }
}
fn status(status: u16, reset: Time) -> api::Error {
    match status {
        401 | 403 => api::Error::Forbidden,
        404 => api::Error::Missing(api::What::Repository),
        413 => api::Error::TooLarge,
        429 => api::Error::RateLimited { reset },
        500 => api::Error::Timeout,
        502 => api::Error::Unreachable,
        507 => api::Error::Full,
        _ => api::Error::Unavailable,
    }
}
