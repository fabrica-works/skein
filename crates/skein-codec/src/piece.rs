//! Items in one byte-bounded piece (codec.md, section 3).

use alloc::boxed::Box;
use skein_lib::{Overflow, Reader, Writer};

use crate::Reason;

/// Writes one item after its big-endian `u32` length, or writes nothing.
/// The owner's writer bounds the whole piece, including every length.
pub fn write(out: &mut Writer, item: &[u8]) -> Result<(), Overflow> {
    let Ok(length) = u32::try_from(item.len()) else { return Err(Overflow) };
    let needed = item.len().checked_add(4).ok_or(Overflow)?;
    if needed > out.room() {
        return Err(Overflow);
    }
    out.put(&length.to_be_bytes())?;
    out.put(item)
}

/// Splits the next item from a bounded piece, or returns `None` at its end.
/// Refuses a partial length or an item past the end before allocating.
/// The owner decodes each item and counts its decoded size against its sum.
pub fn split(reader: &mut Reader<'_>) -> Result<Option<Box<[u8]>>, Reason> {
    if reader.is_empty() {
        return Ok(None);
    }
    let length = reader.u32().ok_or(Reason::Malformed)?;
    let bytes = reader.bytes(length).ok_or(Reason::Malformed)?;
    Ok(Some(Box::from(bytes)))
}
