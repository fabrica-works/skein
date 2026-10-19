//! Owner-supplied credentials (forge.md, section 8).
use skein_lib::Secret;

/// A credential whose debug view delegates to the secret's redaction.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Credential {
    /// A permanent token, until revoked; no renewal plan.
    Token(Secret),
}
