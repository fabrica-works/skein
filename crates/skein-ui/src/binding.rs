//! Bindings and acts (skein's ui.md, section 6).

use alloc::boxed::Box;
use skein_lib::Token;

/// A meaning kept by its owner's view for the last built tree.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Binding {
    pub owner: crate::tree::Owner,
    pub token: Token,
}

/// The shell's act, with a field's whole value owned by the event.
#[derive(PartialEq, Eq, Debug)]
pub enum Act {
    /// Press a button or follow a link.
    Press,
    /// Replace a field's typed value.
    Input { value: Box<[u8]> },
    /// Submit a form or composer.
    Submit,
    /// Toggle a choice or check.
    Toggle,
    /// Bytes of the drawing's owner, reported by its drawing code in a shell.
    Drawing { data: Box<[u8]> },
}

/// An act resolved to the owner that decodes it into a typed domain action.
#[derive(PartialEq, Eq, Debug)]
pub struct BoundAct {
    pub binding: Binding,
    pub act: Act,
}

/// A field's ASCII digits as a count; empty, non-digits and overflow are absent.
#[must_use]
pub fn number(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return None;
    }
    let mut value = 0_u64;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        let digit = byte.checked_sub(b'0')?;
        value = value.checked_mul(10)?.checked_add(u64::from(digit))?;
    }
    Some(value)
}
