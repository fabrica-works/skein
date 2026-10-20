//! Best effort webhooks: item bodies, branch moves and job ends, after a
//! drawn latency, late, or lost. Commit statuses and wiki writes have none.

use alloc::boxed::Box;

use skein_lib::bytes::copy_of;
use skein_lib::{Env, Id, Queue};

use crate::api::{Change, Ended};
use crate::boundary::Request;
use crate::domain::{Alarm, Config, Domain};
use crate::faults;
use crate::store::Repository;

/// What a webhook tells of a change.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Hook {
    change: Change,
    number: Option<u64>,
    branch: Option<Box<[u8]>>,
    commit: Option<u64>,
    by: Option<u64>,
    body: Option<Box<[u8]>>,
    ended: Option<Ended>,
}

impl Hook {
    /// A change of `change` to the item `number`.
    pub(crate) fn item(change: Change, number: u64, by: u64) -> Hook {
        Hook { change, number: Some(number), branch: None, commit: None, by: Some(by), body: None, ended: None }
    }

    /// `branch` moved to `commit`, or was deleted.
    pub(crate) fn push(branch: &[u8], commit: Option<u64>, by: u64) -> Hook {
        Hook {
            change: Change::Push,
            number: None,
            branch: Some(copy_of(branch)),
            commit,
            by: Some(by),
            body: None,
            ended: None,
        }
    }

    /// A text-bearing issue, comment or review; a pull carries no body.
    pub(crate) fn body(change: Change, number: u64, by: u64, body: &[u8]) -> Hook {
        Hook { body: Some(copy_of(body)), ..Self::item(change, number, by) }
    }

    /// A queue group made or ended; its commit identifies the group.
    pub(crate) fn group(commit: u64, by: u64, ended: bool) -> Hook {
        Hook {
            change: Change::Group { ended },
            number: None,
            branch: None,
            commit: Some(commit),
            by: Some(by),
            body: None,
            ended: None,
        }
    }

    /// A job ended with its run, commit and verdict.
    pub(crate) fn job(ended: Ended, by: u64) -> Hook {
        Hook {
            change: Change::Job,
            number: None,
            branch: None,
            commit: None,
            by: Some(by),
            body: None,
            ended: Some(ended),
        }
    }
}

/// A webhook on its way.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Delivery {
    repository: Id<Repository>,
    hook: Hook,
}

/// Something changed in `repository`: its subscriber, if it has one, hears of
/// it later, or never.
pub(crate) fn notify(domain: &mut Domain, env: &Env<Config>, repository: Id<Repository>, mut hook: Hook) {
    if !domain.repositories.get(repository).expect("a repository of the forge").hooked {
        return;
    }
    if hook.change == Change::Issue {
        let repository = domain.repositories.get(repository).expect("a repository of the forge");
        let item = repository.items.get(&hook.number.expect("an issue number")).expect("a hooked issue");
        hook.body = Some(copy_of(&item.body));
    }
    let config = &env.limits;
    if domain.rng.chance(config.hooks_lost) {
        domain.tally.hooks_lost = domain.tally.hooks_lost.saturating_add(1);
        return;
    }
    let late = domain.rng.chance(config.hooks_late);
    let span = if late {
        faults::draw(domain, config.late_min, config.late_max)
    } else {
        faults::draw(domain, config.hook_min, config.hook_max)
    };
    if domain.deliveries.is_full() {
        domain.tally.hooks_dropped = domain.tally.hooks_dropped.saturating_add(1);
        return;
    }
    let id = domain.deliveries.insert(Delivery { repository, hook }).expect("checked for room above");
    if late {
        domain.tally.hooks_late = domain.tally.hooks_late.saturating_add(1);
    }
    domain.timers.arm(Alarm::Hook(id), env.now.saturating_add(span)).expect("a timer per delivery fits");
}

/// A delivery's timer: the webhook goes out.
pub(crate) fn deliver(domain: &mut Domain, id: Id<Delivery>, out: &mut Queue<Request>) {
    let delivery = domain.deliveries.get(id).expect("a delivery lives until its timer fires");
    let repository = domain.repositories.get(delivery.repository).expect("a repository of the forge");
    let Hook { change, number, branch, commit, by, body, ended } = &delivery.hook;
    let hook = Request::Hook {
        repository: copy_of(&repository.name),
        change: *change,
        number: *number,
        branch: branch.clone(),
        commit: *commit,
        by: *by,
        body: body.clone(),
        ended: *ended,
    };
    out.push(hook);
    domain.deliveries.retire(id);
    domain.tally.hooks = domain.tally.hooks.saturating_add(1);
}
