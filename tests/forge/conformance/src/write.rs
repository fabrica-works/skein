use crate::Result;
use skein_throwaway_forgejo::Instance;
use std::{fs, path::Path};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    Password,
    ApiToken,
    GitHeader,
    RunnerSecret,
    HookSecret,
}
pub struct Secrets(Vec<(SecretKind, String)>);
impl Secrets {
    pub fn add(&mut self, kind: SecretKind, value: &str) {
        self.0.push((kind, value.into()));
    }
    #[must_use]
    pub fn of(values: &[(SecretKind, &str)]) -> Self {
        Self(values.iter().map(|(kind, value)| (*kind, (*value).into())).collect())
    }
    #[must_use]
    pub fn from_instance(instance: &Instance) -> Self {
        Self(
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
        )
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct Leak {
    pub case: String,
    pub kind: SecretKind,
}
impl std::fmt::Display for Leak {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {:?} reached a transcript", self.case, self.kind)
    }
}
impl std::error::Error for Leak {}
/// # Errors
/// Reports the case and kind only when a secret reaches either byte stream.
pub fn check(case: &str, request: &[u8], response: &[u8], secrets: &Secrets) -> std::result::Result<(), Leak> {
    for (kind, value) in &secrets.0 {
        if !value.is_empty()
            && [request, response].iter().any(|bytes| bytes.windows(value.len()).any(|w| w == value.as_bytes()))
        {
            return Err(Leak { case: case.into(), kind: *kind });
        }
    }
    Ok(())
}
/// # Errors
/// Scans both files before replacing the case directory or writing any bytes.
pub fn case(out: &Path, name: &str, request: &[u8], response: &[u8], secrets: &Secrets) -> Result<()> {
    check(name, request, response, secrets)?;
    let dir = out.join("transcripts").join(name);
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("request"), request)?;
    fs::write(dir.join("response"), response)?;
    Ok(())
}

pub(crate) fn delivery(out: &Path, name: &str, request: &[u8], response: &[u8], secrets: &Secrets) -> Result<()> {
    check(name, request, response, secrets)?;
    let dir = out.join("deliveries").join(name);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("request"), request)?;
    fs::write(dir.join("response"), response)?;
    Ok(())
}
