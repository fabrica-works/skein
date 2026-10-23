//! Independent form reader: one field scan, exact decoded allocations.
use alloc::boxed::Box;
use skein_lib::{List, Writer, bytes};

#[derive(Debug)]
pub(crate) struct Field {
    name: Box<[u8]>,
    value: Box<[u8]>,
    past: bool,
}
#[derive(Debug)]
pub(crate) struct Form {
    fields: List<Field>,
    pub(crate) duplicate_binding: bool,
    pub(crate) duplicate: bool,
}
impl Form {
    pub(crate) fn read(input: &[u8], state_bytes: Option<u32>) -> Option<Form> {
        let count = bytes::count(input, b"&", u32::MAX).checked_add(1)?;
        let mut form = Form { fields: List::with_capacity(count), duplicate_binding: false, duplicate: false };
        let mut start = 0_usize;
        for end in 0..=input.len() {
            if end != input.len() && input.get(end) != Some(&b'&') {
                continue;
            }
            let piece = input.get(start..end)?;
            start = end.checked_add(1)?;
            if piece.is_empty() {
                continue;
            }
            let at = bytes::find(piece, b"=").unwrap_or(piece.len());
            let name = decoded(piece.get(..at)?)?;
            let raw = if at == piece.len() { b"".as_slice() } else { piece.get(at.checked_add(1)?..)? };
            let len = decoded_len(raw)?;
            let mut past = false;
            if (name.as_ref() == b"state" || name.as_ref() == b"nonce")
                && let Some(cap) = state_bytes
            {
                past = len > usize::try_from(cap).ok()?;
            }
            let value = if past { bytes::copy_of(b"") } else { decoded(raw)? };
            for prior in &form.fields {
                if prior.name == name {
                    form.duplicate = true;
                    if name.as_ref() == b"client_id" || name.as_ref() == b"redirect_uri" {
                        form.duplicate_binding = true;
                    }
                }
            }
            form.fields.push(Field { name, value, past }).ok()?;
        }
        Some(form)
    }
    pub(crate) fn past(&self, name: &[u8]) -> bool {
        for field in &self.fields {
            if field.name.as_ref() == name && field.past {
                return true;
            }
        }
        false
    }
    pub(crate) fn get(&self, name: &[u8]) -> Option<&[u8]> {
        for field in &self.fields {
            if field.name.as_ref() == name {
                return if field.value.is_empty() { None } else { Some(&field.value) };
            }
        }
        None
    }
}

fn decoded_len(input: &[u8]) -> Option<usize> {
    let mut escaped = 0_usize;
    let mut len = 0_usize;
    // The two hex digits are checked before setting aside the final buffer.
    for (at, &byte) in input.iter().enumerate() {
        if escaped > 0 {
            hex(byte)?;
            escaped = escaped.checked_sub(1)?;
        } else {
            if byte == b'%' {
                input.get(at.checked_add(2)?)?;
                escaped = 2;
            }
            len = len.checked_add(1)?;
        }
    }
    Some(len)
}
pub(crate) fn decoded(input: &[u8]) -> Option<Box<[u8]>> {
    let len = decoded_len(input)?;
    let mut out = Writer::new(len);
    let mut hex_left = 0_u8;
    let mut high = 0_u8;
    for &byte in input {
        if hex_left == 2 {
            high = hex(byte)?.checked_mul(16)?;
            hex_left = 1;
        } else if hex_left == 1 {
            let byte = high | hex(byte)?;
            out.put(&[byte]).ok()?;
            hex_left = 0;
        } else if byte == b'%' {
            hex_left = 2;
        } else {
            out.put(&[if byte == b'+' { b' ' } else { byte }]).ok()?;
        }
    }
    Some(out.finish())
}
fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte.wrapping_sub(b'0')),
        b'a'..=b'f' => byte.checked_sub(b'a')?.checked_add(10),
        b'A'..=b'F' => byte.checked_sub(b'A')?.checked_add(10),
        0..=b'/' | b':'..=b'@' | b'G'..=b'`' | b'g'..=u8::MAX => None,
    }
}
pub(crate) fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.' || byte == b'_' || byte == b'~'
}
pub(crate) fn url_character(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}
fn escaped_len(input: &[u8]) -> Option<usize> {
    let mut len = 0_usize;
    for &byte in input {
        len = len.checked_add(if unreserved(byte) { 1 } else { 3 })?;
    }
    Some(len)
}
fn escape(out: &mut Writer, input: &[u8]) {
    for &byte in input {
        if unreserved(byte) {
            out.put(&[byte]).expect("measured character");
        } else {
            let digits = b"0123456789ABCDEF";
            let high = *digits.get(usize::from(byte >> 4_u8)).expect("hex digit");
            let low = *digits.get(usize::from(byte & 15)).expect("hex digit");
            out.put(&[b'%', high, low]).expect("measured escape");
        }
    }
}
pub(crate) fn location(uri: &[u8], name: &[u8], value: &[u8], state: Option<&[u8]>) -> Option<Box<[u8]>> {
    let mut len = uri.len().checked_add(name.len())?.checked_add(2)?.checked_add(escaped_len(value)?)?;
    if let Some(state) = state {
        len = len.checked_add(7)?.checked_add(escaped_len(state)?)?;
    }
    let mut out = Writer::new(len);
    out.put(uri).expect("measured URI");
    out.put(if uri.contains(&b'?') { b"&" } else { b"?" }).expect("separator");
    out.put(name).expect("measured parameter");
    out.put(b"=").expect("equals");
    escape(&mut out, value);
    if let Some(state) = state {
        out.put(b"&state=").expect("measured state key");
        escape(&mut out, state);
    }
    Some(out.finish())
}
