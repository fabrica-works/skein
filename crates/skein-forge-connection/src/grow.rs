//! Forge connection grow (forge-connection.md, sections 3 and 8).
use alloc::boxed::Box;
use skein_lib::List;

/// A write past the cap, refused whole: nothing written.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Full;

/// Bytes of unknown length, at most `cap`.
#[derive(Debug)]
pub struct Grow {
    bytes: List<u8>,
    cap: u32,
}

impl Grow {
    pub(crate) fn len(&self) -> u32 {
        self.bytes.len()
    }
    pub(crate) fn room(&self) -> u32 {
        self.cap.saturating_sub(self.bytes.len())
    }
    /// Starts at `first` bytes (at most `cap`).
    #[must_use]
    pub fn new(first: u32, cap: u32) -> Grow {
        Grow { bytes: List::with_capacity(first.min(cap)), cap }
    }

    /// Appends `piece` whole, growing to twice the capacity or what it needs, never past `cap`.
    pub fn write(&mut self, piece: &[u8]) -> Result<(), Full> {
        let Ok(length) = u32::try_from(piece.len()) else { return Err(Full) };
        let Some(need) = self.bytes.len().checked_add(length) else { return Err(Full) };
        if need > self.cap {
            return Err(Full);
        }
        if need > self.bytes.capacity() {
            let mut grown = List::with_capacity(need.max(self.bytes.capacity().saturating_mul(2)).min(self.cap));
            for byte in self.bytes.as_slice() {
                grown.push(*byte).expect("grown past the length");
            }
            self.bytes = grown;
        }
        for byte in piece {
            self.bytes.push(*byte).expect("room checked above");
        }
        Ok(())
    }

    /// The bytes, in a box of exactly their length.
    #[must_use]
    pub fn finish(self) -> Box<[u8]> {
        self.bytes.into_boxed()
    }
}
impl Grow {
    #[cfg(test)]
    pub(crate) fn capacity(&self) -> u32 {
        self.bytes.capacity()
    }
}
/// A body's last `max` bytes, replacing the oldest once full.
#[derive(Debug)]
pub struct Tail {
    grow: Grow,
    start: u32,
    dropped: bool,
}
impl Tail {
    /// Starts at `first` bytes, growing up to `max`.
    #[must_use]
    pub fn new(first: u32, max: u32) -> Self {
        Self { grow: Grow::new(first, max), start: 0, dropped: false }
    }
    /// Appends each byte, replacing the oldest when full.
    pub fn write(&mut self, piece: &[u8]) {
        for byte in piece {
            if self.grow.room() > 0 {
                self.grow.write(core::slice::from_ref(byte)).expect("Room for one byte");
            } else {
                self.dropped = true;
                if self.grow.cap > 0 {
                    *self.grow.bytes.get_mut(self.start).expect("Full tail") = *byte;
                    self.start = self.start.saturating_add(1).checked_rem(self.grow.cap).expect("Nonzero tail");
                }
            }
        }
    }
    /// Whether any earlier byte was dropped.
    #[must_use]
    pub fn dropped(&self) -> bool {
        self.dropped
    }
    /// The retained bytes, oldest first, in a box of their length.
    #[must_use]
    pub fn finish(self) -> Box<[u8]> {
        let mut bytes = self.grow.finish();
        bytes.rotate_left(usize::try_from(self.start).expect("Bounded position"));
        bytes
    }
    #[cfg(test)]
    pub(crate) fn capacity(&self) -> u32 {
        self.grow.capacity()
    }
}
