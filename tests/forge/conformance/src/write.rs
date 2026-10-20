//! Forgejo's credential adapter to the shared transcript writer.
pub use skein_forge_conformance::write::{Leak, SecretKind, case, check, delivery};
use skein_throwaway_forgejo::Instance;
pub struct Secrets(skein_forge_conformance::write::Secrets);
impl Secrets {
    #[must_use]
    pub fn of(values: &[(SecretKind, &str)]) -> Self {
        Self(skein_forge_conformance::write::Secrets::of(values))
    }
    #[must_use]
    pub fn from_instance(instance: &Instance) -> Self {
        Self(skein_forge_conformance::write::Secrets::owned(
            instance
                .secrets()
                .into_iter()
                .map(|s| {
                    let kind = if s.len() == 40 {
                        SecretKind::ApiToken
                    } else if s.len() == 64 {
                        SecretKind::Password
                    } else {
                        SecretKind::GitHeader
                    };
                    (kind, s)
                })
                .collect(),
        ))
    }
}
impl std::ops::Deref for Secrets {
    type Target = skein_forge_conformance::write::Secrets;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Secrets {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
