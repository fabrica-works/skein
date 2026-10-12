//! io addresses and inline in-process names (io.md, sections 2 and 4).

use core::net::SocketAddr;

/// Where io listens or connects.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Addr {
    /// The kernel's socket address.
    Ip(SocketAddr),
    /// A listener in this io's own namespace (io.md, 3.5).
    InProcess(Name),
}

impl Addr {
    /// The kernel socket address, when this address names one.
    #[must_use]
    pub const fn socket(&self) -> Option<SocketAddr> {
        match self {
            Addr::Ip(socket) => Some(*socket),
            Addr::InProcess(_) => None,
        }
    }
}

/// An inline name, compared as bytes (io.md, section 4).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Name {
    bytes: [u8; 32],
    len: u8,
}

impl Name {
    /// The longest name, in bytes.
    pub const MAX: usize = 32;
    /// A connector's name, told as the accepted socket's peer.
    pub const EMPTY: Name = Name { bytes: [0; 32], len: 0 };

    /// The name of `bytes`, or `None` past `MAX`.
    #[must_use]
    pub fn new(bytes: &[u8]) -> Option<Name> {
        if bytes.len() > Self::MAX {
            return None;
        }
        let mut name = Self::EMPTY;
        name.len = u8::try_from(bytes.len()).ok()?;
        for (to, byte) in name.bytes.iter_mut().zip(bytes) {
            *to = *byte;
        }
        Some(name)
    }

    /// Its bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..usize::from(self.len)).expect("a name's length is within MAX")
    }
}
