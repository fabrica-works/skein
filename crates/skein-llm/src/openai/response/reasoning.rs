//! Ordered native replay items and the completion's reasoning share (llm.md, 2 and 4.2).
use super::Opened;
use crate::openai::{DecodeError, Json, Limits, json};
use alloc::boxed::Box;
use skein_json::{Document, Kind, writer::Encoder};
use skein_lib::List;

/// Borrows the next native object from one replay item or an ordered run.
pub(crate) fn replay_item<'a>(
    value: &'a Json,
    next: &mut u32,
) -> Result<Option<(&'a Document, json::Span)>, DecodeError> {
    let view = value.view();
    if *next == 0 {
        match json::kind(view, 0) {
            Some(Kind::ObjectStart) => {}
            Some(Kind::ArrayStart) if json::len(view) > 2 => *next = 1,
            Some(
                Kind::ArrayStart
                | Kind::ObjectEnd
                | Kind::ArrayEnd
                | Kind::Key
                | Kind::String
                | Kind::Number
                | Kind::True
                | Kind::False
                | Kind::Null
                | Kind::Long,
            )
            | None => return Err(DecodeError::WrongType),
        }
    }
    if *next >= json::len(view) || json::kind(view, *next) == Some(Kind::ArrayEnd) {
        return Ok(None);
    }
    let item = json::value_at(view, *next)?;
    if json::kind(item, 0) != Some(Kind::ObjectStart) {
        return Err(DecodeError::WrongType);
    }
    *next = json::span(view, *next)?;
    Ok(Some(item))
}

pub(crate) fn write_replay(out: &mut Encoder, value: &Json) {
    let mut next = 0;
    for _token in 0..json::len(value.view()) {
        let Some(item) = replay_item(value, &mut next).expect("admitted replay objects") else {
            break;
        };
        json::write_view(out, item);
    }
}

/// Keeps a reasoning run's native objects in order under one retained-token bound.
pub(crate) fn append(previous: &[u8], item: &[u8], limits: &Limits) -> Result<Box<[u8]>, DecodeError> {
    let array = previous.first() == Some(&b'[');
    let prefix = if array {
        previous.get(..previous.len().checked_sub(1).expect("array tail")).expect("array prefix")
    } else {
        previous
    };
    let length = prefix
        .len()
        .checked_add(item.len())
        .ok_or(DecodeError::Malformed)?
        .checked_add(if array { 2 } else { 3 })
        .ok_or(DecodeError::Malformed)?;
    let mut out = List::with_capacity(u32::try_from(length).or(Err(DecodeError::Malformed))?);
    if !array {
        out.push(b'[').expect("measured run");
    }
    for &byte in prefix {
        out.push(byte).expect("measured run");
    }
    out.push(b',').expect("measured run");
    for &byte in item {
        out.push(byte).expect("measured run");
    }
    out.push(b']').expect("measured run");
    let bytes = out.into_boxed();
    let document =
        crate::DocumentLimits { bytes: u32::try_from(length).or(Err(DecodeError::Malformed))?, ..limits.document() };
    let _value = Json::from_bytes(&bytes, &document)?;
    Ok(bytes)
}

pub(super) fn following_index(opened: &List<(u32, Opened)>) -> Result<u32, DecodeError> {
    match opened.last() {
        Some((index, _)) => index.checked_add(1).ok_or(DecodeError::Malformed),
        None => Ok(0),
    }
}
pub(super) fn slot(opened: &List<(u32, Opened)>, index: u32) -> Result<u32, DecodeError> {
    for position in 0..opened.len() {
        if let Some((provider, _)) = opened.get(position)
            && *provider == index
        {
            return Ok(position);
        }
    }
    Err(DecodeError::Malformed)
}
pub(crate) fn charge(total: &mut u64, bytes: usize, limits: &Limits) -> Result<(), DecodeError> {
    let share = u64::from(limits.output_items).checked_mul(u64::from(limits.reasoning)).expect("u32 product fits u64");
    *total = total
        .checked_add(u64::try_from(bytes).expect("usize fits u64"))
        .ok_or(DecodeError::TooLarge { which: crate::Cap::Reasoning, bound: share })?;
    if *total > share {
        return Err(DecodeError::TooLarge { which: crate::Cap::Reasoning, bound: share });
    }
    Ok(())
}
