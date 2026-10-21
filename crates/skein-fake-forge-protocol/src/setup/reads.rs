use super::{Store, absent, collaborators, encoded, full, refusal};
use crate::{Config, Hosted, Limits, Repository, answers, response::Reply, routes};
use skein_fake_forge_domain::api::{self as a, Permission, Read, Settings};
use skein_forge::forgejo::{
    document::{self as d, CollaboratorPermission, User},
    request::{Grant, Repo, Request},
    server::Answer,
};
use skein_lib::bytes;

impl Store {
    pub(super) fn repository_by_id(
        &self,
        id: u64,
        caller: &User,
        config: &Config,
        limits: &Limits,
        hosted: Option<&Hosted<'_>>,
    ) -> Reply {
        let Ok(id) = u32::try_from(id) else { return absent(limits) };
        for repo in &config.repositories {
            if repo.id != id {
                continue;
            }
            let Some(owner) = owner(repo) else { return absent(limits) };
            let permission = self.effective(owner, caller.id, &caller.login, self.direct(id, &caller.login));
            if !caller.is_admin && self.is_private(u64::from(id)) && permission == Permission::None {
                return absent(limits);
            }
            let settings = match hosted {
                Some(hosted) => {
                    let Ok(settings) = hosted.domain.inspect(hosted.config, &repo.full, &Read::Settings) else {
                        return absent(limits);
                    };
                    settings
                }
                None => {
                    // up and resume lend no domain, so setup uses the fixtures' main
                    // and the merge styles a new domain repository enables.
                    a::Answer::Settings(Settings {
                        default: bytes::copy_of(b"main"),
                        merge: true,
                        rebase: true,
                        squash: true,
                    })
                }
            };
            return match answers::read(config, &Request::RepositoryById { id: u64::from(id) }, &settings) {
                Ok(answer) => encoded(200, answer, limits),
                Err(_) => full(limits),
            };
        }
        absent(limits)
    }

    pub(super) fn read_permission(
        &self,
        at: &Repo,
        login: &[u8],
        caller: &User,
        config: &Config,
        limits: &Limits,
    ) -> Reply {
        let Some(repository) = collaborators::repository(config, at) else { return absent(limits) };
        let Some(user) = self.permission_user(config, login) else { return absent(limits) };
        if !caller.is_admin && caller.login.as_ref() != login {
            return refusal(
                403,
                b"Only admins can query all permissions, repo admins can query all repo permissions, collaborators can query only their own",
                limits,
            );
        }
        // Listing::accept adds no permission for a public repository without a grant.
        let effective = self.effective(&at.owner, user.id, login, self.direct(repository, login));
        let permission =
            if self.owns(&at.owner, login) { d::Permission::Owner } else { answers::permission(effective) };
        encoded(200, Answer::Permission(CollaboratorPermission { permission, user }), limits)
    }

    fn direct(&self, repository: u32, login: &[u8]) -> Permission {
        match self.collaborator(repository, login) {
            None => Permission::None,
            Some(Grant::Read) => Permission::Read,
            Some(Grant::Write) => Permission::Write,
            Some(Grant::Admin) => Permission::Admin,
        }
    }

    fn permission_user(&self, config: &Config, login: &[u8]) -> Option<User> {
        if let Some(account) = self.find(login) {
            return self.user(account.id);
        }
        for (id, name) in &config.users {
            if name.as_ref() == login {
                return Some(routes::user(config, *id));
            }
        }
        None
    }
}

fn owner(repo: &Repository) -> Option<&[u8]> {
    repo.full.get(..bytes::find(&repo.full, b"/")?)
}
