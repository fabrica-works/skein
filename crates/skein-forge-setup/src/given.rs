//! Owner-provided values and admission (forge-setup.md, 4.1).
use crate::limits::{self, Limits};
use alloc::boxed::Box;
use core::net::SocketAddr;
use skein_lib::Writer;
/// Owner's inputs; the checked machine copies these for one run.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Given {
    pub forge: u16,
    pub bot_login: Box<[u8]>,
    pub bot_email: Box<[u8]>,
    pub admin_name: Box<[u8]>,
    pub api_name: Box<[u8]>,
    pub git_name: Box<[u8]>,
    pub orgs: Box<[Box<[u8]>]>,
    pub org_owner: Option<Box<[u8]>>,
    pub listener: Option<SocketAddr>,
    pub word: Box<[u8]>,
    pub limits: Limits,
}
/// Refusal locations; the owner maps them to its configuration paths.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Place {
    Word,
    BotLogin,
    BotEmail,
    AdminName,
    ApiName,
    GitName,
    Orgs,
    Org(u32),
    OrgOwner,
    OrgsLimit,
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
        (login(&given.bot_login), Place::BotLogin),
        (email(&given.bot_email), Place::BotEmail),
        (!given.admin_name.is_empty(), Place::AdminName),
        (!given.api_name.is_empty(), Place::ApiName),
        (!given.git_name.is_empty(), Place::GitName),
    ] {
        if !valid {
            return Err(Refused { place, other: None });
        }
    }
    for (at, name) in given.orgs.iter().enumerate() {
        if !login(name) {
            return Err(Refused { place: Place::Org(u32::try_from(at).expect("bounded orgs")), other: None });
        }
    }
    if let Some(owner) = &given.org_owner
        && !login(owner)
    {
        return Err(Refused { place: Place::OrgOwner, other: None });
    }
    limits::check(&given.limits)?;
    if given.orgs.is_empty() || given.orgs.len() > usize::try_from(given.limits.orgs).expect("u32 fits") {
        return Err(Refused { place: Place::Orgs, other: None });
    }
    Ok(())
}
pub(crate) fn marker(word: &[u8]) -> Box<[u8]> {
    let mut out = Writer::new(word.len().checked_add(4).expect("checked word"));
    out.put(word).expect("word capacity");
    out.put(b":bot").expect("marker capacity");
    out.finish()
}
/// Checks the forge login grammar.
#[must_use]
pub fn login(b: &[u8]) -> bool {
    if b.is_empty() || b.len() > 40 || !b.first().expect("not empty").is_ascii_alphanumeric() {
        return false;
    }
    for c in b {
        if !c.is_ascii_alphanumeric() && !b"._-".contains(c) {
            return false;
        }
    }
    true
}
/// Checks the forge email grammar.
#[must_use]
pub fn email(b: &[u8]) -> bool {
    if !(3..=254).contains(&b.len()) {
        return false;
    }
    let mut ats = 0_u32;
    for c in b {
        if *c == b'@' {
            ats = ats.saturating_add(1);
        }
        if c.is_ascii_whitespace() || c.is_ascii_control() || b"<>".contains(c) {
            return false;
        }
    }
    ats == 1
}
