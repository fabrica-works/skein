//! Owner-supplied credentials (forge.md, section 8).
use skein_lib::Secret;

/// A credential whose debug view delegates to the secret's redaction.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Credential {
    /// A permanent token, until revoked; no renewal plan.
    Token(Secret),
    /// The App's id and key, which the mint turns into installation tokens.
    App {
        /// The App's id on its forge.
        id: u64,
        /// A 2048-bit PKCS#1 `RSAPrivateKey` DER for the mint's signatures.
        key: Secret,
    },
}
