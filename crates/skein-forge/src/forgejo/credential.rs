//! Forgejo's authorization field (forge.md, sections 5.1 and 8).
use crate::credential::Credential;
use skein_lib::{Secret, Writer};

/// Renders a credential with the dialect's prefix, retaining its redaction.
#[must_use]
pub fn authorization(credential: &Credential) -> Option<Secret> {
    let token = match credential {
        Credential::Token(token) => token,
        // Credential::App gets its meaning in runtime 02.
        Credential::App { .. } => return None,
    };
    let prefix = b"token ";
    let mut out = Writer::new(prefix.len().checked_add(token.len())?);
    out.put(prefix).ok()?;
    out.put(token.expose()).ok()?;
    Some(Secret::new(out.finish()))
}
