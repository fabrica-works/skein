//! The fake's own inflater (fake-llm.md, section 3.1): a request body sent
//! with `Content-Encoding: gzip` read back from its gzip member (RFC 1952),
//! DEFLATE inside (RFC 1951), into the room the server already holds for a
//! body. Simple over fast: one bit at a time, one symbol's code searched
//! width by width, and the CRC-32 reckoned bit by bit. A member that is
//! malformed, carries flags or trailing bytes, fails its CRC or ISIZE, or
//! inflates past the room is refused as `None`.

use skein_lib::List;

/// A member's first four bytes: gzip, DEFLATE, no flags. Ten bytes of
/// header in all; the encoder sends no optional fields.
const MAGIC: [u8; 4] = [0x1f, 0x8b, 8, 0];
const HEADER: usize = 10;

/// The order in which a block's header sends the widths of its width code
/// (RFC 1951, 3.2.7).
const ORDER: [u8; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// Each length symbol from 257: its base and extra bits (RFC 1951, 3.2.5).
const LENGTHS: [(u16, u8); 29] = [
    (3, 0),
    (4, 0),
    (5, 0),
    (6, 0),
    (7, 0),
    (8, 0),
    (9, 0),
    (10, 0),
    (11, 1),
    (13, 1),
    (15, 1),
    (17, 1),
    (19, 2),
    (23, 2),
    (27, 2),
    (31, 2),
    (35, 3),
    (43, 3),
    (51, 3),
    (59, 3),
    (67, 4),
    (83, 4),
    (99, 4),
    (115, 4),
    (131, 5),
    (163, 5),
    (195, 5),
    (227, 5),
    (258, 0),
];

/// Each distance symbol: its base and extra bits (RFC 1951, 3.2.5).
const DISTANCES: [(u16, u8); 30] = [
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (5, 1),
    (7, 1),
    (9, 2),
    (13, 2),
    (17, 3),
    (25, 3),
    (33, 4),
    (49, 4),
    (65, 5),
    (97, 5),
    (129, 6),
    (193, 6),
    (257, 7),
    (385, 7),
    (513, 8),
    (769, 8),
    (1025, 9),
    (1537, 9),
    (2049, 10),
    (3073, 10),
    (4097, 11),
    (6145, 11),
    (8193, 12),
    (12289, 12),
    (16385, 13),
    (24577, 13),
];

/// The most symbols one code holds: 286 literal/length and 30 distance
/// widths are sent together, and a width code has 19.
const SYMBOLS: usize = 320;

/// Inflates `member` into `out`, which is cleared first; the worlds read
/// a sent gzip body back through it too.
pub fn inflate(member: &[u8], out: &mut List<u8>) -> Option<()> {
    out.clear();
    if member.get(..4)? != MAGIC.as_slice() {
        return None;
    }
    let mut input = Input { bytes: member.get(HEADER..)?, bit: 0 };
    let mut last = false;
    // Each block takes three bits at least, so the member's bits bound them.
    for _block in 0..=member.len() {
        if last {
            break;
        }
        last = input.bits(1)? == 1;
        match input.bits(2)? {
            0 => stored(&mut input, out)?,
            1 => {
                let (literals, distances) = fixed()?;
                codes(&mut input, out, &literals, &distances)?;
            }
            2 => {
                let (literals, distances) = header(&mut input)?;
                codes(&mut input, out, &literals, &distances)?;
            }
            _ => return None,
        }
    }
    if !last {
        return None;
    }
    let at = input.align()?;
    let trailer = input.bytes.get(at..)?;
    let check = <[u8; 4]>::try_from(trailer.get(..4)?).ok()?;
    let size = <[u8; 4]>::try_from(trailer.get(4..)?).ok()?;
    if u32::from_le_bytes(check) != crc(out.as_slice()) || u32::from_le_bytes(size) != out.len() {
        return None;
    }
    Some(())
}

/// A member's DEFLATE data read bit by bit, each byte's lowest bit first
/// (RFC 1951, 3.1.1).
struct Input<'a> {
    bytes: &'a [u8],
    bit: usize,
}

impl Input<'_> {
    /// `width` bits, at most 16, the first lowest.
    fn bits(&mut self, width: u32) -> Option<u32> {
        let mut value = 0_u32;
        for index in 0..width.min(16) {
            let byte = *self.bytes.get(self.bit >> 3_u32)?;
            let bit = u32::from(byte.checked_shr(u32::try_from(self.bit & 7).ok()?)? & 1);
            value |= bit.checked_shl(index)?;
            self.bit = self.bit.checked_add(1)?;
        }
        Some(value)
    }

    /// Skips to the next byte's start, and returns where that is.
    fn align(&mut self) -> Option<usize> {
        let at = self.bit.div_ceil(8);
        self.bit = at.checked_mul(8)?;
        Some(at)
    }
}

/// A canonical Huffman code (RFC 1951, 3.2.2), for decoding: how many codes
/// there are of each width, and the symbols in the codes' order.
struct Code {
    counts: [u16; 16],
    symbols: [u16; SYMBOLS],
}

/// The code whose symbols have these widths, 0 for a symbol unused; an
/// oversubscribed code is refused, and an incomplete one fails only on a
/// string that starts no code.
fn code(widths: &[u8]) -> Option<Code> {
    let mut code = Code { counts: [0; 16], symbols: [0; SYMBOLS] };
    for width in widths {
        let count = code.counts.get_mut(usize::from(*width))?;
        *count = count.checked_add(1)?;
    }
    *code.counts.get_mut(0)? = 0;
    let mut left = 1_i32;
    let mut offsets = [0_u16; 16];
    for width in 1..16_usize {
        let count = *code.counts.get(width)?;
        left = left.checked_mul(2)?.checked_sub(i32::from(count))?;
        if left < 0_i32 {
            return None;
        }
        if let Some(next) = offsets.get(width.checked_add(1)?).copied() {
            *offsets.get_mut(width.checked_add(1)?)? = next.checked_add(offsets.get(width)?.checked_add(count)?)?;
        }
    }
    for (symbol, width) in widths.iter().enumerate() {
        if *width != 0 {
            let offset = offsets.get_mut(usize::from(*width))?;
            *code.symbols.get_mut(usize::from(*offset))? = u16::try_from(symbol).ok()?;
            *offset = offset.checked_add(1)?;
        }
    }
    Some(code)
}

impl Code {
    /// The next symbol, its code read one bit at a time, most significant
    /// first.
    fn decode(&self, input: &mut Input<'_>) -> Option<u16> {
        let mut value = 0_u32;
        let mut first = 0_u32;
        let mut index = 0_u32;
        for count in self.counts.get(1..)? {
            value |= input.bits(1)?;
            let count = u32::from(*count);
            if value < first.checked_add(count)? {
                let at = index.checked_add(value.checked_sub(first)?)?;
                return self.symbols.get(usize::try_from(at).ok()?).copied();
            }
            index = index.checked_add(count)?;
            first = first.checked_add(count)?.checked_shl(1)?;
            value = value.checked_shl(1)?;
        }
        None
    }
}

/// A stored block (RFC 1951, 3.2.4): its length, that length's complement,
/// and its bytes.
fn stored(input: &mut Input<'_>, out: &mut List<u8>) -> Option<()> {
    let at = input.align()?;
    let [a, b, c, d] = <[u8; 4]>::try_from(input.bytes.get(at..at.checked_add(4)?)?).ok()?;
    let length = u16::from_le_bytes([a, b]);
    if length != !u16::from_le_bytes([c, d]) {
        return None;
    }
    let start = at.checked_add(4)?;
    let end = start.checked_add(usize::from(length))?;
    for byte in input.bytes.get(start..end)? {
        out.push(*byte).ok()?;
    }
    input.bit = end.checked_mul(8)?;
    Some(())
}

/// The fixed literal/length and distance codes (RFC 1951, 3.2.6).
fn fixed() -> Option<(Code, Code)> {
    let mut widths = [0_u8; 288];
    for (symbol, width) in widths.iter_mut().enumerate() {
        *width = match symbol {
            0..=143 | 280.. => 8,
            144..=255 => 9,
            _ => 7,
        };
    }
    Some((code(&widths)?, code(&[5; 30])?))
}

/// A block's header in its own codes (RFC 1951, 3.2.7): its literal/length
/// and distance codes.
fn header(input: &mut Input<'_>) -> Option<(Code, Code)> {
    let literals = usize::try_from(input.bits(5)?.checked_add(257)?).ok()?;
    let distances = usize::try_from(input.bits(5)?.checked_add(1)?).ok()?;
    let sent = usize::try_from(input.bits(4)?.checked_add(4)?).ok()?;
    if literals > 286 || distances > 30 {
        return None;
    }
    let mut widths = [0_u8; 19];
    for symbol in ORDER.get(..sent)? {
        *widths.get_mut(usize::from(*symbol))? = u8::try_from(input.bits(3)?).ok()?;
    }
    let width_code = code(&widths)?;
    let total = literals.checked_add(distances)?;
    let mut all = [0_u8; SYMBOLS];
    let mut filled = 0_usize;
    for _run in 0..total {
        if filled == total {
            break;
        }
        let (width, repeats) = match width_code.decode(input)? {
            width @ 0..=15 => (u8::try_from(width).ok()?, 1),
            16 => (*all.get(filled.checked_sub(1)?)?, input.bits(2)?.checked_add(3)?),
            17 => (0, input.bits(3)?.checked_add(3)?),
            18 => (0, input.bits(7)?.checked_add(11)?),
            _ => return None,
        };
        let end = filled.checked_add(usize::try_from(repeats).ok()?)?;
        if end > total {
            return None;
        }
        for slot in all.get_mut(filled..end)? {
            *slot = width;
        }
        filled = end;
    }
    if filled != total || *all.get(256)? == 0 {
        return None;
    }
    Some((code(all.get(..literals)?)?, code(all.get(literals..total)?)?))
}

/// Reads a block's symbols in these codes to its end.
fn codes(input: &mut Input<'_>, out: &mut List<u8>, literals: &Code, distances: &Code) -> Option<()> {
    // Each symbol takes a bit at least, so the member's bits bound them.
    let bound = input.bytes.len().checked_mul(8)?;
    for _symbol in 0..=bound {
        let symbol = literals.decode(input)?;
        match symbol {
            0..=255 => out.push(u8::try_from(symbol).ok()?).ok()?,
            256 => return Some(()),
            _ => {
                let (base, extra) = *LENGTHS.get(usize::from(symbol.checked_sub(257)?))?;
                let length = u32::from(base).checked_add(input.bits(u32::from(extra))?)?;
                let (base, extra) = *DISTANCES.get(usize::from(distances.decode(input)?))?;
                let distance = u32::from(base).checked_add(input.bits(u32::from(extra))?)?;
                let mut from = out.len().checked_sub(distance)?;
                for _byte in 0..length {
                    let byte = *out.get(from)?;
                    out.push(byte).ok()?;
                    from = from.checked_add(1)?;
                }
            }
        }
    }
    None
}

/// The CRC-32 of the inflated body (RFC 1952, 8), bit by bit with the
/// reflected polynomial: the encoder's table is not consulted.
fn crc(body: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in body {
        crc ^= u32::from(*byte);
        for _bit in 0..8_u32 {
            crc = if crc & 1 == 1 { (crc >> 1_u32) ^ 0xEDB8_8320 } else { crc >> 1_u32 };
        }
    }
    !crc
}

#[cfg(test)]
#[expect(clippy::disallowed_types, clippy::disallowed_macros, reason = "a test builds bodies and members in a Vec")]
#[expect(clippy::disallowed_methods, reason = "fixtures are built with ordinary iterators and slices")]
#[expect(
    clippy::arithmetic_side_effects,
    clippy::default_numeric_fallback,
    reason = "the fixtures' sizes and offsets use ordinary test arithmetic"
)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// `body` through the client's encoder, as the Codex route sends it.
    fn encoded(body: &[u8]) -> Vec<u8> {
        let mut member = vec![0; skein_http::gzip::bound(body.len()).expect("a bounded body")];
        let length = skein_http::gzip::encode(body, &mut member).expect("room for the member");
        member.truncate(length);
        member
    }

    fn inflated(member: &[u8], room: u32) -> Option<Vec<u8>> {
        let mut out = List::with_capacity(room);
        inflate(member, &mut out).map(|()| out.as_slice().to_vec())
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len()).step_by(2).map(|at| u8::from_str_radix(&text[at..at + 2], 16).expect("hex")).collect()
    }

    #[test]
    fn what_the_encoder_sends_inflates_to_the_body() {
        let mut state = 0x2545_f491_u32;
        let noise: Vec<u8> = (0..70_000)
            .map(|_| {
                state ^= state << 13_u32;
                state ^= state >> 17_u32;
                state ^= state << 5_u32;
                state.to_le_bytes()[0]
            })
            .collect();
        let text = br#"{"model":"fixture","input":[{"type":"message","content":"Hello"}]}"#.repeat(900);
        let run = vec![7; 300_000];
        for body in [&b""[..], b"x", &text, &noise, &run] {
            let member = encoded(body);
            let room = u32::try_from(body.len()).expect("a bounded body");
            assert_eq!(inflated(&member, room).as_deref(), Some(body));
        }
    }

    #[test]
    fn stored_and_fixed_blocks_inflate_too() {
        // Python's zlib at level 0, and with Z_FIXED.
        let stored = hex("1f8b0800000000000403010f00f0ff73746f7265642061732069742069736578d15a0f000000");
        assert_eq!(inflated(&stored, 15).as_deref(), Some(&b"stored as it is"[..]));
        let fixed = hex("1f8b08000000000002034b5448cbac484d5148ce4f49d55148c4c903005ef3d59928000000");
        assert_eq!(inflated(&fixed, 40).as_deref(), Some(&b"a fixed code, a fixed code, a fixed code"[..]));
    }

    #[test]
    fn a_member_that_is_not_whole_and_checked_is_refused() {
        let body = b"a body the server holds room for, a body the server holds room for";
        let member = encoded(body);
        let room = u32::try_from(body.len()).expect("a bounded body");
        assert!(inflated(&member, room).is_some());
        let changed = |at: usize, bits: u8| {
            let mut member = member.clone();
            member[at] ^= bits;
            member
        };
        let crc = member.len() - 8;
        let size = member.len() - 4;
        for refused in [
            changed(0, 1),    // not gzip
            changed(2, 1),    // not DEFLATE
            changed(3, 8),    // a file name follows
            changed(crc, 1),  // the CRC-32 fails
            changed(size, 1), // ISIZE fails
            member[..member.len() - 1].to_vec(),
            member[..HEADER + 2].to_vec(),
            [&member[..], &[0]].concat(), // a byte trails the member
        ] {
            assert_eq!(inflated(&refused, room), None);
        }
        assert_eq!(inflated(&member, room - 1), None, "past the room");
    }
}
