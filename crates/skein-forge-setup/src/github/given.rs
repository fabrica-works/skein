//! Owner-provided values and admission (forge-setup.md, 5.1 and 6.3).
use super::limits::{self, Limits};
use alloc::boxed::Box;
/// Owner's inputs; the checked machine copies these for one run.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Given {
    pub forge: u16,
    /// Names the App: `<word>-<six hex>`.
    pub word: Box<[u8]>,
    /// GitHub's web origin, no trailing '/'.
    pub web: Box<[u8]>,
    /// The deployment's URL.
    pub url: Box<[u8]>,
    /// Where GitHub sends the person after the manifest.
    pub redirect_url: Box<[u8]>,
    /// Where GitHub sends the person after an install.
    pub setup_url: Box<[u8]>,
    pub limits: Limits,
}
/// The largest admitted origin or whole URL, in bytes.
pub const URL_BYTES: usize = 1024;
/// Refusal locations; the owner maps them to its configuration paths.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Place {
    Word,
    Web,
    Url,
    RedirectUrl,
    SetupUrl,
    KeyBytes,
    NameBytes,
    PageBytes,
    Installations,
    Reachable,
    Pages,
    StateLife,
    Deadline,
    Backoff,
    BackoffMax,
}
/// A refused input; no value or secret crosses this diagnostic boundary.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Refused {
    pub place: Place,
    pub other: Option<Place>,
}
/// Checks all owner-provided values before the machine copies them.
pub fn check(given: &Given) -> Result<(), Refused> {
    for (valid, place) in [
        (skein_forge::marker::valid_word(&given.word), Place::Word),
        (origin(&given.web), Place::Web),
        (url(&given.url), Place::Url),
        (url(&given.redirect_url), Place::RedirectUrl),
        (url(&given.setup_url), Place::SetupUrl),
    ] {
        if !valid {
            return Err(Refused { place, other: None });
        }
    }
    limits::check(&given.limits)
}
fn origin(value: &[u8]) -> bool {
    if !url(value) {
        return false;
    }
    let Some(host) = after_scheme(value) else {
        return false;
    };
    for byte in host {
        if b"/?#".contains(byte) {
            return false;
        }
    }
    true
}
fn url(value: &[u8]) -> bool {
    if value.is_empty() || value.len() > URL_BYTES {
        return false;
    }
    for byte in value {
        if !(0x21..=0x7e).contains(byte) {
            return false;
        }
    }
    match after_scheme(value) {
        Some(rest) => !rest.is_empty(),
        None => false,
    }
}
fn after_scheme(value: &[u8]) -> Option<&[u8]> {
    if let Some(rest) = value.strip_prefix(b"https://") { Some(rest) } else { value.strip_prefix(b"http://") }
}
