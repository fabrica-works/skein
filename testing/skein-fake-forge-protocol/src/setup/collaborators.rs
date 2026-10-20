use super::{Store, absent, empty, full};
use crate::{Config, Error, Limits, response::Reply};
use alloc::boxed::Box;
use skein_forge::forgejo::request::{Grant, Repo};
use skein_lib::List;
#[derive(Debug)]
pub(super) struct Collaborator {
    repository: u32,
    login: Box<[u8]>,
    permission: Grant,
}
/// Resolves only scenario repositories, keeping one source of repository identity.
pub(super) fn repository(config: &Config, at: &Repo) -> Option<u32> {
    for repo in &config.repositories {
        if let Some(tail) = repo.full.strip_prefix(at.owner.as_ref())
            && tail.strip_prefix(b"/") == Some(at.name.as_ref())
        {
            return Some(repo.id);
        }
    }
    None
}
impl Store {
    /// Inspects a setup grant by scenario repository id without consulting domain permissions.
    #[must_use]
    pub fn collaborator(&self, repository: u32, login: &[u8]) -> Option<Grant> {
        for row in &self.collaborators {
            if row.repository == repository && row.login.as_ref() == login {
                return Some(row.permission);
            }
        }
        None
    }
    pub(super) fn add_collaborator(
        &mut self,
        at: &Repo,
        login: &[u8],
        permission: Grant,
        config: &Config,
        limits: &Limits,
    ) -> Reply {
        let Some(repository) = repository(config, at) else {
            return absent(limits);
        };
        let mut known = self.find(login).is_some();
        for (_, name) in &config.users {
            if name.as_ref() == login {
                known = true;
            }
        }
        if !known {
            return absent(limits);
        }
        match self.grant(repository, login, permission) {
            Ok(()) => empty(),
            Err(Error::Limits) => full(limits),
        }
    }
    /// Sets or replaces a bounded scenario grant without an HTTP exchange.
    pub fn grant(&mut self, repository: u32, login: &[u8], permission: Grant) -> Result<(), Error> {
        if login.len() > self.string_bytes {
            return Err(Error::Limits);
        }
        for index in 0..self.collaborators.len() {
            if let Some(row) = self.collaborators.get_mut(index)
                && row.repository == repository
                && row.login.as_ref() == login
            {
                row.permission = permission;
                return Ok(());
            }
        }
        if self.collaborators.room() == 0 {
            return Err(Error::Limits);
        }
        if self.collaborators.push(Collaborator { repository, login: login.into(), permission }).is_err() {
            return Err(Error::Limits);
        }
        Ok(())
    }
}
pub(super) fn worst_case(limits: &Limits) -> Option<u64> {
    let cap = limits.documents.inner;
    List::<Collaborator>::worst_case(cap)?
        .checked_add(u64::from(cap).checked_mul(u64::from(limits.documents.string_bytes))?)
}
