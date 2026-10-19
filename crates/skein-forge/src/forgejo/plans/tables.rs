//! Repository paths and user logins, evicted by least recent use (forge.md, 3.5).
use crate::api;
use crate::forgejo::{document::MergeStyle, id::ObjectFormat};
use alloc::boxed::Box;
use skein_lib::Map;
/// The repository fields needed to address calls and read their answers.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Repository {
    pub owner: Box<[u8]>,
    pub name: Box<[u8]>,
    pub object_format: ObjectFormat,
    pub merge_style: MergeStyle,
    pub html_url: Box<[u8]>,
}
#[derive(Debug)]
pub struct RepositoryEntry {
    pub row: Repository,
    pub used: u64,
}
#[derive(Debug)]
pub struct UserEntry {
    pub login: Box<[u8]>,
    pub used: u64,
}
/// The two bounded lookup tables. Forge starts hold their page caps beside settings' tokens.
#[derive(Debug)]
pub struct Tables {
    repositories: Map<api::Repository, RepositoryEntry>,
    users: Map<(u16, u64), UserEntry>,
    clock: u64,
}
impl Tables {
    #[must_use]
    pub fn new(repositories: u32, users: u32) -> Self {
        Self { repositories: Map::with_capacity(repositories), users: Map::with_capacity(users), clock: 0 }
    }
    fn tick(&mut self) -> u64 {
        self.clock = self.clock.checked_add(1).expect("fewer than u64::MAX table uses in a deployment");
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
        drop(self.repositories.insert(key, RepositoryEntry { row, used }).expect("room after eviction"));
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
        drop(self.users.insert(key, UserEntry { login, used }).expect("room after eviction"));
    }
}
