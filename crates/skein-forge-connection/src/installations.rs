//! Bounded reads and resumable changes (forge-connection.md, 4.4 and 5.3).
use crate::{Account, Change, Event, Fact, app::Jwt};
use alloc::boxed::Box;
use skein_forge::{
    api,
    github::{document, request::Scope},
};
use skein_lib::{Duration, Map, Secret, Set, Time, Wall};

#[derive(Debug)]
pub(crate) struct Row {
    pub account: Account,
    pub suspended: bool,
    pub accepted: bool,
    pub api: Option<Secret>,
    pub api_expires: Wall,
    pub git_expires: Wall,
}
impl Row {
    pub fn forget(&mut self) {
        self.api = None;
        self.api_expires = Wall::EPOCH;
        self.git_expires = Wall::EPOCH;
    }
}
#[derive(Debug)]
pub(crate) struct Metadata {
    pub account: Account,
    pub suspended: bool,
    pub seen: bool,
}
#[derive(Debug)]
pub(crate) struct Accepted {
    pub known: Option<Account>,
    pub checked: bool,
}
#[derive(Debug)]
pub(crate) struct Reading {
    pub page: u32,
    pub complete: bool,
    pub next: Duration,
    pub strays: skein_lib::List<u64>,
    pub staged: Map<u64, Metadata>,
    pub seen: Set<u64>,
    pub generation: u64,
    pub stale: bool,
    pub fact: Option<u64>,
    pub overflowed: bool,
}
impl Reading {
    pub fn new(cap: u32, generation: u64, next: Duration) -> Self {
        Self {
            page: 1,
            complete: false,
            next,
            strays: skein_lib::List::with_capacity(cap),
            staged: Map::with_capacity(cap),
            seen: Set::with_capacity(cap),
            generation,
            stale: false,
            fact: None,
            overflowed: false,
        }
    }
    /// Only completed page outcomes enter the selector.
    pub fn page(
        &mut self,
        rows: Box<[document::Installation]>,
        old: &Map<u64, Row>,
        accepted: &Set<u64>,
        name_bytes: u32,
    ) -> Result<(), api::Error> {
        for row in rows {
            if row.login.len() > usize::try_from(name_bytes).expect("name bound") {
                return Err(api::Error::TooLarge);
            }
            if old.contains_key(&row.id) {
                let _inserted = self.seen.insert(row.id).expect("seen ids belong to the old table");
            }
            if !self.staged.contains_key(&row.id) && self.staged.len() == self.staged.capacity() {
                let mut displaced = None;
                if accepted.contains(&row.id) {
                    displaced = self.strays.last().copied();
                }
                match displaced {
                    Some(id) => {
                        drop(self.staged.remove(&id));
                        self.strays.truncate(self.strays.len().checked_sub(1).expect("displaced stray"));
                        self.unkept(id);
                    }
                    None => {
                        self.unkept(row.id);
                        continue;
                    }
                }
            }
            if !accepted.contains(&row.id) && !self.staged.contains_key(&row.id) {
                self.strays.push(row.id).expect("retained stray bound");
            }
            drop(
                self.staged
                    .insert(
                        row.id,
                        Metadata {
                            account: Account { id: row.account, login: row.login },
                            suspended: row.suspended,
                            seen: true,
                        },
                    )
                    .expect("accepted rows fit; strays require room"),
            );
        }
        Ok(())
    }
    fn unkept(&mut self, id: u64) {
        if !self.overflowed {
            self.overflowed = true;
            self.fact = Some(id);
        }
    }
}
#[derive(Debug)]
pub(crate) struct Diff {
    pub old: Map<u64, Metadata>,
    pub accepted: Set<u64>,
    pub missing: Map<u64, Option<Account>>,
    pub baseline: bool,
    pub after: Option<u64>,
    pub renamed: Option<(u64, Account)>,
}
impl Diff {
    fn next(&mut self, rows: &Map<u64, Row>, forge: u16) -> Option<Event> {
        if let Some((installation, account)) = self.renamed.take() {
            return Some(Event::Installation { forge, installation, account: Some(account), change: Change::Renamed });
        }
        // At most the union of three bounded tables, without an event queue.
        for _ in 0..self.old.len().saturating_add(rows.len()).saturating_add(self.missing.len()) {
            let mut next = None;
            for (id, _) in &self.old {
                next = least_after(next, self.after, *id);
            }
            for (id, _) in rows {
                next = least_after(next, self.after, *id);
            }
            for (id, _) in &self.missing {
                next = least_after(next, self.after, *id);
            }
            let id = next?;
            self.after = Some(id);
            let old = self.old.get(&id);
            let new = rows.get(&id);
            let change = if self.baseline {
                match new {
                    Some(row) => {
                        if !self.accepted.contains(&id) {
                            Some(Change::Created)
                        } else if row.suspended {
                            Some(Change::Suspended)
                        } else {
                            None
                        }
                    }
                    None => {
                        if self.missing.contains_key(&id) {
                            Some(Change::Deleted)
                        } else {
                            None
                        }
                    }
                }
            } else {
                match new {
                    Some(row) => match old {
                        Some(before) => {
                            let renamed = row.account.login != before.account.login;
                            if row.suspended != before.suspended {
                                if renamed {
                                    self.renamed = Some((id, row.account.clone()));
                                }
                                Some(if row.suspended { Change::Suspended } else { Change::Unsuspended })
                            } else if renamed {
                                Some(Change::Renamed)
                            } else {
                                None
                            }
                        }
                        None => Some(Change::Created),
                    },
                    None => {
                        let gone = match old {
                            Some(row) => !row.seen,
                            None => false,
                        };
                        if gone || self.missing.contains_key(&id) { Some(Change::Deleted) } else { None }
                    }
                }
            };
            if let Some(change) = change {
                let account = match new {
                    Some(row) => Some(row.account.clone()),
                    None => match old {
                        Some(row) => Some(row.account.clone()),
                        None => self.missing.get(&id).cloned().flatten(),
                    },
                };
                return Some(Event::Installation { forge, installation: id, account, change });
            }
        }
        None
    }
}
fn least_after(next: Option<u64>, after: Option<u64>, id: u64) -> Option<u64> {
    if (after.is_none() || Some(id) > after) && (next.is_none() || Some(id) < next) {
        return Some(id);
    }
    next
}
#[derive(Debug)]
pub(crate) enum Read {
    Idle,
    Reading(Reading),
    Changes(Diff),
    Waiting { until: Time, next: Duration },
}
#[derive(Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "baseline consumption, read scheduling, mint staleness, identity and connect notification are independent"
)]
pub(crate) struct GitHub {
    pub jwt: Jwt,
    pub accepted: Set<u64>,
    pub known: Map<u64, Accepted>,
    pub rows: Map<u64, Row>,
    pub read: Read,
    pub generation: u64,
    pub baseline: bool,
    pub reread: bool,
    pub mint: Option<(u64, Scope)>,
    pub stale: bool,
    pub recent: Option<u64>,
    pub identity_checked: bool,
    pub status: Option<u16>,
    pub retry: Option<Duration>,
    pub connected: bool,
}
impl GitHub {
    pub fn new(cap: u32) -> Self {
        Self {
            jwt: Jwt::new(),
            accepted: Set::with_capacity(cap),
            known: Map::with_capacity(cap),
            rows: Map::with_capacity(cap),
            read: Read::Idle,
            generation: 0,
            baseline: true,
            reread: false,
            mint: None,
            stale: false,
            recent: None,
            identity_checked: false,
            status: None,
            retry: None,
            connected: false,
        }
    }
    /// Access replacement drops private work and credentials, keeping the comparison table.
    pub fn forget(&mut self) {
        self.jwt.forget();
        self.read = Read::Idle;
        self.forget_tokens();
        self.reread = false;
        self.status = None;
        self.retry = None;
        self.connected = false;
    }
    pub fn forget_tokens(&mut self) {
        let mut ids = skein_lib::List::with_capacity(self.rows.capacity());
        for (id, _) in &self.rows {
            ids.push(*id).expect("bounded rows");
        }
        for id in ids.into_boxed() {
            self.rows.get_mut(&id).expect("row").forget();
        }
        self.mint = None;
        self.stale = false;
        self.recent = None;
        self.identity_checked = false;
    }
    pub fn accept(&mut self, ids: &[u64]) {
        let cap = self.accepted.capacity();
        let mut accepted = Set::with_capacity(cap);
        for id in ids {
            if *id != 0 {
                let _inserted = accepted.insert(*id);
            }
        }
        if accepted.iter().eq(self.accepted.iter()) {
            return;
        }
        let mut known = Map::with_capacity(cap);
        self.reread = false;
        for id in &accepted {
            let account = match self.rows.get(id) {
                Some(row) => Some(row.account.clone()),
                None => {
                    self.reread = true;
                    match self.known.get(id) {
                        Some(known) => known.known.clone(),
                        None => None,
                    }
                }
            };
            drop(known.insert(*id, Accepted { known: account, checked: false }).expect("accepted bound"));
        }
        self.accepted = accepted;
        self.known = known;
        // Staleness also guards against a generation wrapping while an exchange is written.
        self.generation = self.generation.wrapping_add(1);
        match &mut self.read {
            Read::Reading(reading) => reading.stale = true,
            Read::Waiting { .. } => {
                if !self.reread {
                    self.read = Read::Idle;
                }
            }
            Read::Idle | Read::Changes(_) => {}
        }
        #[expect(clippy::manual_map, reason = "the programming model excludes closures")]
        let mint_id = match self.mint {
            Some((id, _)) => Some(id),
            None => None,
        };
        let mut rows = skein_lib::List::with_capacity(self.rows.capacity());
        for (id, _) in &self.rows {
            rows.push(*id).expect("bounded rows");
        }
        for id in rows.into_boxed() {
            let row = self.rows.get_mut(&id).expect("row");
            row.accepted = self.accepted.contains(&id);
            if !row.accepted || mint_id == Some(id) {
                row.forget();
            }
        }
        if self.mint.is_some() {
            self.stale = true;
        }
        if let Some(id) = self.recent
            && !self.accepted.contains(&id)
        {
            self.recent = None;
        }
    }
    pub fn begin_read(&mut self, next: Duration) {
        self.mint = None;
        self.stale = false;
        self.read = Read::Reading(Reading::new(self.rows.capacity(), self.generation, next));
        self.reread = false;
    }
    pub fn stale_read(&self) -> bool {
        match &self.read {
            Read::Reading(reading) => reading.stale || reading.generation != self.generation,
            Read::Idle | Read::Changes(_) | Read::Waiting { .. } => false,
        }
    }
    pub fn commit(&mut self) {
        let reading = match core::mem::replace(&mut self.read, Read::Idle) {
            Read::Reading(reading) => reading,
            Read::Idle | Read::Changes(_) | Read::Waiting { .. } => unreachable!("completed read"),
        };
        assert!(!reading.stale && reading.generation == self.generation, "current read commits");
        let mut old = Map::with_capacity(self.rows.capacity());
        for (id, row) in &self.rows {
            drop(
                old.insert(
                    *id,
                    Metadata {
                        account: row.account.clone(),
                        suspended: row.suspended,
                        seen: reading.seen.contains(id),
                    },
                )
                .expect("old table bound"),
            );
        }
        let mut missing = Map::with_capacity(self.accepted.capacity());
        let mut accepted = Set::with_capacity(self.accepted.capacity());
        for id in &self.accepted {
            let _inserted = accepted.insert(*id).expect("accepted bound");
            let known = self.known.get_mut(id).expect("accepted bookkeeping");
            if let Some(row) = reading.staged.get(id) {
                known.known = Some(row.account.clone());
            }
            if !reading.staged.contains_key(id) && (self.baseline || !known.checked) {
                drop(missing.insert(*id, known.known.clone()).expect("accepted bound"));
            }
            known.checked = true;
        }
        for (id, _) in &old {
            if !reading.staged.contains_key(id) {
                drop(self.rows.remove(id));
            }
        }
        let mut staged = reading.staged;
        for _ in 0..staged.len() {
            let id = *staged.first().expect("staged row").0;
            let metadata = staged.remove(&id).expect("staged row");
            match self.rows.get_mut(&id) {
                Some(row) => {
                    row.account = metadata.account;
                    row.suspended = metadata.suspended;
                    row.accepted = self.accepted.contains(&id);
                }
                None => {
                    drop(
                        self.rows
                            .insert(
                                id,
                                Row {
                                    account: metadata.account,
                                    suspended: metadata.suspended,
                                    accepted: self.accepted.contains(&id),
                                    api: None,
                                    api_expires: Wall::EPOCH,
                                    git_expires: Wall::EPOCH,
                                },
                            )
                            .expect("selected table bound"),
                    );
                }
            }
        }
        self.read = Read::Changes(Diff { old, accepted, missing, baseline: self.baseline, after: None, renamed: None });
        self.baseline = false;
    }
    #[expect(clippy::manual_map, reason = "the programming model excludes closures")]
    pub fn output(&mut self, forge: u16) -> Option<Event> {
        match &mut self.read {
            Read::Reading(reading) => match reading.fact.take() {
                Some(installation) => Some(Event::Fact { forge, fact: Fact::Unkept { installation } }),
                None => None,
            },
            Read::Changes(diff) => diff.next(&self.rows, forge),
            Read::Idle | Read::Waiting { .. } => None,
        }
    }
    pub fn deadline(&self) -> Option<Time> {
        match self.read {
            Read::Waiting { until, .. } => Some(until),
            Read::Idle | Read::Reading(_) | Read::Changes(_) => None,
        }
    }
}

#[cfg(test)]
mod tests;
