//! Creates repositories and files in the lent domain, keeping protocol identities bounded.
use super::{Store, absent, encoded, full, joined, refusal};
use crate::{Config, Hosted, Limits, Repository, response::Reply};
use alloc::boxed::Box;
use skein_fake_forge_domain::{
    self as domain,
    api::{Error, Permission},
};
use skein_forge::forgejo::{
    document::CreatedRepository,
    request::{Repo, Request},
    server::Answer,
};
use skein_lib::Env;

impl Store {
    pub(super) fn creation(
        &mut self,
        request: &Request,
        user: u64,
        config: &mut Config,
        env: &Env<Limits>,
        hosted: &mut Hosted<'_>,
    ) -> Option<Reply> {
        if let Request::CreateRepository { org, name, private } = request {
            let limits = &env.limits;
            if !self.resolve_user(config, user).is_admin {
                return Some(refusal(403, b"user should be the site admin", limits));
            }
            let Some(organization) = self.organization(org) else {
                return Some(absent(limits));
            };
            let Some(owner) = self.find(&organization.owner) else {
                return Some(absent(limits));
            };
            let owner = owner.id;
            if !repository_name(name) {
                return Some(refusal(422, b"invalid repository name", limits));
            }
            let full_name = joined(&[org, b"/", name]);
            if hosted.domain.metadata(&full_name).is_ok() {
                return Some(refusal(409, b"repository already exists", limits));
            }
            if full_name.len() > usize::try_from(limits.documents.target_bytes).expect("u32 fits")
                || config.repositories.room() == 0
                || (*private && self.private.room() == 0)
            {
                return Some(full(limits));
            }
            let mut id = 0_u32;
            for repository in &config.repositories {
                id = id.max(repository.id);
            }
            let Some(id) = id.checked_add(1) else {
                return Some(full(limits));
            };
            if let Err(error) = domain::create_repository(hosted.domain, hosted.config, id, &full_name, name, owner) {
                return Some(failed(error, limits));
            }
            if hosted.domain.can_grant(&full_name, user) == Ok(true) {
                domain::grant(hosted.domain, &full_name, user, Permission::Admin);
            }
            config
                .repositories
                .push(Repository { id, full: full_name.clone(), protected: None })
                .expect("identity room checked");
            if *private {
                self.private.push(u64::from(id)).expect("private room checked");
            }
            return Some(encoded(
                201,
                Answer::CreatedRepository(CreatedRepository {
                    id: u64::from(id),
                    full_name: full_name.clone(),
                    default_branch: b"main".as_slice().into(),
                    clone_url: joined(&[&config.base, b"/", &full_name, b".git"]),
                    ssh_url: joined(&[b"ssh://git@localhost/", &full_name, b".git"]),
                }),
                limits,
            ));
        }
        if let Request::CreateFile { at, path, branch, content, message, author } = request {
            let limits = &env.limits;
            let full_name = path_of(at);
            let signature = domain::Signature { name: author.name.clone(), email: author.email.clone() };
            let domain_env = Env { now: env.now, wall: env.wall, limits: *hosted.config };
            let result = domain::create_file(
                hosted.domain,
                &domain_env,
                &full_name,
                branch,
                path,
                content,
                message,
                signature,
                user,
            );
            return Some(match result {
                Ok(commit) => encoded(201, Answer::CreatedFile { commit: crate::routes::object(commit) }, limits),
                Err(Error::Exists) => refusal(422, b"file already exists", limits),
                Err(error) => failed(error, limits),
            });
        }
        None
    }
}
fn path_of(at: &Repo) -> Box<[u8]> {
    joined(&[&at.owner, b"/", &at.name])
}
fn repository_name(name: &[u8]) -> bool {
    if name.is_empty() || name.len() > 100 || name == b"." || name == b".." {
        return false;
    }
    for byte in name {
        if !byte.is_ascii_alphanumeric() && !b"._-".contains(byte) {
            return false;
        }
    }
    true
}
fn failed(error: Error, limits: &Limits) -> Reply {
    let failure = crate::errors::answer(error, crate::Route::CreateFile, skein_lib::Time::ZERO);
    crate::response::failure(limits, failure)
}
