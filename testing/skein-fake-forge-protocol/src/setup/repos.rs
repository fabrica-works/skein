use super::Store;
use crate::{Config, Error, Limits, Repository, response::Reply};
use alloc::boxed::Box;
use skein_fake_forge_domain::api::Permission;
use skein_forge::forgejo::{
    document::{Access, Reachable},
    request::Page,
};
use skein_json::writer::Encoder;
use skein_lib::List;

impl Store {
    /// Marks scenario privacy independently of repository domain metadata.
    pub fn private(&mut self, repository: u64) -> Result<(), Error> {
        if self.is_private(repository) {
            return Ok(());
        }
        match self.private.push(repository) {
            Ok(()) => Ok(()),
            Err(_) => Err(Error::Limits),
        }
    }
    /// Repositories without scenario privacy metadata are public.
    #[must_use]
    pub fn is_private(&self, repository: u64) -> bool {
        self.private.as_slice().contains(&repository)
    }
    fn login(&self, config: &Config, user: u64) -> Option<Box<[u8]>> {
        if let Some(account) = self.user(user) {
            return Some(account.login);
        }
        for (id, login) in &config.users {
            if *id == user {
                return Some(login.clone());
            }
        }
        None
    }
    fn owner(&self, config: &Config, login: &[u8]) -> Option<u64> {
        if let Some(org) = self.organization(login) {
            return Some(org.id);
        }
        if let Some(account) = self.find(login) {
            return Some(account.id);
        }
        for (id, name) in &config.users {
            if name.as_ref() == login {
                return Some(*id);
            }
        }
        None
    }
    fn effective(&self, owner: &[u8], user: u64, login: &[u8], direct: Permission) -> Permission {
        let mut permission = direct;
        if let Some(org) = self.organization(owner) {
            if org.owner.as_ref() == login {
                permission = Permission::Admin;
            }
            for team in &self.teams {
                // forge-setup.md 4.8: teams without all repositories grant nothing without a team-to-repository table.
                if team.org.as_ref() == owner
                    && team.spec.includes_all_repositories
                    && team.members.as_slice().contains(&user)
                {
                    let grant = match team.spec.permission {
                        Access::None => Permission::None,
                        Access::Read => Permission::Read,
                        Access::Write => Permission::Write,
                        Access::Admin | Access::Owner => Permission::Admin,
                    };
                    permission = permission.max(grant);
                }
            }
        } else if owner == login {
            permission = Permission::Admin;
        }
        permission
    }
}
#[derive(Debug)]
struct Row {
    reachable: Reachable,
    permission: Permission,
}
#[derive(Debug)]
pub(crate) struct Listing {
    pub(crate) current: Option<u32>,
    page: Page,
    seen: u64,
    rows: List<Row>,
    search: bool,
}
impl Listing {
    pub(crate) fn new(page: Page, config: &Config) -> Self {
        let (_, cap) = crate::routes::page(config, page);
        Self { current: None, page, seen: 0, rows: List::with_capacity(cap), search: false }
    }
    pub(crate) fn next<'a>(&self, config: &'a Config) -> Option<&'a Repository> {
        let mut next: Option<&Repository> = None;
        for repo in &config.repositories {
            let after = match self.current {
                Some(id) => repo.id > id,
                None => true,
            };
            let before = match next {
                Some(prior) => repo.id < prior.id,
                None => true,
            };
            if after && before {
                next = Some(repo);
            }
        }
        next
    }
    pub(crate) fn accept(
        &mut self,
        store: &Store,
        config: &Config,
        user: u64,
        permission: Permission,
    ) -> Result<(), Error> {
        let Some(id) = self.current else {
            return Err(Error::Limits);
        };
        for repo in &config.repositories {
            if repo.id != id {
                continue;
            }
            let Some(slash) = skein_lib::bytes::find(&repo.full, b"/") else {
                return Err(Error::Limits);
            };
            let Some(owner) = repo.full.get(..slash) else {
                return Err(Error::Limits);
            };
            let Some(name) = repo.full.get(slash.saturating_add(1)..) else {
                return Err(Error::Limits);
            };
            if owner.len() > store.string_bytes || name.len() > store.string_bytes {
                return Err(Error::Limits);
            }
            let Some(login) = store.login(config, user) else {
                return Err(Error::Limits);
            };
            let permission = store.effective(owner, user, &login, permission);
            if permission == Permission::None {
                return Ok(());
            }
            let (page, cap) = crate::routes::page(config, self.page);
            let start = u64::from(page.saturating_sub(1)).saturating_mul(u64::from(cap));
            if self.seen >= start && self.rows.room() > 0 {
                let Some(owner_id) = store.owner(config, owner) else {
                    return Err(Error::Limits);
                };
                let row = Row {
                    reachable: Reachable {
                        repository: u64::from(id),
                        owner: owner_id,
                        login: owner.into(),
                        name: name.into(),
                        private: store.is_private(u64::from(id)),
                        push: permission >= Permission::Write,
                    },
                    permission,
                };
                if self.rows.push(row).is_err() {
                    return Err(Error::Limits);
                }
            }
            self.seen = self.seen.saturating_add(1);
            return Ok(());
        }
        Err(Error::Limits)
    }
    pub(crate) fn reply(&self, limits: &Limits) -> Reply {
        let mut measure = Encoder::measure(&limits.documents.writer);
        self.write(&mut measure);
        let Ok(length) = measure.measured() else {
            return super::full(limits);
        };
        let mut json = Encoder::write(length, &limits.documents.writer);
        self.write(&mut json);
        Reply { status: 200, headers: Box::from([]), body: json.finish() }
    }
    fn write(&self, json: &mut Encoder) {
        if self.search {
            json.object_start();
            json.key(b"ok");
            json.boolean(true);
            json.key(b"data");
        }
        json.array_start();
        for row in &self.rows {
            let repo = &row.reachable;
            json.object_start();
            json.key(b"id");
            json.unsigned(repo.repository);
            json.key(b"name");
            json.string(&repo.name);
            json.key(b"private");
            json.boolean(repo.private);
            json.key(b"owner");
            json.object_start();
            json.key(b"id");
            json.unsigned(repo.owner);
            json.key(b"login");
            json.string(&repo.login);
            json.object_end();
            if self.search {
                json.key(b"full_name");
                json.string_parts(&[&repo.login, b"/", &repo.name]);
            }
            json.key(b"permissions");
            json.object_start();
            json.key(b"push");
            json.boolean(repo.push);
            json.key(b"pull");
            json.boolean(row.permission >= Permission::Read);
            json.key(b"admin");
            json.boolean(row.permission == Permission::Admin);
            json.object_end();
            json.object_end();
        }
        json.array_end();
        if self.search {
            json.object_end();
        }
    }
}

impl Store {
    /// Admins see every repository; other users see public and personally owned repositories, without collaborator or team grants.
    pub(crate) fn search(&self, config: &Config, user: u64, page: Page, limits: &Limits) -> Reply {
        match self.search_page(config, user, page) {
            Ok(listing) => listing.reply(limits),
            Err(Error::Limits) => super::full(limits),
        }
    }
    fn search_page(&self, config: &Config, user: u64, page: Page) -> Result<Listing, Error> {
        let admin = match self.user(user) {
            Some(account) => account.is_admin,
            None => false,
        };
        let login = self.login(config, user).ok_or(Error::Limits)?;
        let mut listing = Listing::new(page, config);
        listing.search = true;
        let (page, cap) = crate::routes::page(config, page);
        let start = u64::from(page.saturating_sub(1)).saturating_mul(u64::from(cap));
        while let Some(repo) = listing.next(config) {
            listing.current = Some(repo.id);
            let slash = skein_lib::bytes::find(&repo.full, b"/").ok_or(Error::Limits)?;
            let owner = repo.full.get(..slash).ok_or(Error::Limits)?;
            let name = repo.full.get(slash.saturating_add(1)..).ok_or(Error::Limits)?;
            let private = self.is_private(u64::from(repo.id));
            if !admin && private && owner != login.as_ref() {
                continue;
            }
            if listing.seen >= start && listing.rows.room() > 0 {
                if owner.len() > self.string_bytes || name.len() > self.string_bytes {
                    return Err(Error::Limits);
                }
                let owner_id = self.owner(config, owner).ok_or(Error::Limits)?;
                let row = Row {
                    reachable: Reachable {
                        repository: u64::from(repo.id),
                        owner: owner_id,
                        login: owner.into(),
                        name: name.into(),
                        private,
                        push: true,
                    },
                    permission: Permission::Admin,
                };
                if listing.rows.push(row).is_err() {
                    return Err(Error::Limits);
                }
            }
            listing.seen = listing.seen.saturating_add(1);
        }
        Ok(listing)
    }
}
