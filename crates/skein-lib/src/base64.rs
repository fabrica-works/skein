//! Canonical, bounded RFC 4648 text (lib.md, 6.1).
#![expect(clippy::disallowed_types, reason = "the decoder owns its bounded byte buffer")]
use crate::{Overflow, Writer};
use alloc::{boxed::Box, vec::Vec};

/// An alphabet and its padding rule.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Alphabet {
    /// Standard alphabet, padded to groups of four.
    Standard,
    /// URL alphabet, without padding.
    Url,
}
/// Why text was refused.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Error {
    /// A symbol outside the alphabet.
    Symbol { at: u32 },
    /// Padding outside its permitted position.
    Padding { at: u32 },
    /// A length no encoding has.
    Length,
    /// Nonzero trailing bits.
    Bits,
    /// More decoded bytes than the maximum.
    TooLong { max: u32 },
}
/// Text length, measured before allocation.
#[must_use]
pub fn encoded_len(alphabet: Alphabet, bytes: u32) -> Option<u32> {
    let whole = bytes.checked_div(3)?.checked_mul(4)?;
    let tail = bytes % 3;
    let extra = match alphabet {
        Alphabet::Standard => {
            if tail == 0 {
                0
            } else {
                4
            }
        }
        Alphabet::Url => {
            if tail == 0 {
                0
            } else {
                tail.checked_add(1)?
            }
        }
    };
    whole.checked_add(extra)
}
/// Writes the whole encoding, or writes nothing if room is insufficient.
pub fn encode(alphabet: Alphabet, bytes: &[u8], out: &mut Writer) -> Result<(), Overflow> {
    let size = encoded_len(alphabet, u32::try_from(bytes.len()).or(Err(Overflow))?).ok_or(Overflow)?;
    if usize::try_from(size).or(Err(Overflow))? > out.room() {
        return Err(Overflow);
    }
    let table: &[u8] = match alphabet {
        Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
        Alphabet::Url => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
    };
    for part in bytes.chunks(3) {
        let a = *part.first().expect("nonempty chunk");
        let b = part.get(1).copied().unwrap_or(0);
        let c = part.get(2).copied().unwrap_or(0);
        let digits = [a >> 2_u32, ((a & 3) << 4_u32) | (b >> 4_u32), ((b & 15) << 2_u32) | (c >> 6_u32), c & 63];
        let count = part.len().checked_add(1).expect("at most four digits");
        for value in digits.get(..count).expect("at most four digits") {
            out.put(&[*table.get(usize::from(*value)).expect("six bit digit")])?;
        }
        match alphabet {
            Alphabet::Standard => {
                for _ in count..4 {
                    out.put(b"=")?;
                }
            }
            Alphabet::Url => {}
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct State {
    alphabet: Alphabet,
    at: u32,
    len: u32,
    carry: u16,
    bits: u32,
    padding: u32,
}
impl State {
    fn new(alphabet: Alphabet) -> State {
        State { alphabet, at: 0, len: 0, carry: 0, bits: 0, padding: 0 }
    }
    fn push(&mut self, byte: u8, max: u32) -> Result<Option<u8>, Error> {
        let at = self.at;
        self.at = at.checked_add(1).ok_or(Error::Length)?;
        if byte == b'=' {
            match self.alphabet {
                Alphabet::Url => return Err(Error::Padding { at }),
                Alphabet::Standard => {}
            }
            let needed = match self.bits {
                4 => 2,
                2 => 1,
                _ => return Err(Error::Padding { at }),
            };
            if self.padding >= needed {
                return Err(Error::Padding { at });
            }
            if self.carry != 0 {
                return Err(Error::Bits);
            }
            self.padding = self.padding.checked_add(1).expect("at most two pads");
            return Ok(None);
        }
        if self.padding != 0 {
            return Err(Error::Padding { at });
        }
        let table: &[u8] = match self.alphabet {
            Alphabet::Standard => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
            Alphabet::Url => b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
        };
        let mut value = None;
        for (index, symbol) in table.iter().enumerate() {
            if *symbol == byte {
                value = Some(index);
                break;
            }
        }
        let value = value.ok_or(Error::Symbol { at })?;
        self.carry = (self.carry << 6_u32) | u16::try_from(value).expect("six bit digit");
        self.bits = self.bits.checked_add(6).expect("at most twelve bits");
        if self.bits < 8 {
            return Ok(None);
        }
        if self.len >= max {
            return Err(Error::TooLong { max });
        }
        self.len = self.len.checked_add(1).expect("below maximum");
        self.bits = self.bits.checked_sub(8).expect("one byte available");
        let byte = u8::try_from(self.carry >> self.bits).expect("eight bit byte");
        self.carry &= (1_u16 << self.bits).checked_sub(1).expect("positive bit mask");
        Ok(Some(byte))
    }
    fn finish(self) -> Result<u32, Error> {
        if self.bits == 6 {
            return Err(Error::Length);
        }
        if self.carry != 0 {
            return Err(Error::Bits);
        }
        match self.alphabet {
            Alphabet::Standard => {
                let needed = match self.bits {
                    0 => 0,
                    4 => 2,
                    2 => 1,
                    _ => return Err(Error::Length),
                };
                if self.padding != needed {
                    return Err(Error::Length);
                }
            }
            Alphabet::Url => {}
        }
        Ok(self.len)
    }
}
/// Checks the text without setting any bytes aside.
pub fn decoded_len(alphabet: Alphabet, text: &[u8]) -> Result<u32, Error> {
    let mut state = State::new(alphabet);
    for byte in text {
        state.push(*byte, u32::MAX)?;
    }
    state.finish()
}
/// Refuses invalid or oversized text before allocating its decoded buffer.
pub fn decode(alphabet: Alphabet, text: &[u8], max: u32) -> Result<Box<[u8]>, Error> {
    let len = decoded_len(alphabet, text)?;
    if len > max {
        return Err(Error::TooLong { max });
    }
    let mut decoder = Decoder::new(alphabet, max);
    decoder.push(text)?;
    decoder.finish()
}
/// Text received in pieces, with canonical padding and a byte maximum.
#[derive(Debug)]
pub struct Decoder {
    state: State,
    max: u32,
    bytes: Vec<u8>,
    failed: Option<Error>,
}
impl Decoder {
    /// Makes an empty decoder with the supplied bound.
    #[must_use]
    pub fn new(alphabet: Alphabet, max: u32) -> Decoder {
        Decoder { state: State::new(alphabet), max, bytes: Vec::new(), failed: None }
    }
    /// Checks and takes a piece; a refusal poisons the decoder.
    pub fn push(&mut self, piece: &[u8]) -> Result<(), Error> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        let mut checked = self.state;
        for byte in piece {
            if let Err(error) = checked.push(*byte, self.max) {
                self.failed = Some(error);
                return Err(error);
            }
        }
        let added = checked.len.checked_sub(self.state.len).expect("decoded length only grows");
        self.bytes.reserve_exact(usize::try_from(added).or(Err(Error::Length))?);
        for byte in piece {
            if let Some(byte) = self.state.push(*byte, self.max)? {
                self.bytes.push(byte);
            }
        }
        Ok(())
    }
    /// The decoded bytes held so far.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.state.len
    }
    /// Whether no bytes are held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Gives the bytes once the complete text has been checked.
    pub fn finish(self) -> Result<Box<[u8]>, Error> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        self.state.finish()?;
        Ok(self.bytes.into_boxed_slice())
    }
}
