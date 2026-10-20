//! Owner-provided values and admission (forge-setup.md, 4.1).
use crate::limits::{self, Limits};
use alloc::boxed::Box;
use core::net::SocketAddr;
use skein_lib::{List, Writer};
/// Owner's inputs; the checked machine copies these for one run.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Given {
    pub forge: u16,
    pub bot_login: Box<[u8]>,
    pub bot_email: Box<[u8]>,
    pub admin_name: Box<[u8]>,
    pub api_name: Box<[u8]>,
    pub git_name: Box<[u8]>,
    /// The orgs to make when absent, in order; none when the forge is managed.
    pub orgs: Box<[Box<[u8]>]>,
    /// Who owns the orgs made; set with `orgs`.
    pub org_owner: Option<Box<[u8]>>,
    /// The repositories adopted, by id, in the order given; none when no repository is.
    pub adopted: Box<[u64]>,
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
    Repositories,
    Repository(u32),
    OrgsLimit,
    RepositoriesLimit,
    ReachableLimit,
    Page,
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
    if given.orgs.len() > usize::try_from(given.limits.orgs).expect("u32 fits") {
        return Err(Refused { place: Place::Orgs, other: None });
    }
    if !given.orgs.is_empty() && given.org_owner.is_none() {
        return Err(Refused { place: Place::Orgs, other: Some(Place::OrgOwner) });
    }
    if given.adopted.len() > usize::try_from(given.limits.repositories).expect("u32 fits") {
        return Err(Refused { place: Place::Repositories, other: None });
    }
    for (at, id) in given.adopted.iter().enumerate() {
        if *id == 0 {
            return Err(Refused {
                place: Place::Repository(u32::try_from(at).expect("bounded repositories")),
                other: None,
            });
        }
    }
    let count = u32::try_from(given.adopted.len()).expect("bounded repositories");
    let mut ids = List::with_capacity(count);
    for (at, id) in given.adopted.iter().enumerate() {
        ids.push((*id, u32::try_from(at).expect("bounded repositories"))).expect("checked repository count");
    }
    let mut ids = ids.into_boxed();
    ids.sort_unstable();
    let mut previous = None;
    let mut duplicate = None;
    for &(id, later) in &ids {
        if let Some((prior, earlier)) = previous
            && prior == id
        {
            // Tuple order chooses the first repeat in the order given.
            let pair = (later, earlier);
            duplicate = Some(match duplicate {
                Some(found) => core::cmp::min(found, pair),
                None => pair,
            });
        }
        previous = Some((id, later));
    }
    if let Some((later, earlier)) = duplicate {
        return Err(Refused { place: Place::Repository(later), other: Some(Place::Repository(earlier)) });
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
