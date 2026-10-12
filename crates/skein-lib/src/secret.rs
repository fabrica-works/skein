//! Owned secret bytes (lib.md, 6), exposed only at their point of use.
use alloc::boxed::Box;

/// A credential, token or key whose debug view contains its length only.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Secret(Box<[u8]>);

impl Secret {
    /// Takes ownership of a secret's bytes.
    #[must_use]
    pub fn new(bytes: Box<[u8]>) -> Secret {
        Secret(bytes)
    }
    /// Reads the bytes for their owner's use.
    #[must_use]
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
    /// How many bytes the secret holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Whether it holds none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
impl core::fmt::Debug for Secret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Secret").field("len", &self.len()).finish()
    }
}
