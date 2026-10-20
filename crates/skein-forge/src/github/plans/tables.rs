//! Repository rows and user logins, evicted by least recent use (forge.md, 3.5).
use crate::{api, github::document};
use alloc::boxed::Box;
use skein_lib::Map;

/// The first merge method the repository allows (forge-github.md, 5.1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Method {
    Merge,
    Squash,
    Rebase,
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Repository {
    pub owner: Box<[u8]>,
    pub name: Box<[u8]>,
    pub owner_id: u64,
    pub organization: bool,
    /// `None` when no method is allowed; no disabled method is chosen as a default.
    pub merge: Option<Method>,
    pub html_url: Box<[u8]>,
}
#[derive(Debug)]
struct RepositoryEntry {
    row: Repository,
    used: u64,
}
#[derive(Debug)]
struct UserEntry {
    login: Box<[u8]>,
    used: u64,
}
#[derive(Debug)]
pub struct Tables {
    repositories: Map<api::Repository, RepositoryEntry>,
    users: Map<(u16, u64), UserEntry>,
    clock: u64,
}
impl Tables {
    /// Two bounded tables; a zero capacity keeps nothing and returns normally.
    #[must_use]
    pub fn new(repositories: u32, users: u32) -> Self {
        Self { repositories: Map::with_capacity(repositories), users: Map::with_capacity(users), clock: 0 }
    }
    fn tick(&mut self) -> u64 {
        // At clock exhaustion equal ages retain the same deterministic key tie-break.
        self.clock = self.clock.saturating_add(1);
        self.clock
    }
    #[must_use]
    pub fn repository(&mut self, key: api::Repository) -> Option<&Repository> {
        let used = self.tick();
        let entry = self.repositories.get_mut(&key)?;
        entry.used = used;
        Some(&entry.row)
    }
    pub fn keep_repository(&mut self, key: api::Repository, row: Repository) {
        if self.repositories.capacity() == 0 {
            return;
        }
        if !self.repositories.contains_key(&key) && self.repositories.len() == self.repositories.capacity() {
            let mut oldest = None;
            for (key, entry) in &self.repositories {
                let order = (entry.used, *key);
                if oldest.is_none() || Some(order) < oldest {
                    oldest = Some(order);
                }
            }
            if let Some((_, key)) = oldest {
                drop(self.repositories.remove(&key));
            }
        }
        let used = self.tick();
        // An absent key has room after eviction; replacement needs no extra slot.
        drop(self.repositories.insert(key, RepositoryEntry { row, used }));
    }
    pub fn drop_repository(&mut self, key: api::Repository) {
        drop(self.repositories.remove(&key));
    }
    #[must_use]
    pub fn user(&mut self, forge: u16, id: u64) -> Option<&[u8]> {
        let used = self.tick();
        let entry = self.users.get_mut(&(forge, id))?;
        entry.used = used;
        Some(&entry.login)
    }
    pub fn keep_user(&mut self, forge: u16, id: u64, login: Box<[u8]>) {
        if self.users.capacity() == 0 {
            return;
        }
        let key = (forge, id);
        if !self.users.contains_key(&key) && self.users.len() == self.users.capacity() {
            let mut oldest = None;
            for (key, entry) in &self.users {
                let order = (entry.used, *key);
                if oldest.is_none() || Some(order) < oldest {
                    oldest = Some(order);
                }
            }
            if let Some((_, key)) = oldest {
                drop(self.users.remove(&key));
            }
        }
        let used = self.tick();
        drop(self.users.insert(key, UserEntry { login, used }));
    }
}
/// Copies GitHub's routing and owner fields, choosing merge, then squash, then rebase.
#[must_use]
pub fn repository(row: &document::Repository) -> Repository {
    let merge = if row.merge {
        Some(Method::Merge)
    } else if row.squash {
        Some(Method::Squash)
    } else if row.rebase {
        Some(Method::Rebase)
    } else {
        None
    };
    Repository {
        owner: row.owner.clone(),
        name: row.name.clone(),
        owner_id: row.owner_id,
        organization: row.organization,
        merge,
        html_url: row.html_url.clone(),
    }
}
