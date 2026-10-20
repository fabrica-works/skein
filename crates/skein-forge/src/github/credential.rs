//! GitHub's Bearer credential (forge-github.md, sections 2 and 3).
use crate::credential::Credential;
use skein_lib::{Secret, Writer};
/// Renders a vocabulary credential, retaining its redaction.
#[must_use]
pub fn authorization(credential: &Credential) -> Option<Secret> {
    let token = match credential {
        Credential::Token(token) => token,
        // Credential::App gets its meaning in runtime 02.
        Credential::App { .. } => return None,
    };
    bearer(token)
}
/// Renders a token, installation token or App JSON Web Token.
#[must_use]
pub fn bearer(token: &Secret) -> Option<Secret> {
    let mut out = Writer::new(7usize.checked_add(token.len())?);
    out.put(b"Bearer ").ok()?;
    out.put(token.expose()).ok()?;
    Some(Secret::new(out.finish()))
}
