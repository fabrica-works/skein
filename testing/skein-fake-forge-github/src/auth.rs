//! Credentials remain in the layer; only the acting user crosses into the domain.
use crate::{
    installations::{Grant, Scope},
    service::Service,
};

use crate::{
    answers,
    installations::Reach,
    reads,
    service::{ContentType, Outcome, Pending, PendingKind, Request, Response},
};
use alloc::boxed::Box;
use skein_fake_forge_domain::{
    self as fake,
    api::{Answer, Error, Op, Permission, Read},
};
use skein_lib::{List, ReplyTo, Wall};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Access {
    Person(u64),
    Installation { user: u64, grant: Grant },
}
impl Access {
    pub(crate) fn user(self) -> u64 {
        match self {
            Self::Person(user) | Self::Installation { user, .. } => user,
        }
    }
    pub(crate) fn contents(self) -> bool {
        match self {
            Self::Person(_) => false,
            Self::Installation { grant, .. } => grant.scope == Scope::Contents,
        }
    }
    pub(crate) fn reaches(self, service: &Service, repository: u32) -> bool {
        match self {
            Self::Person(_) => true,
            Self::Installation { grant, .. } => service.reaches(&grant, repository),
        }
    }
}

impl Service {
    pub(crate) fn reaches(&self, grant: &Grant, repository: u32) -> bool {
        let Some(row) = self.repositories.get(&repository) else { return false };
        let Some(installation) = self.installations.get(grant.installation) else { return false };
        row.owner_id == installation.account && self.installations.reaches(grant, repository)
    }
    pub(crate) fn access(&self, bearer: &[u8], now: Wall) -> Result<Access, Response> {
        for (secret, user) in &self.tokens {
            if secret.expose() == bearer {
                return Ok(Access::Person(*user));
            }
        }
        let Ok(grant) = self.installations.token(bearer, now) else {
            return Err(answers::unauthorized(&self.limits));
        };
        let Some(installation) = self.installations.get(grant.installation) else {
            return Err(answers::unauthorized(&self.limits));
        };
        if installation.suspended {
            return Err(answers::refusal(403, answers::SUSPENDED, &self.limits));
        }
        Ok(Access::Installation { user: self.bot, grant: *grant })
    }
}
pub(crate) fn empty() -> Response {
    Response { status: 204, content_type: ContentType::Json, link: None, location: None, body: Box::new([]) }
}
pub(crate) fn reachable(service: &Service, page: u32, access: Access, request: &Request<'_>) -> Outcome {
    Outcome::Reply(match access {
        Access::Person(_) => answers::refusal(403, crate::texts::INTEGRATION, &service.limits),
        Access::Installation { grant, .. } => {
            if service.body_fits(request.body) {
                service.reachable(page, &grant, request.target)
            } else {
                answers::refusal(413, b"Payload Too Large", &service.limits)
            }
        }
    })
}
pub(crate) fn edit_reach(
    service: &Service,
    access: Access,
    installation: u64,
    repository: u32,
    add: bool,
    reply_to: ReplyTo,
) -> Outcome {
    let Access::Person(user) = access else {
        return Outcome::Reply(answers::refusal(403, crate::texts::INTEGRATION, &service.limits));
    };
    let Some(row) = service.repositories.get(&repository) else {
        return Outcome::Reply(answers::missing(&service.limits));
    };
    let Some(installed) = service.installations.get(installation) else {
        return Outcome::Reply(answers::missing(&service.limits));
    };
    if installed.account != row.owner_id {
        return Outcome::Reply(answers::missing(&service.limits));
    }
    let Some(path) = reads::full(row) else {
        return Outcome::Reply(answers::failed());
    };
    Outcome::Call(
        Pending { kind: PendingKind::EditReach { installation, repository, add } },
        fake::Event::Call { reply_to, user, repository: path, op: Op::Read(Read::Permission { user }) },
    )
}
pub(crate) fn finish_reach(
    service: &mut Service,
    installation: u64,
    repository: u32,
    add: bool,
    result: Result<Answer, Error>,
) -> Response {
    let Ok(Answer::Permission(Permission::Admin)) = result else {
        return answers::missing(&service.limits);
    };
    let Some(installed) = service.installations.get(installation) else {
        return answers::missing(&service.limits);
    };
    let mut repositories = List::with_capacity(service.limits.installations.listed);
    match &installed.reach {
        Reach::All => {
            if add {
                return empty();
            }
            for (id, row) in &service.repositories {
                if row.owner_id == installed.account && *id != repository && repositories.push(*id).is_err() {
                    return answers::refusal(422, b"Validation Failed", &service.limits);
                }
            }
        }
        Reach::Listed(ids) => {
            for id in ids {
                if *id != repository && repositories.push(*id).is_err() {
                    return answers::refusal(422, b"Validation Failed", &service.limits);
                }
            }
            if add && repositories.push(repository).is_err() {
                return answers::refusal(422, b"Validation Failed", &service.limits);
            }
        }
    }
    if !service.installations.set_reach(installation, Reach::Listed(repositories)) {
        return answers::missing(&service.limits);
    }
    empty()
}

pub(crate) fn check(config: &crate::Config, limits: &crate::Limits) -> Result<(), crate::Problem> {
    if config.tokens.len() > limits.github.inner {
        return Err(crate::Problem::Config);
    }
    for (at, (secret, user)) in config.tokens.iter().enumerate() {
        let mut person = false;
        for account in &config.accounts {
            if account.id == *user && !account.organization {
                person = true;
            }
        }
        if secret.expose().is_empty()
            || secret.expose().len() > usize::try_from(limits.github.string_bytes).expect("u32 fits")
            || !person
        {
            return Err(crate::Problem::Config);
        }
        for (prior, _) in config.tokens.iter().take(at) {
            if prior.expose() == secret.expose() {
                return Err(crate::Problem::Config);
            }
        }
    }
    Ok(())
}
