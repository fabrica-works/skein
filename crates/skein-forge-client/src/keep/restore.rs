use super::{Domain, Env, Fact, Fresh, Limits, LiveRecord, Priority, Progress, RepositoryRecord, Scan, State, What};

pub(crate) fn restore_live(d: &mut Domain, env: &Env<Limits>, record: LiveRecord) {
    assert!(!d.keep.ready, "restore before restored");
    if !super::valid_participation(&record.watch)
        || !super::valid_resource(&record.watch.resource, &env.limits)
        || record.review_page == 0
        || record.review_page > env.limits.inbox
        || record.comment_page == 0
        || record.comment_page > env.limits.inbox
        || !super::answer::valid_deliveries(&record.comments, env.limits.inbox)
        || !super::answer::valid_deliveries(&record.reviews, env.limits.inbox)
        || !crate::bounds::cached(&record.cached, &env.limits)
        || record.echoes.len() > usize::try_from(env.limits.rows).expect("u32 fits usize")
        || d.keep.live.len() >= env.limits.resources
    {
        d.keep.failed = true;
        d.fact(Fact::Refused);
        return;
    }
    let key = record.watch.resource.clone();
    assert!(
        d.keep
            .live
            .insert(key, super::live(record, &env.limits, super::next_id(&mut d.keep.sequence)))
            .expect("restore admitted")
            .is_none(),
        "one stored row per resource"
    );
}
pub(crate) fn restore_repository(d: &mut Domain, env: &Env<Limits>, record: RepositoryRecord) {
    assert!(!d.keep.ready, "restore before restored");
    if d.keep.repos.len() >= env.limits.repositories {
        d.keep.failed = true;
        d.fact(Fact::Refused);
        return;
    }
    assert!(
        d.keep.repos.insert(record.repository, super::repository(record)).expect("restore admitted").is_none(),
        "one stored row per repository"
    );
}
pub(crate) fn restored(d: &mut Domain) {
    assert!(!d.keep.ready, "restored once");
    for (key, _) in &d.keep.live {
        if d.keep.repos.get(&key.repository).is_none() {
            d.keep.failed = true;
        }
    }
    for (repository, _) in &d.keep.repos {
        let mut named = false;
        for (key, _) in &d.keep.live {
            if key.repository == *repository {
                named = true;
                break;
            }
        }
        if !named {
            d.keep.failed = true;
        }
    }
    d.keep.ready = true;
}
pub(crate) fn start_fresh(d: &mut Domain) {
    assert!(d.keep.ready && !d.keep.fresh_active, "fresh pass follows restored records once");
    d.keep.fresh_active = true;
    for key in super::keys(&d.keep) {
        let row = d.keep.live.get_mut(&key).expect("restored live resource remains");
        if row.active {
            row.fresh = match &row.record.watch.resource.what {
                What::Repository => Fresh::Complete,
                What::Branch(_) | What::Pull(_) | What::Issue(_) => Fresh::Pending,
            };
            row.state = State::Due(super::first(&row.record.watch.resource), Priority::Fresh);
        }
    }
    for key in super::repos(&d.keep) {
        let row = d.keep.repos.get_mut(&key).expect("restored repository remains");
        if row.active {
            row.fresh = Fresh::Pending;
            row.state = Scan::Due(Progress::first(row.record.mark));
        }
    }
}
pub(crate) fn fresh_done(d: &mut Domain) -> bool {
    if !d.keep.fresh_active {
        return false;
    }
    for (_, row) in &d.keep.live {
        if row.active && row.fresh == Fresh::Pending {
            return false;
        }
    }
    for (_, row) in &d.keep.repos {
        if row.active && row.fresh == Fresh::Pending {
            return false;
        }
    }
    d.keep.fresh_active = false;
    true
}
