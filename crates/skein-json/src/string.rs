//! A string's text, decoded as it arrives (RFC 8259, section 7; json.md,
//! 3): escapes undone, every byte checked as UTF-8, a control character
//! refused, and the result held under the string limit.
//!
//! The tokenizer scans to the next quote, so a piece ends at a quote or at
//! the scan's maximum, anywhere in a character or an escape: the state
//! between two pieces is a [`Text`].

use skein_lib::List;

use crate::tokenizer::Error;
use crate::utf8::Utf8;

/// Retention chosen by the pending demand (json.md, 3.1).
#[derive(Clone, Copy, Debug)]
pub(crate) enum Retention {
    Strict(u32),
    Capped(u32),
    Discard,
}

/// The reusable text buffer and the decoded length, including discarded text.
#[derive(Debug)]
pub(crate) struct Buffer {
    pub(crate) bytes: List<u8>,
    pub(crate) length: u64,
    pub(crate) retention: Retention,
    /// The first byte of the character or escape being decoded.
    pub(crate) begun: u32,
}

impl Buffer {
    pub(crate) fn reset(&mut self, retention: Retention) {
        self.bytes.clear();
        self.length = 0;
        self.retention = retention;
    }

    fn room(&mut self, more: u32) -> Result<(), Error> {
        self.length = self.length.checked_add(u64::from(more)).ok_or(Error::TooLong)?;
        match self.retention {
            Retention::Strict(limit) => {
                if self.length > u64::from(limit) {
                    return Err(Error::StringTooLong);
                }
            }
            Retention::Capped(limit) => {
                if self.length > u64::from(limit) {
                    self.bytes.clear();
                }
            }
            Retention::Discard => {}
        }
        Ok(())
    }

    fn put(&mut self, bytes: &[u8]) {
        let retain = match self.retention {
            Retention::Strict(_) => true,
            Retention::Capped(limit) => self.length <= u64::from(limit),
            Retention::Discard => false,
        };
        if retain {
            for &byte in bytes {
                self.bytes.push(byte).expect("room was reserved for the character");
            }
        }
    }
}

/// Where the decoding of a string's text is, between two of its bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Text {
    /// Between characters: a character, an escape or the closing quote
    /// comes next.
    Plain,
    /// Within a character of UTF-8, whose first byte is in the text.
    Char { more: u8, low: u8, high: u8 },
    /// After a backslash: what it escapes comes next.
    Escape,
    /// Within a `\u` escape: `digits` of its four hex digits read, making
    /// `unit`.
    Unicode { digits: u8, unit: u16 },
    /// After the `\u` escape of a high surrogate, which only the escape of
    /// a low one may follow: its backslash comes next.
    High { high: u16 },
    /// After that backslash: its `u` comes next.
    HighEscape { high: u16 },
    /// Within the low surrogate's `\u` escape, after `high`.
    Low { high: u16, digits: u8, unit: u16 },
}

/// How a piece of a string's text left the string.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Piece {
    /// It goes on, in this state.
    More(Text),
    /// The closing quote was the piece's last byte: the string is whole.
    Closed,
}

/// One decoded fragment, at most one scalar, held entirely inline.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Decoded {
    pub(crate) after: Option<Text>,
    bytes: [u8; 4],
    length: u8,
    /// Reserve the whole character at its first byte, before its continuation.
    room: u8,
}

impl Decoded {
    pub(crate) fn bytes(&self) -> &[u8] {
        self.bytes.get(..usize::from(self.length)).expect("at most four bytes")
    }

    fn empty(after: Option<Text>) -> Decoded {
        Decoded { after, bytes: [0; 4], length: 0, room: 0 }
    }

    fn byte(after: Text, byte: u8, room: u8) -> Decoded {
        Decoded { after: Some(after), bytes: [byte, 0, 0, 0], length: 1, room }
    }
}

/// Decodes `piece`, the next bytes of a string's text after `text`, onto
/// `out`, under its retention limit. `first` is the offset of the piece's
/// first byte in the document. A closing quote is last.
pub(crate) fn decode(text: Text, piece: &[u8], first: u32, out: &mut Buffer) -> Result<Piece, Error> {
    let mut text = text;
    // Bounded by the piece, which is at most the scan's maximum.
    for (at, &byte) in piece.iter().enumerate() {
        if text == Text::Plain {
            out.begun = first
                .checked_add(u32::try_from(at).expect("a scan's length fits u32"))
                .expect("within the document limit");
        }
        let decoded = next(text, byte)?;
        out.room(u32::from(decoded.room))?;
        out.put(decoded.bytes());
        match decoded.after {
            Some(after) => text = after,
            None => {
                assert!(at.checked_add(1) == Some(piece.len()), "a scan ends at its first quote");
                return Ok(Piece::Closed);
            }
        }
    }
    Ok(Piece::More(text))
}

/// Decodes one byte without allocating or retaining its string. The tokenizer
/// retains the fragment; borrowed JSON spelling writes it immediately.
pub(crate) fn next(text: Text, byte: u8) -> Result<Decoded, Error> {
    match text {
        Text::Plain => plain(byte),
        Text::Char { more, low, high } => {
            let after = Utf8::Within { more, low, high }.next(byte).ok_or(Error::Utf8)?;
            // The room for the whole character was made at its first byte.
            Ok(Decoded::byte(character(after), byte, 0))
        }
        Text::Escape => escape(byte),
        Text::Unicode { digits, unit } => {
            let unit = hex(unit, byte)?;
            match count(digits) {
                4 => unit_escaped(unit),
                digits => Ok(Decoded::empty(Some(Text::Unicode { digits, unit }))),
            }
        }
        Text::High { high } => match byte {
            b'\\' => Ok(Decoded::empty(Some(Text::HighEscape { high }))),
            _ => Err(Error::Surrogate),
        },
        Text::HighEscape { high } => match byte {
            b'u' => Ok(Decoded::empty(Some(Text::Low { high, digits: 0, unit: 0 }))),
            _ => Err(Error::Surrogate),
        },
        Text::Low { high, digits, unit } => {
            let unit = hex(unit, byte)?;
            match count(digits) {
                4 => pair_escaped(high, unit),
                digits => Ok(Decoded::empty(Some(Text::Low { high, digits, unit }))),
            }
        }
    }
}

/// A byte between characters.
fn plain(byte: u8) -> Result<Decoded, Error> {
    match byte {
        b'"' => Ok(Decoded::empty(None)),
        b'\\' => Ok(Decoded::empty(Some(Text::Escape))),
        0x00..=0x1F => Err(Error::Control),
        _ => {
            let after = Utf8::Between.next(byte).ok_or(Error::Utf8)?;
            // The whole character must fit, decided at its first byte, so
            // that whether it is too long does not wait on whether it is UTF-8.
            let width = match after {
                Utf8::Between => 1,
                Utf8::Within { more, .. } => more.checked_add(1).expect("at most four bytes"),
            };
            Ok(Decoded::byte(character(after), byte, width))
        }
    }
}

/// The state within or after a character of UTF-8.
fn character(utf8: Utf8) -> Text {
    match utf8 {
        Utf8::Between => Text::Plain,
        Utf8::Within { more, low, high } => Text::Char { more, low, high },
    }
}

/// The byte after a backslash.
fn escape(byte: u8) -> Result<Decoded, Error> {
    let unescaped = match byte {
        b'"' => b'"',
        b'\\' => b'\\',
        b'/' => b'/',
        b'b' => 0x08,
        b'f' => 0x0C,
        b'n' => b'\n',
        b'r' => b'\r',
        b't' => b'\t',
        b'u' => return Ok(Decoded::empty(Some(Text::Unicode { digits: 0, unit: 0 }))),
        _ => return Err(Error::Escape),
    };
    Ok(Decoded::byte(Text::Plain, unescaped, 1))
}

/// A `\u` escape's four digits read, outside a surrogate pair.
fn unit_escaped(unit: u16) -> Result<Decoded, Error> {
    match unit {
        0xD800..=0xDBFF => Ok(Decoded::empty(Some(Text::High { high: unit }))),
        0xDC00..=0xDFFF => Err(Error::Surrogate),
        _ => Ok(scalar(u32::from(unit))),
    }
}

/// The low surrogate's four digits read, after `high`.
fn pair_escaped(high: u16, low: u16) -> Result<Decoded, Error> {
    if !(0xDC00..=0xDFFF).contains(&low) {
        return Err(Error::Surrogate);
    }
    // Ten bits from each half, above the basic plane.
    let bits = (u32::from(high) & 0x3FF).wrapping_shl(10) | (u32::from(low) & 0x3FF);
    Ok(scalar(bits.checked_add(0x1_0000).expect("at most U+10FFFF")))
}

/// The UTF-8 of the scalar value `point`, which an escape spelled.
fn scalar(point: u32) -> Decoded {
    let character = char::from_u32(point).expect("neither a surrogate nor past U+10FFFF");
    let mut bytes = [0; 4];
    let length = u8::try_from(character.encode_utf8(&mut bytes).len()).expect("at most four bytes");
    Decoded { after: Some(Text::Plain), bytes, length, room: length }
}

/// `unit` with the hex digit `byte` appended.
fn hex(unit: u16, byte: u8) -> Result<u16, Error> {
    let value = match byte {
        b'0'..=b'9' => byte.wrapping_sub(b'0'),
        b'a'..=b'f' => byte.wrapping_sub(b'a').wrapping_add(10),
        b'A'..=b'F' => byte.wrapping_sub(b'A').wrapping_add(10),
        _ => return Err(Error::Escape),
    };
    // At most three digits are in `unit`, so nothing is shifted out.
    Ok(unit.wrapping_shl(4) | u16::from(value))
}

/// The digits read, one more.
fn count(digits: u8) -> u8 {
    digits.checked_add(1).expect("at most four digits")
}
