//! Refuse leaked secrets before writing any transcript bytes.
use crate::Result;
use std::{fs, path::Path};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    Password,
    ApiToken,
    GitHeader,
    RunnerSecret,
    HookSecret,
    AppKey,
    Jwt,
    PersonToken,
    MintedToken,
}
pub struct Secrets(Vec<(SecretKind, String)>);
impl Secrets {
    #[must_use]
    pub fn owned(values: Vec<(SecretKind, String)>) -> Self {
        Self(values)
    }
    pub fn add(&mut self, kind: SecretKind, value: &str) {
        self.0.push((kind, value.into()));
    }
    #[must_use]
    pub fn of(values: &[(SecretKind, &str)]) -> Self {
        Self(values.iter().map(|(kind, value)| (*kind, (*value).into())).collect())
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
        if !value.is_empty() && (contains(request, value.as_bytes()) || contains(response, value.as_bytes())) {
            return Err(Leak { case: case.into(), kind: *kind });
        }
    }
    Ok(())
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    let (Some(&first), Some(&last)) = (needle.first(), needle.last()) else { return false };
    let Some(end) = bytes.len().checked_sub(needle.len()) else { return false };
    let tail = needle.len() - 1;
    let mut at = 0;
    // The needle is nonempty and at <= bytes.len() - needle.len(), so both
    // indices and the window end fit; the next position is at most bytes.len().
    while at <= end {
        if bytes[at] == first && bytes[at + tail] == last && bytes[at..at + needle.len()] == *needle {
            return true;
        }
        at += 1;
    }
    false
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

/// # Errors
/// Refuses a leaked secret or returns a directory or file write failure.
pub fn delivery(out: &Path, name: &str, request: &[u8], response: &[u8], secrets: &Secrets) -> Result<()> {
    check(name, request, response, secrets)?;
    let dir = out.join("deliveries").join(name);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("request"), request)?;
    fs::write(dir.join("response"), response)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Leak, SecretKind, Secrets, check};

    #[test]
    fn either_stream_refuses_an_exact_nonempty_secret() {
        let rows = [
            ("empty secret", "", b"bytes".as_slice(), false),
            ("empty stream", "secret", b"".as_slice(), false),
            ("one byte present", "x", b"axb".as_slice(), true),
            ("one byte absent", "x", b"abc".as_slice(), false),
            ("equal length", "secret", b"secret".as_slice(), true),
            ("longer needle", "secret", b"secre".as_slice(), false),
            ("first position", "secret", b"secret suffix".as_slice(), true),
            ("last position", "secret", b"prefix secret".as_slice(), true),
            ("periodic near miss", "ababac", b"abababababab".as_slice(), false),
            ("periodic match", "ababac", b"abababababac".as_slice(), true),
            ("equal ends different middle", "a-b-a", b"xa-x-ay".as_slice(), false),
            ("adjacent failed candidate then match", "aab", b"aaab".as_slice(), true),
            ("binary match", "key", b"\xff\0key\x80".as_slice(), true),
            ("binary near miss", "key", b"\xff\0kex\x80".as_slice(), false),
        ];
        for (name, secret, bytes, leaks) in rows {
            let secrets = Secrets::of(&[(SecretKind::ApiToken, secret)]);
            for (stream, request, response) in [("request", bytes, b"".as_slice()), ("response", b"".as_slice(), bytes)]
            {
                let expected = if leaks { Err(Leak { case: name.into(), kind: SecretKind::ApiToken }) } else { Ok(()) };
                assert_eq!(check(name, request, response, &secrets), expected, "{name}: {stream}");
            }
        }
    }

    #[test]
    fn registration_order_selects_the_first_leak_kind_and_case() {
        let secrets = Secrets::of(&[
            (SecretKind::ApiToken, "response-secret"),
            (SecretKind::Password, "request-secret"),
            (SecretKind::HookSecret, "response-secret"),
        ]);
        assert_eq!(
            check("both streams", b"request-secret", b"response-secret", &secrets),
            Err(Leak { case: "both streams".into(), kind: SecretKind::ApiToken })
        );
        assert_eq!(
            check("request only", b"request-secret", b"", &secrets),
            Err(Leak { case: "request only".into(), kind: SecretKind::Password })
        );
    }

    #[test]
    fn a_newly_registered_secret_is_found_in_captured_bytes() {
        let request = b"GET / HTTP/1.1\r\n";
        let response = b"captured-token";
        let mut secrets = Secrets::of(&[(SecretKind::Password, "different-token")]);
        assert_eq!(check("exchange", request, response, &secrets), Ok(()));
        secrets.add(SecretKind::MintedToken, "captured-token");
        assert_eq!(
            check("flush", request, response, &secrets),
            Err(Leak { case: "flush".into(), kind: SecretKind::MintedToken })
        );
    }
}
