//! Origin targets and percent encoding, segment boundaries preserved.
use super::limits::Limits;
use alloc::boxed::Box;
use skein_lib::{List, Overflow, Writer};
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Pair {
    pub name: Box<[u8]>,
    pub value: Box<[u8]>,
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Target {
    pub segments: Box<[Box<[u8]>]>,
    pub query: Box<[Pair]>,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Unread {
    Escape,
    TooLong,
    Form,
}
pub fn segment(name: &[u8], out: &mut Writer) -> Result<(), Overflow> {
    for byte in name {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(byte) {
            out.put(&[*byte])?;
        } else {
            let digits =
                [b'%', super::id::hex(byte / 16).to_ascii_uppercase(), super::id::hex(byte % 16).to_ascii_uppercase()];
            out.put(&digits)?;
        }
    }
    Ok(())
}
pub fn file(path: &[u8], out: &mut Writer) -> Result<(), Overflow> {
    let mut start = 0;
    for at in 0..=path.len() {
        if at == path.len() || path.get(at) == Some(&b'/') {
            segment(path.get(start..at).expect("path positions"), out)?;
            if at != path.len() {
                out.put(b"/")?;
            }
            start = at.saturating_add(1);
        }
    }
    Ok(())
}
pub(crate) fn segment_length(name: &[u8]) -> usize {
    let mut size = 0usize;
    for byte in name {
        size = size.saturating_add(if byte.is_ascii_alphanumeric() || b"-._~".contains(byte) { 1 } else { 3 });
    }
    size
}
fn unescape(text: &[u8], query: bool) -> Result<Box<[u8]>, Unread> {
    let mut out = List::with_capacity(match u32::try_from(text.len()) {
        Ok(n) => n,
        Err(_) => return Err(Unread::TooLong),
    });
    let mut skip: u32 = 0;
    for (i, byte) in text.iter().enumerate() {
        if skip != 0 {
            skip = skip.saturating_sub(1);
            continue;
        }
        let value = if *byte == b'%' {
            let a = *text.get(i.saturating_add(1)).ok_or(Unread::Escape)?;
            let b = *text.get(i.saturating_add(2)).ok_or(Unread::Escape)?;
            let high = super::id::digit(a.to_ascii_lowercase()).ok_or(Unread::Escape)?;
            let low = super::id::digit(b.to_ascii_lowercase()).ok_or(Unread::Escape)?;
            skip = 2;
            high.saturating_mul(16).saturating_add(low)
        } else if query && *byte == b'+' {
            b' '
        } else {
            *byte
        };
        out.push(value).expect("decoded bytes no longer than their input");
    }
    Ok(out.into_boxed())
}
pub fn read(target: &[u8], limits: &Limits) -> Result<Target, Unread> {
    if target.len() > usize::try_from(limits.target_bytes).expect("u32 fits") {
        return Err(Unread::TooLong);
    }
    if target.first() != Some(&b'/') || target.contains(&b'#') {
        return Err(Unread::Form);
    }
    let end = match skein_lib::bytes::find(target, b"?") {
        Some(at) => at,
        None => target.len(),
    };
    let mut segments = List::with_capacity(12);
    let mut query = List::with_capacity(8);
    let mut start = 1;
    for at in 1..=end {
        if at == end || target.get(at) == Some(&b'/') {
            let value = unescape(target.get(start..at).expect("path range"), false)?;
            if segments.push(value).is_err() {
                return Err(Unread::TooLong);
            }
            start = at.saturating_add(1);
        }
    }
    if end < target.len() {
        start = end.saturating_add(1);
        let query_start = start;
        for at in query_start..=target.len() {
            if at == target.len() || target.get(at) == Some(&b'&') {
                let part = target.get(start..at).expect("query range");
                let eq = match skein_lib::bytes::find(part, b"=") {
                    Some(eq) => eq,
                    None => part.len(),
                };
                let name = unescape(part.get(..eq).expect("query key"), true)?;
                let value = unescape(part.get(eq.saturating_add(1)..).unwrap_or(b""), true)?;
                if query.push(Pair { name, value }).is_err() {
                    return Err(Unread::TooLong);
                }
                start = at.saturating_add(1);
            }
        }
    }
    Ok(Target { segments: segments.into_boxed(), query: query.into_boxed() })
}
