//! Forgejo plan storage, bounded by the given and host limits (6.5).
use crate::{given::Given, host::Limits};
use alloc::boxed::Box;
use skein_lib::List;

/// The owned text and arrays of one copied given, with a full adopted set.
pub(crate) fn copied(g: &Given) -> Option<u64> {
    let mut bytes = List::<Box<[u8]>>::worst_case(u32::try_from(g.orgs.len()).ok()?)?
        .checked_add(List::<u64>::worst_case(g.limits.repositories)?)?;
    for text in [&g.bot_login, &g.bot_email, &g.admin_name, &g.api_name, &g.git_name, &g.word] {
        bytes = bytes.checked_add(u64::try_from(text.len()).ok()?)?;
    }
    for text in &g.orgs {
        bytes = bytes.checked_add(u64::try_from(text.len()).ok()?)?;
    }
    if let Some(owner) = &g.org_owner {
        bytes = bytes.checked_add(u64::try_from(owner.len()).ok()?)?;
    }
    Some(bytes)
}

/// Reconcile's boxed record and copied configuration dominate the retained
/// storage of `Check`, `TakeOver` and `Adopt`. Creation holds its files; the picker additionally
/// builds its reached list while consuming a page. Charge the larger plan,
/// then common credentials, decoded path/login and hook/request scratch.
pub(crate) fn per_run(g: &Given, l: &Limits) -> Option<u64> {
    crate::given::check(g).ok()?;
    let reconcile = u64::try_from(size_of::<crate::reconcile::Reconcile>())
        .ok()?
        // given_for, constructor copies and retained child names overlap.
        .checked_add(copied(g)?.checked_mul(3)?)?;
    let creation = u64::try_from(size_of::<crate::create::Create>())
        .ok()?
        .checked_add(copied(g)?)?
        // Create retains the verified admin's login within the host decoder's text bound.
        .checked_add(u64::from(l.string_bytes))?
        .checked_add(List::<crate::host::File>::worst_case(g.limits.files)?)?
        .checked_add(u64::from(g.limits.files).checked_mul(255_u64.checked_add(u64::from(g.limits.file_bytes))?)?)?
        // Names, repository path and pending file request may overlap the plan.
        .checked_add(u64::from(l.string_bytes).checked_mul(8)?)?
        .checked_add(u64::from(g.limits.file_bytes).checked_add(255)?.checked_mul(2)?)?;
    let picker = List::<crate::reachable::Reached>::worst_case(g.limits.reachable)?;
    let hook = match g.listener {
        Some(address) => match crate::reconcile::hook_url(address, g.forge) {
            Some(url) => u64::try_from(url.len()).ok()?,
            None => 0,
        },
        None => 0,
    };
    let mut events =
        List::<Box<[u8]>>::worst_case(u32::try_from(skein_forge::forgejo::delivery::SUBSCRIBED.len()).ok()?)?;
    for event in skein_forge::forgejo::delivery::SUBSCRIBED {
        events = events.checked_add(u64::try_from(event.len()).ok()?)?;
    }
    reconcile
        .max(picker)
        .max(creation)
        .checked_add(u64::from(l.token_bytes.max(l.string_bytes)).checked_mul(4)?)?
        .checked_add(u64::from(l.string_bytes).checked_mul(6)?)?
        .checked_add(hook.checked_mul(4)?)?
        .checked_add(events.checked_mul(2)?)?
        // CreateToken's three scopes and generated marker/password/decimal text.
        .checked_add(List::<Box<[u8]>>::worst_case(3)?)?
        .checked_add(256)
}
